// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use crate::types::*;

/// Emit a YAML node to a string, with the given indentation level.
pub fn emit_node(node: &YamlNode, indent: usize, out: &mut String) {
    match node {
        YamlNode::Mapping(m) => emit_mapping(m, indent, out),
        YamlNode::Sequence(s) => emit_sequence(s, indent, out),
        YamlNode::Scalar(s) => {
            out.push_str(&emit_scalar_value(&s.value));
        }
        YamlNode::Null => {
            out.push_str("null");
        }
    }
}

/// Emit a full document list to a string.
pub fn emit_docs(docs: &[YamlNode]) -> String {
    let mut out = String::new();
    for (i, doc) in docs.iter().enumerate() {
        if docs.len() > 1 {
            out.push_str("---\n");
        }
        emit_node(doc, 0, &mut out);
        if i + 1 < docs.len() && !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn indent_str(indent: usize) -> String {
    " ".repeat(indent)
}

fn emit_mapping(m: &YamlMapping, indent: usize, out: &mut String) {
    if m.entries.is_empty() {
        out.push_str("{}\n");
        return;
    }
    for (key, entry) in &m.entries {
        // comment_before: each line prefixed with indent + "# "
        if let Some(cb) = &entry.comment_before {
            for line in cb.lines() {
                out.push_str(&indent_str(indent));
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
        }
        // key:
        out.push_str(&indent_str(indent));
        out.push_str(&emit_key(key));
        out.push(':');

        match &entry.value {
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                // inline comment goes after the colon on key line
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_mapping(nested, indent + 2, out);
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_sequence(nested, indent + 2, out);
            }
            YamlNode::Mapping(_) => {
                // empty mapping
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                out.push_str(&indent_str(indent + 2));
                out.push_str("{}\n");
            }
            YamlNode::Sequence(_) => {
                // empty sequence
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                out.push_str(&indent_str(indent + 2));
                out.push_str("[]\n");
            }
            node => {
                out.push(' ');
                let mut val_str = String::new();
                emit_node(node, indent + 2, &mut val_str);
                // Remove trailing newline from scalar
                let val_str = val_str.trim_end_matches('\n');
                out.push_str(val_str);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
        }
    }
}

fn emit_sequence(s: &YamlSequence, indent: usize, out: &mut String) {
    if s.items.is_empty() {
        out.push_str("[]\n");
        return;
    }
    for item in &s.items {
        if let Some(cb) = &item.comment_before {
            for line in cb.lines() {
                out.push_str(&indent_str(indent));
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str(&indent_str(indent));
        out.push_str("- ");
        match &item.value {
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                if let Some(ci) = &item.comment_inline {
                    out.push_str("# ");
                    out.push_str(ci);
                    out.push('\n');
                    out.push_str(&indent_str(indent + 2));
                    emit_mapping(nested, indent + 2, out);
                } else {
                    // First entry inline with `-`, rest indented
                    emit_mapping_inline_first(nested, indent + 2, out);
                }
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                if let Some(ci) = &item.comment_inline {
                    out.push_str("# ");
                    out.push_str(ci);
                    out.push('\n');
                }
                emit_sequence(nested, indent + 2, out);
            }
            node => {
                let mut val_str = String::new();
                emit_node(node, indent + 2, &mut val_str);
                let val_str = val_str.trim_end_matches('\n');
                out.push_str(val_str);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
        }
    }
}

/// Emit a mapping where the first entry shares the line with the parent `-`.
fn emit_mapping_inline_first(m: &YamlMapping, indent: usize, out: &mut String) {
    let mut first = true;
    for (key, entry) in &m.entries {
        if let Some(cb) = &entry.comment_before {
            if first {
                // Can't put before-comment on the same line as `-`; put it on a new line
                out.push('\n');
                for line in cb.lines() {
                    out.push_str(&indent_str(indent));
                    out.push_str("# ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&indent_str(indent));
            } else {
                for line in cb.lines() {
                    out.push_str(&indent_str(indent));
                    out.push_str("# ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&indent_str(indent));
            }
        } else if !first {
            out.push_str(&indent_str(indent));
        }

        out.push_str(&emit_key(key));
        out.push(':');

        match &entry.value {
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_mapping(nested, indent + 2, out);
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_sequence(nested, indent + 2, out);
            }
            node => {
                out.push(' ');
                let mut val_str = String::new();
                emit_node(node, indent + 2, &mut val_str);
                let val_str = val_str.trim_end_matches('\n');
                out.push_str(val_str);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
        }
        first = false;
    }
}

/// Emit a scalar value in plain or quoted style.
pub fn emit_scalar_value(v: &ScalarValue) -> String {
    match v {
        ScalarValue::Null => "null".to_string(),
        ScalarValue::Bool(true) => "true".to_string(),
        ScalarValue::Bool(false) => "false".to_string(),
        ScalarValue::Int(n) => n.to_string(),
        ScalarValue::Float(f) => {
            if f.is_nan() {
                ".nan".to_string()
            } else if f.is_infinite() {
                if *f > 0.0 {
                    ".inf".to_string()
                } else {
                    "-.inf".to_string()
                }
            } else {
                // Ensure it has a decimal point so it round-trips as float
                let s = format!("{f}");
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    s
                } else {
                    format!("{f}.0")
                }
            }
        }
        ScalarValue::Str(s) => emit_string(s),
    }
}

/// Emit a key string (always safe to quote if needed, but prefer plain).
fn emit_key(key: &str) -> String {
    emit_string(key)
}

/// Emit a string, quoting if necessary.
fn emit_string(s: &str) -> String {
    if needs_quoting(s) {
        // Single-quote, escaping any single-quotes by doubling them
        let escaped = s.replace('\'', "''");
        format!("'{escaped}'")
    } else {
        s.to_owned()
    }
}

/// Return true if the string needs to be quoted in YAML plain style.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Check if it would be parsed as a different type
    let sv = ScalarValue::from_str(s);
    if !matches!(sv, ScalarValue::Str(_)) {
        return true; // would be coerced
    }
    // Check for characters that require quoting
    let first = s.chars().next().unwrap();
    if matches!(
        first,
        '#' | '&'
            | '*'
            | '?'
            | '|'
            | '-'
            | '<'
            | '>'
            | '='
            | '!'
            | '%'
            | '@'
            | '`'
            | '{'
            | '}'
            | '['
            | ']'
            | ','
    ) {
        return true;
    }
    if s.contains(": ") || s.starts_with(": ") || s.ends_with(':') {
        return true;
    }
    if s.contains(" #") {
        return true;
    }
    if s.contains('\n') || s.contains('\r') {
        return true;
    }
    false
}
