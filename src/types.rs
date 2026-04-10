// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub enum YamlNode {
    Mapping(YamlMapping),
    Sequence(YamlSequence),
    Scalar(YamlScalar),
    Null,
}

#[derive(Debug, Clone)]
pub struct YamlScalar {
    pub value: ScalarValue,
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
}

impl YamlMapping {
    pub fn new() -> Self {
        YamlMapping {
            entries: IndexMap::new(),
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
}

#[derive(Debug, Clone)]
pub struct YamlSequence {
    pub items: Vec<YamlItem>,
}

impl YamlSequence {
    pub fn new() -> Self {
        YamlSequence { items: Vec::new() }
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
}
