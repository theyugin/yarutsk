// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub enum YamlNode {
    Mapping(YamlMapping),
    Sequence(YamlSequence),
    Scalar(YamlScalar),
    Null,
    /// An alias node (`*name`).  `resolved` holds the expanded value so the
    /// Python-visible layer can return a normal value; `name` is preserved for
    /// round-trip emission as `*name`.
    Alias {
        name: String,
        resolved: Box<YamlNode>,
    },
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

#[derive(Debug, Clone)]
pub struct YamlScalar {
    pub value: ScalarValue,
    /// The quoting style used in the source (or `Plain` for newly constructed scalars).
    pub style: ScalarStyle,
    /// Optional YAML tag (e.g. `"!!str"`, `"!python/tuple"`).
    pub tag: Option<String>,
    /// Original source text preserved for scalars where formatting matters
    /// (e.g. floats written in exponent form: `1.5e10`).
    pub original: Option<String>,
    /// Anchor name declared on this scalar (`&name`), if any.
    pub anchor: Option<String>,
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

#[derive(Debug, Clone)]
pub struct YamlMapping {
    pub entries: IndexMap<String, YamlEntry>,
    /// Block (`key: value`) or flow (`{key: value}`) style.
    pub style: ContainerStyle,
    /// Optional YAML tag.
    pub tag: Option<String>,
    /// Blank lines at the end of this mapping before the closing context (capped at 255).
    pub trailing_blank_lines: u8,
    /// Anchor name declared on this mapping (`&name`), if any.
    pub anchor: Option<String>,
}

impl YamlMapping {
    #[must_use]
    pub fn new() -> Self {
        YamlMapping {
            entries: IndexMap::new(),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
            anchor: None,
        }
    }

    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        YamlMapping {
            entries: IndexMap::with_capacity(n),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
            anchor: None,
        }
    }
}

impl Default for YamlMapping {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct YamlEntry {
    pub value: YamlNode,
    pub comment_before: Option<String>,
    pub comment_inline: Option<String>,
    /// Blank lines in the source before this entry (capped at 255).
    pub blank_lines_before: u8,
    /// The quoting style the key was written with in the source.
    pub key_style: ScalarStyle,
    /// Anchor declared on the key scalar (`&name`), if any.
    pub key_anchor: Option<String>,
    /// If the key was written as an alias (`*name:`), the alias name.
    pub key_alias: Option<String>,
    /// Tag on the key scalar (e.g. `!!str`), if any.
    pub key_tag: Option<String>,
    /// For complex (non-scalar) keys: the original key node.
    /// When set, the string key in the `IndexMap` is a synthetic placeholder.
    pub key_node: Option<Box<YamlNode>>,
}

#[derive(Debug, Clone)]
pub struct YamlSequence {
    pub items: Vec<YamlItem>,
    /// Block (`- item`) or flow (`[item]`) style.
    pub style: ContainerStyle,
    /// Optional YAML tag.
    pub tag: Option<String>,
    /// Blank lines at the end of this sequence before the closing context (capped at 255).
    pub trailing_blank_lines: u8,
    /// Anchor name declared on this sequence (`&name`), if any.
    pub anchor: Option<String>,
}

impl YamlSequence {
    #[must_use]
    pub fn new() -> Self {
        YamlSequence {
            items: Vec::new(),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
            anchor: None,
        }
    }

    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        YamlSequence {
            items: Vec::with_capacity(n),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
            anchor: None,
        }
    }
}

impl Default for YamlSequence {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct YamlItem {
    pub value: YamlNode,
    pub comment_before: Option<String>,
    pub comment_inline: Option<String>,
    /// Blank lines in the source before this item (capped at 255).
    pub blank_lines_before: u8,
}

// ─── Format options ───────────────────────────────────────────────────────────

/// Controls which cosmetic fields are reset by `format_with()`.
#[derive(Clone, Copy)]
pub struct FormatOptions {
    /// Reset scalar quoting → plain (literal for multi-line), container style → block,
    /// and clear `original` so non-canonical forms (hex, exponent) emit canonically.
    pub styles: bool,
    /// Clear `comment_before` and `comment_inline` on every entry/item.
    pub comments: bool,
    /// Zero `blank_lines_before` on every entry/item and `trailing_blank_lines` on containers.
    pub blank_lines: bool,
}

impl YamlScalar {
    pub fn format_with(&mut self, opts: FormatOptions) {
        if opts.styles {
            // Strings with embedded newlines get literal block style so the emitter
            // doesn't fall back to double-quoted with \n escape sequences.
            let is_multiline = matches!(&self.value, ScalarValue::Str(s) if s.contains('\n'));
            self.style = if is_multiline {
                ScalarStyle::Literal
            } else {
                ScalarStyle::Plain
            };
            self.original = None;
        }
        // tag and anchor are semantic — always preserved.
    }
}

impl YamlMapping {
    pub fn format_with(&mut self, opts: FormatOptions) {
        if opts.styles {
            self.style = ContainerStyle::Block;
        }
        if opts.blank_lines {
            self.trailing_blank_lines = 0;
        }
        for entry in self.entries.values_mut() {
            if opts.comments {
                entry.comment_before = None;
                entry.comment_inline = None;
            }
            if opts.blank_lines {
                entry.blank_lines_before = 0;
            }
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

impl YamlSequence {
    pub fn format_with(&mut self, opts: FormatOptions) {
        if opts.styles {
            self.style = ContainerStyle::Block;
        }
        if opts.blank_lines {
            self.trailing_blank_lines = 0;
        }
        for item in &mut self.items {
            if opts.comments {
                item.comment_before = None;
                item.comment_inline = None;
            }
            if opts.blank_lines {
                item.blank_lines_before = 0;
            }
            item.value.format_with(opts);
        }
    }
}

impl YamlNode {
    pub fn format_with(&mut self, opts: FormatOptions) {
        match self {
            YamlNode::Mapping(m) => m.format_with(opts),
            YamlNode::Sequence(s) => s.format_with(opts),
            YamlNode::Scalar(s) => s.format_with(opts),
            YamlNode::Alias { .. } | YamlNode::Null => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Null ───────────────────────────────────────────────────────────────────

    #[test]
    fn from_str_null_variants() {
        for s in &["", "null", "Null", "NULL", "~"] {
            assert!(
                matches!(ScalarValue::from_str(s), ScalarValue::Null),
                "expected Null for {s:?}"
            );
        }
    }

    // ── Bool ───────────────────────────────────────────────────────────────────

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

    // ── Integer ────────────────────────────────────────────────────────────────

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

    // ── Float ──────────────────────────────────────────────────────────────────

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

    // ── String fallback ────────────────────────────────────────────────────────

    #[test]
    fn from_str_string_fallback() {
        for s in &["hello", "world", "YAML", "not-a-bool", "1.2.3", "v1.0"] {
            assert!(
                matches!(ScalarValue::from_str(s), ScalarValue::Str(_)),
                "expected Str for {s:?}"
            );
        }
    }

    // ── Edge cases ─────────────────────────────────────────────────────────────

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
