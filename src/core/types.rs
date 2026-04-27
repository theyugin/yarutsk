// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::sync::Arc;

use indexmap::IndexMap;

/// Document-level metadata for one YAML document.
///
/// Lives on every node (held inside `PyYamlNode`); only the document root's
/// values are honoured at emit time. Produced by the builder per parsed
/// document and consumed by the emitter to render the right directives /
/// markers.
#[derive(Debug, Default, Clone)]
pub struct DocMetadata {
    /// Whether the doc had an explicit `---` marker.
    pub explicit_start: bool,
    /// Whether the doc had an explicit `...` end marker.
    pub explicit_end: bool,
    /// `%YAML major.minor` directive, if present.
    pub yaml_version: Option<(u8, u8)>,
    /// `%TAG handle prefix` pairs (empty if none).
    pub tag_directives: Vec<(String, String)>,
}

/// Key into [`YamlMapping::entries`]. Scalar keys hold their string form;
/// complex (non-scalar) keys carry only a positional id — the actual key
/// node lives on [`YamlEntry::key_node`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MapKey {
    Scalar(String),
    Complex(usize),
}

impl MapKey {
    /// Construct a scalar key from any string-like value.
    pub fn scalar<S: Into<String>>(s: S) -> Self {
        MapKey::Scalar(s.into())
    }

    /// Borrowed access to the scalar key's string. `None` for complex keys.
    #[must_use]
    pub fn as_scalar(&self) -> Option<&str> {
        match self {
            MapKey::Scalar(s) => Some(s),
            MapKey::Complex(_) => None,
        }
    }

    /// `true` iff this is a complex (non-scalar) key.
    #[must_use]
    pub fn is_complex(&self) -> bool {
        matches!(self, MapKey::Complex(_))
    }

    /// String surfaced to the Python `dict` view. Scalar keys map to their
    /// own string; complex keys get a synthetic `\x00<n>` placeholder so the
    /// dict has a unique key for each entry. The placeholder is *only* a
    /// Python-side artifact — internal lookups use the typed enum.
    #[must_use]
    pub fn python_key(&self) -> String {
        match self {
            MapKey::Scalar(s) => s.clone(),
            MapKey::Complex(n) => format!("\x00{n}"),
        }
    }
}

impl From<String> for MapKey {
    fn from(s: String) -> Self {
        MapKey::Scalar(s)
    }
}

impl From<&str> for MapKey {
    fn from(s: &str) -> Self {
        MapKey::Scalar(s.to_owned())
    }
}

/// Metadata shared by every concrete node variant: cosmetic comments/blank-lines
/// plus semantic tag/anchor. Lives in one place so adding a field touches one
/// struct, not four.
///
/// `Alias` populates only the comment/blank-line fields — YAML aliases (`*name`)
/// cannot carry their own tag or anchor (those belong to the anchored node).
#[derive(Debug, Clone, Default)]
pub struct NodeMeta {
    /// Trailing `# comment` on the node's line.
    pub comment_inline: Option<String>,
    /// Block `# comment` lines rendered immediately above this node.
    pub comment_before: Option<String>,
    /// Blank lines in the source before this node (capped at 255).
    pub blank_lines_before: u8,
    /// Optional YAML tag (e.g. `"!!str"`, `"!python/tuple"`). Always `None` for `Alias`.
    pub tag: Option<String>,
    /// Anchor name declared on this node (`&name`). Always `None` for `Alias`.
    pub anchor: Option<String>,
}

/// Pure Rust data model produced by the parser/builder and consumed by the
/// emitter. Holds no `Py` references — Python identity sharing lives in the
/// bridge layer's `LiveNode` (see [`crate::py::live`]).
///
/// `Null` is represented as a `YamlScalar` whose value is `ScalarValue::Null`;
/// see [`YamlScalar::null`].
#[derive(Debug, Clone)]
pub enum YamlNode {
    Mapping(YamlMapping<YamlNode>),
    Sequence(YamlSequence<YamlNode>),
    Scalar(YamlScalar),
    /// An alias node (`*name`). `resolved` is a shared reference to the
    /// anchor's value — multiple aliases for the same anchor share storage.
    /// Identity-sharing for the Python wrapper is layered on top in
    /// `LiveNode::Alias`, which adds a `materialised: Option<Py>` cache.
    Alias {
        name: String,
        resolved: Arc<YamlNode>,
        meta: NodeMeta,
    },
}

/// Generate paired getter/setter on `YamlNode` that delegate to `meta.<field>` on
/// every variant (all four carry a `NodeMeta`).
macro_rules! node_accessor {
    // Option<String> field: getter returns Option<&str>, setter takes Option<String>.
    ($field:ident, $get:ident, $set:ident, $doc:literal, optstr) => {
        #[doc = $doc]
        #[must_use]
        pub fn $get(&self) -> Option<&str> {
            match self {
                YamlNode::Mapping(m) => m.meta.$field.as_deref(),
                YamlNode::Sequence(s) => s.meta.$field.as_deref(),
                YamlNode::Scalar(s) => s.meta.$field.as_deref(),
                YamlNode::Alias { meta, .. } => meta.$field.as_deref(),
            }
        }

        pub fn $set(&mut self, value: Option<String>) {
            match self {
                YamlNode::Mapping(m) => m.meta.$field = value,
                YamlNode::Sequence(s) => s.meta.$field = value,
                YamlNode::Scalar(s) => s.meta.$field = value,
                YamlNode::Alias { meta, .. } => meta.$field = value,
            }
        }
    };
    // Copy field: getter returns the value, setter takes the value.
    ($field:ident, $get:ident, $set:ident, $doc:literal, copy $ty:ty) => {
        #[doc = $doc]
        #[must_use]
        pub fn $get(&self) -> $ty {
            match self {
                YamlNode::Mapping(m) => m.meta.$field,
                YamlNode::Sequence(s) => s.meta.$field,
                YamlNode::Scalar(s) => s.meta.$field,
                YamlNode::Alias { meta, .. } => meta.$field,
            }
        }

        pub fn $set(&mut self, value: $ty) {
            match self {
                YamlNode::Mapping(m) => m.meta.$field = value,
                YamlNode::Sequence(s) => s.meta.$field = value,
                YamlNode::Scalar(s) => s.meta.$field = value,
                YamlNode::Alias { meta, .. } => meta.$field = value,
            }
        }
    };
}

impl YamlNode {
    node_accessor!(
        comment_inline,
        comment_inline,
        set_comment_inline,
        "Read the trailing inline comment on this node, if any.",
        optstr
    );
    node_accessor!(
        comment_before,
        comment_before,
        set_comment_before,
        "Read the block comment rendered above this node, if any.",
        optstr
    );
    node_accessor!(
        blank_lines_before,
        blank_lines_before,
        set_blank_lines_before,
        "Read the number of blank lines preceding this node in its parent.",
        copy u8
    );
}

/// How a scalar value was written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScalarStyle {
    #[default]
    Plain,
    SingleQuoted,
    DoubleQuoted,
    /// Literal block scalar (`|`).
    Literal,
    /// Folded block scalar (`>`).
    Folded,
}

/// Whether a mapping or sequence used flow (`{…}`/`[…]`) or block (`key:`/`- `) style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContainerStyle {
    Block,
    Flow,
}

/// Chomping indicator for block scalars (`|` / `>`).
///
/// Mirrors `scanner::Chomping` at the data-model layer, so the builder
/// can carry the scanner's chomping through without leaking scanner
/// internals into the emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chomping {
    /// `-` — strip: remove the final line break and any trailing empty lines.
    Strip,
    /// No indicator — clip: keep exactly one final line break.
    Clip,
    /// `+` — keep: preserve the final line break and any trailing empty lines.
    Keep,
}

#[derive(Debug, Clone)]
pub struct YamlScalar {
    /// The typed value of the scalar.
    pub value: ScalarValue,
    /// Original source text, kept when canonical re-emission of `value`
    /// would change the spelling (e.g. `1.5e10` → `15000000000.0`,
    /// `yes` → `true`, `0xFF` → `255`). The emitter prefers `source` over a
    /// canonical re-emission of `value`. Cleared on any value mutation —
    /// see [`YamlScalar::set_value`].
    pub source: Option<String>,
    /// The quoting style used in the source (or `Plain` for newly constructed scalars).
    pub style: ScalarStyle,
    /// Source chomping indicator for block scalars (`|-`/`|`/`|+` and
    /// `>-`/`>`/`>+`). `None` for non-block scalars and new constructions.
    /// When present and consistent with the value's trailing-newline count,
    /// the emitter uses this instead of re-inferring — so `>+` on a value
    /// with exactly one trailing `\n` round-trips as `>+`, not `>`. Cleared
    /// on any value mutation.
    pub chomping: Option<Chomping>,
    pub meta: NodeMeta,
}

impl YamlScalar {
    /// Read the typed value.
    #[must_use]
    pub fn value(&self) -> &ScalarValue {
        &self.value
    }

    /// Read the preserved source spelling, if any.
    #[must_use]
    pub fn original(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Replace the value, dropping any preserved source. Also clears
    /// `chomping` since block-scalar indicators are tied to the source text.
    pub fn set_value(&mut self, v: ScalarValue) {
        self.value = v;
        self.source = None;
        self.chomping = None;
    }

    /// A plain `null` scalar with default metadata. Used in lieu of the old
    /// `YamlNode::Null` variant.
    #[must_use]
    pub fn null() -> Self {
        YamlScalar {
            value: ScalarValue::Null,
            source: None,
            style: ScalarStyle::Plain,
            chomping: None,
            meta: NodeMeta::default(),
        }
    }

    /// `true` iff the underlying value is `ScalarValue::Null`.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self.value(), ScalarValue::Null)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Null,
}

impl ScalarValue {
    /// Convert to the string used as a Python dict key for this scalar.
    #[must_use]
    pub fn to_key_string(&self) -> String {
        match self {
            ScalarValue::Null => String::new(),
            ScalarValue::Bool(b) => b.to_string(),
            ScalarValue::Int(n) => n.to_string(),
            ScalarValue::Float(f) => f.to_string(),
            ScalarValue::Str(s) => s.clone(),
        }
    }

    /// Parse a raw YAML scalar string into a typed value.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn from_str(s: &str) -> ScalarValue {
        if matches!(s, "" | "null" | "Null" | "NULL" | "~") {
            return ScalarValue::Null;
        }
        if matches!(
            s,
            "true" | "True" | "TRUE" | "yes" | "Yes" | "YES" | "on" | "On" | "ON"
        ) {
            return ScalarValue::Bool(true);
        }
        if matches!(
            s,
            "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off" | "OFF"
        ) {
            return ScalarValue::Bool(false);
        }
        // Integer: decimal, octal (0o), hex (0x)
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
            && let Ok(n) = i64::from_str_radix(hex, 16)
        {
            return ScalarValue::Int(n);
        }
        if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O"))
            && let Ok(n) = i64::from_str_radix(oct, 8)
        {
            return ScalarValue::Int(n);
        }
        if let Ok(n) = s.parse::<i64>() {
            return ScalarValue::Int(n);
        }
        // Float
        if matches!(s, ".inf" | ".Inf" | ".INF") {
            return ScalarValue::Float(f64::INFINITY);
        }
        if matches!(s, "-.inf" | "-.Inf" | "-.INF") {
            return ScalarValue::Float(f64::NEG_INFINITY);
        }
        if matches!(s, ".nan" | ".NaN" | ".NAN") {
            return ScalarValue::Float(f64::NAN);
        }
        if let Ok(f) = s.parse::<f64>()
            && (s.contains('.') || s.contains('e') || s.contains('E'))
        {
            return ScalarValue::Float(f);
        }
        ScalarValue::Str(s.to_owned())
    }
}

/// Anything that can fill a `YamlEntry::value` or `YamlSequence::items` slot.
///
/// Two implementors:
/// - [`YamlNode`] — the pure Rust data model used by the parser/emitter.
/// - `py::live::LiveNode` — the bridge type that adds `Py`-bearing variants
///   for identity-shared Python wrappers and materialised aliases.
///
/// Genericising the containers over this trait keeps `core` Py-free while
/// letting the same `YamlMapping` / `YamlSequence` / `YamlEntry` shapes carry
/// either the parsed tree (builder output) or the live tree (pyclass `inner`).
pub trait Node: Clone {
    fn comment_inline(&self) -> Option<&str>;
    fn set_comment_inline(&mut self, value: Option<String>);
    fn comment_before(&self) -> Option<&str>;
    fn set_comment_before(&mut self, value: Option<String>);
    fn blank_lines_before(&self) -> u8;
    fn set_blank_lines_before(&mut self, value: u8);
    fn format_with(&mut self, opts: FormatOptions);
}

#[derive(Debug, Clone)]
pub struct YamlMapping<N: Node = YamlNode> {
    pub entries: IndexMap<MapKey, YamlEntry<N>>,
    /// Block (`key: value`) or flow (`{key: value}`) style.
    pub style: ContainerStyle,
    /// Blank lines at the end of this mapping before the closing context (capped at 255).
    pub trailing_blank_lines: u8,
    pub meta: NodeMeta,
}

impl<N: Node> YamlMapping<N> {
    #[must_use]
    pub fn new() -> Self {
        YamlMapping {
            entries: IndexMap::new(),
            style: ContainerStyle::Block,
            trailing_blank_lines: 0,
            meta: NodeMeta::default(),
        }
    }

    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        YamlMapping {
            entries: IndexMap::with_capacity(n),
            style: ContainerStyle::Block,
            trailing_blank_lines: 0,
            meta: NodeMeta::default(),
        }
    }
}

impl<N: Node> Default for YamlMapping<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct YamlEntry<N: Node = YamlNode> {
    pub value: N,
    /// The quoting style the key was written with in the source.
    pub key_style: ScalarStyle,
    /// Anchor declared on the key scalar (`&name`), if any.
    pub key_anchor: Option<String>,
    /// If the key was written as an alias (`*name:`), the alias name.
    pub key_alias: Option<String>,
    /// Tag on the key scalar (e.g. `!!str`), if any.
    pub key_tag: Option<String>,
    /// For complex (non-scalar) keys: the original key node. Keys are always
    /// pure (they never become `Container`/`OpaquePy`), so this is `YamlNode`.
    pub key_node: Option<Box<YamlNode>>,
}

#[derive(Debug, Clone)]
pub struct YamlSequence<N: Node = YamlNode> {
    pub items: Vec<N>,
    /// Block (`- item`) or flow (`[item]`) style.
    pub style: ContainerStyle,
    /// Blank lines at the end of this sequence before the closing context (capped at 255).
    pub trailing_blank_lines: u8,
    pub meta: NodeMeta,
}

impl<N: Node> YamlSequence<N> {
    #[must_use]
    pub fn new() -> Self {
        YamlSequence {
            items: Vec::new(),
            style: ContainerStyle::Block,
            trailing_blank_lines: 0,
            meta: NodeMeta::default(),
        }
    }

    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        YamlSequence {
            items: Vec::with_capacity(n),
            style: ContainerStyle::Block,
            trailing_blank_lines: 0,
            meta: NodeMeta::default(),
        }
    }
}

impl<N: Node> Default for YamlSequence<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Controls which cosmetic fields are reset by `format_with()`.
#[derive(Clone, Copy)]
pub struct FormatOptions {
    /// Reset scalar quoting → plain (literal for multi-line), container style → block,
    /// and clear `original` so non-canonical forms (hex, exponent) emit canonically.
    pub styles: bool,
    /// Clear `comment_before` and `comment_inline` on every node.
    pub comments: bool,
    /// Zero `blank_lines_before` on every entry/item and `trailing_blank_lines` on containers.
    pub blank_lines: bool,
}

/// Apply `comments`/`blank_lines` resets to a `NodeMeta`. `tag`/`anchor` are
/// semantic and always preserved.
pub fn meta_format_with(meta: &mut NodeMeta, opts: FormatOptions) {
    if opts.comments {
        meta.comment_inline = None;
        meta.comment_before = None;
    }
    if opts.blank_lines {
        meta.blank_lines_before = 0;
    }
}

impl YamlScalar {
    pub fn format_with(&mut self, opts: FormatOptions) {
        if opts.styles {
            // Strings with embedded newlines get literal block style so the emitter
            // doesn't fall back to double-quoted with \n escape sequences.
            let is_multiline = matches!(self.value(), ScalarValue::Str(s) if s.contains('\n'));
            self.style = if is_multiline {
                ScalarStyle::Literal
            } else {
                ScalarStyle::Plain
            };
            // Drop any preserved source so non-canonical forms (hex, exponent)
            // re-emit canonically. Also clears chomping (tied to source).
            self.source = None;
            self.chomping = None;
        }
        meta_format_with(&mut self.meta, opts);
    }
}

impl<N: Node> YamlMapping<N> {
    pub fn format_with(&mut self, opts: FormatOptions) {
        if opts.styles {
            self.style = ContainerStyle::Block;
        }
        meta_format_with(&mut self.meta, opts);
        if opts.blank_lines {
            self.trailing_blank_lines = 0;
        }
        for entry in self.entries.values_mut() {
            if opts.styles {
                entry.key_style = ScalarStyle::Plain;
            }
            // key_tag, key_anchor, key_alias are semantic — preserved.
            if let Some(kn) = &mut entry.key_node {
                kn.format_with(opts);
            }
            entry.value.format_with(opts);
        }
    }
}

impl<N: Node> YamlSequence<N> {
    pub fn format_with(&mut self, opts: FormatOptions) {
        if opts.styles {
            self.style = ContainerStyle::Block;
        }
        meta_format_with(&mut self.meta, opts);
        if opts.blank_lines {
            self.trailing_blank_lines = 0;
        }
        for item in &mut self.items {
            item.format_with(opts);
        }
    }
}

impl YamlNode {
    pub fn format_with(&mut self, opts: FormatOptions) {
        match self {
            YamlNode::Mapping(m) => m.format_with(opts),
            YamlNode::Sequence(s) => s.format_with(opts),
            YamlNode::Scalar(s) => s.format_with(opts),
            YamlNode::Alias { meta, .. } => meta_format_with(meta, opts),
        }
    }
}

impl Node for YamlNode {
    fn comment_inline(&self) -> Option<&str> {
        YamlNode::comment_inline(self)
    }
    fn set_comment_inline(&mut self, value: Option<String>) {
        YamlNode::set_comment_inline(self, value);
    }
    fn comment_before(&self) -> Option<&str> {
        YamlNode::comment_before(self)
    }
    fn set_comment_before(&mut self, value: Option<String>) {
        YamlNode::set_comment_before(self, value);
    }
    fn blank_lines_before(&self) -> u8 {
        YamlNode::blank_lines_before(self)
    }
    fn set_blank_lines_before(&mut self, value: u8) {
        YamlNode::set_blank_lines_before(self, value);
    }
    fn format_with(&mut self, opts: FormatOptions) {
        YamlNode::format_with(self, opts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_null_variants() {
        for s in &["", "null", "Null", "NULL", "~"] {
            assert!(
                matches!(ScalarValue::from_str(s), ScalarValue::Null),
                "expected Null for {s:?}"
            );
        }
    }

    #[test]
    fn from_str_bool_true_variants() {
        for s in &[
            "true", "True", "TRUE", "yes", "Yes", "YES", "on", "On", "ON",
        ] {
            assert!(
                matches!(ScalarValue::from_str(s), ScalarValue::Bool(true)),
                "expected Bool(true) for {s:?}"
            );
        }
    }

    #[test]
    fn from_str_bool_false_variants() {
        for s in &[
            "false", "False", "FALSE", "no", "No", "NO", "off", "Off", "OFF",
        ] {
            assert!(
                matches!(ScalarValue::from_str(s), ScalarValue::Bool(false)),
                "expected Bool(false) for {s:?}"
            );
        }
    }

    #[test]
    fn from_str_integer_decimal() {
        assert!(matches!(ScalarValue::from_str("0"), ScalarValue::Int(0)));
        assert!(matches!(ScalarValue::from_str("42"), ScalarValue::Int(42)));
        assert!(matches!(ScalarValue::from_str("-1"), ScalarValue::Int(-1)));
        assert!(matches!(
            ScalarValue::from_str("9223372036854775807"),
            ScalarValue::Int(i64::MAX)
        ));
        assert!(matches!(
            ScalarValue::from_str("-9223372036854775808"),
            ScalarValue::Int(i64::MIN)
        ));
    }

    #[test]
    fn from_str_integer_hex() {
        assert!(matches!(
            ScalarValue::from_str("0xFF"),
            ScalarValue::Int(255)
        ));
        assert!(matches!(
            ScalarValue::from_str("0XFF"),
            ScalarValue::Int(255)
        ));
        assert!(matches!(ScalarValue::from_str("0x0"), ScalarValue::Int(0)));
        assert!(matches!(
            ScalarValue::from_str("0xDEAD"),
            ScalarValue::Int(0xDEAD)
        ));
        assert!(matches!(
            ScalarValue::from_str("0x7fffffffffffffff"),
            ScalarValue::Int(i64::MAX)
        ));
    }

    #[test]
    fn from_str_integer_octal() {
        assert!(matches!(
            ScalarValue::from_str("0o77"),
            ScalarValue::Int(63)
        ));
        assert!(matches!(
            ScalarValue::from_str("0O77"),
            ScalarValue::Int(63)
        ));
        assert!(matches!(ScalarValue::from_str("0o0"), ScalarValue::Int(0)));
        assert!(matches!(
            ScalarValue::from_str("0o777"),
            ScalarValue::Int(0o777)
        ));
    }

    #[test]
    fn from_str_integer_overflow_falls_back_to_str() {
        // One past i64::MAX — parse::<i64> fails, from_str_radix won't be tried
        assert!(matches!(
            ScalarValue::from_str("9223372036854775808"),
            ScalarValue::Str(_)
        ));
    }

    #[test]
    fn from_str_float_decimal() {
        assert!(matches!(
            ScalarValue::from_str("1.5"),
            ScalarValue::Float(_)
        ));
        assert!(matches!(
            ScalarValue::from_str("0.0"),
            ScalarValue::Float(_)
        ));
        assert!(matches!(
            ScalarValue::from_str("-1.5"),
            ScalarValue::Float(_)
        ));
        assert!(matches!(ScalarValue::from_str(".5"), ScalarValue::Float(_)));
        // Whole-number float: must have trailing dot to be a float
        assert!(matches!(ScalarValue::from_str("1."), ScalarValue::Float(_)));
    }

    #[test]
    fn from_str_float_exponent() {
        assert!(matches!(
            ScalarValue::from_str("1e5"),
            ScalarValue::Float(_)
        ));
        assert!(matches!(
            ScalarValue::from_str("1E5"),
            ScalarValue::Float(_)
        ));
        assert!(matches!(
            ScalarValue::from_str("1.5e-3"),
            ScalarValue::Float(_)
        ));
        assert!(matches!(
            ScalarValue::from_str("1.5E+10"),
            ScalarValue::Float(_)
        ));
    }

    #[test]
    fn from_str_float_infinity() {
        assert!(matches!(
            ScalarValue::from_str(".inf"),
            ScalarValue::Float(f) if f.is_infinite() && f > 0.0
        ));
        assert!(matches!(
            ScalarValue::from_str(".Inf"),
            ScalarValue::Float(f) if f.is_infinite() && f > 0.0
        ));
        assert!(matches!(
            ScalarValue::from_str(".INF"),
            ScalarValue::Float(f) if f.is_infinite() && f > 0.0
        ));
        assert!(matches!(
            ScalarValue::from_str("-.inf"),
            ScalarValue::Float(f) if f.is_infinite() && f < 0.0
        ));
        assert!(matches!(
            ScalarValue::from_str("-.INF"),
            ScalarValue::Float(f) if f.is_infinite() && f < 0.0
        ));
    }

    #[test]
    fn from_str_float_nan() {
        assert!(matches!(
            ScalarValue::from_str(".nan"),
            ScalarValue::Float(f) if f.is_nan()
        ));
        assert!(matches!(
            ScalarValue::from_str(".NaN"),
            ScalarValue::Float(f) if f.is_nan()
        ));
        assert!(matches!(
            ScalarValue::from_str(".NAN"),
            ScalarValue::Float(f) if f.is_nan()
        ));
    }

    #[test]
    fn from_str_float_requires_dot_or_e() {
        // A bare integer-looking string without . or e is not a float
        // "1" → Int, not Float
        assert!(matches!(ScalarValue::from_str("1"), ScalarValue::Int(1)));
    }

    #[test]
    fn from_str_string_fallback() {
        for s in &["hello", "world", "YAML", "not-a-bool", "1.2.3", "v1.0"] {
            assert!(
                matches!(ScalarValue::from_str(s), ScalarValue::Str(_)),
                "expected Str for {s:?}"
            );
        }
    }

    #[test]
    fn from_str_invalid_hex_prefix_only_is_str() {
        // "0x" with no digits — from_str_radix("", 16) fails → falls through to Str
        assert!(matches!(ScalarValue::from_str("0x"), ScalarValue::Str(_)));
        assert!(matches!(ScalarValue::from_str("0X"), ScalarValue::Str(_)));
    }

    #[test]
    fn from_str_invalid_octal_digit_is_str() {
        // '8' and '9' are not valid octal digits
        assert!(matches!(ScalarValue::from_str("0o8"), ScalarValue::Str(_)));
        assert!(matches!(ScalarValue::from_str("0o9"), ScalarValue::Str(_)));
    }

    #[test]
    fn from_str_underscore_integer_is_str() {
        // Rust's parse::<i64>() does not accept underscores, so these stay as Str.
        // The emitter preserves the original source text so round-trip still works.
        assert!(matches!(
            ScalarValue::from_str("1_000"),
            ScalarValue::Str(_)
        ));
        assert!(matches!(
            ScalarValue::from_str("1_000_000"),
            ScalarValue::Str(_)
        ));
    }

    #[test]
    fn from_str_partial_bool_lookalike_is_str() {
        // Mixed-case forms that don't match the recognised list
        assert!(matches!(ScalarValue::from_str("TrUe"), ScalarValue::Str(_)));
        assert!(matches!(
            ScalarValue::from_str("fAlSe"),
            ScalarValue::Str(_)
        ));
        assert!(matches!(ScalarValue::from_str("YeS"), ScalarValue::Str(_)));
        assert!(matches!(ScalarValue::from_str("nUlL"), ScalarValue::Str(_)));
    }

    #[test]
    fn from_str_partial_null_lookalike_is_str() {
        assert!(matches!(
            ScalarValue::from_str("nulll"),
            ScalarValue::Str(_)
        ));
        assert!(matches!(ScalarValue::from_str("Nul"), ScalarValue::Str(_)));
        assert!(matches!(ScalarValue::from_str("~~"), ScalarValue::Str(_)));
    }
}
