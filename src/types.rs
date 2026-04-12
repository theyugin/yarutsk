// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub enum YamlNode {
    Mapping(YamlMapping),
    Sequence(YamlSequence),
    Scalar(YamlScalar),
    Null,
}

/// How a scalar value was written in the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalarStyle {
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
}

#[derive(Debug, Clone)]
pub enum ScalarValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Null,
}

impl ScalarValue {
    /// Parse a raw YAML scalar string into a typed value.
    pub fn from_str(s: &str) -> ScalarValue {
        if s.is_empty() || s == "null" || s == "Null" || s == "NULL" || s == "~" {
            return ScalarValue::Null;
        }
        if s == "true"
            || s == "True"
            || s == "TRUE"
            || s == "yes"
            || s == "Yes"
            || s == "YES"
            || s == "on"
            || s == "On"
            || s == "ON"
        {
            return ScalarValue::Bool(true);
        }
        if s == "false"
            || s == "False"
            || s == "FALSE"
            || s == "no"
            || s == "No"
            || s == "NO"
            || s == "off"
            || s == "Off"
            || s == "OFF"
        {
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
        if s == ".inf" || s == ".Inf" || s == ".INF" {
            return ScalarValue::Float(f64::INFINITY);
        }
        if s == "-.inf" || s == "-.Inf" || s == "-.INF" {
            return ScalarValue::Float(f64::NEG_INFINITY);
        }
        if s == ".nan" || s == ".NaN" || s == ".NAN" {
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
}

impl YamlMapping {
    pub fn new() -> Self {
        YamlMapping {
            entries: IndexMap::new(),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        YamlMapping {
            entries: IndexMap::with_capacity(n),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
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
}

impl YamlSequence {
    pub fn new() -> Self {
        YamlSequence {
            items: Vec::new(),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        YamlSequence {
            items: Vec::with_capacity(n),
            style: ContainerStyle::Block,
            tag: None,
            trailing_blank_lines: 0,
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
