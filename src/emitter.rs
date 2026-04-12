// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::borrow::Cow;

use crate::types::*;

/// Emit a YAML node to a string, with the given indentation level.
pub fn emit_node(node: &YamlNode, indent: usize, out: &mut String) {
    match node {
        YamlNode::Mapping(m) => emit_mapping(m, indent, out),
        YamlNode::Sequence(s) => emit_sequence(s, indent, out),
        YamlNode::Scalar(s) => emit_scalar(s, out),
        YamlNode::Null => {
            out.push_str("null");
        }
    }
}

/// Emit a full document list to a string.
pub fn emit_docs(docs: &[YamlNode]) -> String {
    let mut out = String::with_capacity(256);
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

// 128 spaces covers any realistic YAML indentation depth.
// For pathological depths beyond 128, we fall back to an owned allocation.
const SPACES: &str = "                                                                                                                                ";

fn indent_str(indent: usize) -> Cow<'static, str> {
    if indent <= SPACES.len() {
        Cow::Borrowed(&SPACES[..indent])
    } else {
        Cow::Owned(" ".repeat(indent))
    }
}

/// Emit `node` into `out` without a trailing newline.
/// Used for scalar values that appear inline after `: ` or `- `.
fn emit_node_inline(node: &YamlNode, indent: usize, out: &mut String) {
    let start = out.len();
    emit_node(node, indent, out);
    // Trim any trailing newline added by emit_node.
    while out.len() > start && out.ends_with('\n') {
        out.pop();
    }
}

fn emit_mapping(m: &YamlMapping, indent: usize, out: &mut String) {
    if m.style == ContainerStyle::Flow {
        emit_mapping_flow(m, out);
        return;
    }
    if m.entries.is_empty() {
        out.push_str("{}\n");
        return;
    }
    for (key, entry) in &m.entries {
        for _ in 0..entry.blank_lines_before {
            out.push('\n');
        }
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
            YamlNode::Mapping(nested) if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow => {
                // Flow mapping value: emit inline on same line
                out.push(' ');
                emit_mapping_flow(nested, out);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                // Block mapping value: emit on next line, indented
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_mapping(nested, indent + 2, out);
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() && nested.style == ContainerStyle::Flow => {
                // Flow sequence value: emit inline on same line
                out.push(' ');
                emit_sequence_flow(nested, out);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                // Block sequence value: emit on next line, indented
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_sequence(nested, indent + 2, out);
            }
            YamlNode::Mapping(_) => {
                // empty mapping — always inline
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push_str(" {}\n");
            }
            YamlNode::Sequence(_) => {
                // empty sequence — always inline
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push_str(" []\n");
            }
            YamlNode::Scalar(s) if is_block_scalar(s) => {
                // Block scalar: `|` or `>` goes on the key line, content indented below
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push(' ');
                emit_block_scalar(s, indent + 2, out);
            }
            node => {
                out.push(' ');
                emit_node_inline(node, indent + 2, out);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
        }
    }
}

fn emit_mapping_flow(m: &YamlMapping, out: &mut String) {
    out.push('{');
    let mut first = true;
    for (key, entry) in &m.entries {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(&emit_key(key));
        out.push_str(": ");
        emit_node_inline(&entry.value, 0, out);
    }
    out.push('}');
}

fn emit_sequence(s: &YamlSequence, indent: usize, out: &mut String) {
    if s.style == ContainerStyle::Flow {
        emit_sequence_flow(s, out);
        return;
    }
    if s.items.is_empty() {
        out.push_str("[]\n");
        return;
    }
    for item in &s.items {
        for _ in 0..item.blank_lines_before {
            out.push('\n');
        }
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
            YamlNode::Mapping(nested) if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow => {
                // Flow mapping in sequence: emit inline
                emit_mapping_flow(nested, out);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
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
            YamlNode::Sequence(nested) if !nested.items.is_empty() && nested.style == ContainerStyle::Flow => {
                // Flow sequence in sequence: emit inline
                emit_sequence_flow(nested, out);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                if let Some(ci) = &item.comment_inline {
                    out.push_str("# ");
                    out.push_str(ci);
                    out.push('\n');
                    emit_sequence(nested, indent + 2, out);
                } else {
                    // First inner item inline with `-`, rest indented
                    emit_sequence_inline_first(nested, indent + 2, out);
                }
            }
            YamlNode::Scalar(scalar) if is_block_scalar(scalar) => {
                // Block scalar directly in sequence
                emit_block_scalar(scalar, indent + 2, out);
            }
            node => {
                emit_node_inline(node, indent + 2, out);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
        }
    }
}

fn emit_sequence_flow(s: &YamlSequence, out: &mut String) {
    out.push('[');
    let mut first = true;
    for item in &s.items {
        if !first {
            out.push_str(", ");
        }
        first = false;
        emit_node_inline(&item.value, 0, out);
    }
    out.push(']');
}

/// Emit a sequence where the first item shares the line with the parent `-`.
fn emit_sequence_inline_first(s: &YamlSequence, indent: usize, out: &mut String) {
    let mut first = true;
    for item in &s.items {
        if !first {
            for _ in 0..item.blank_lines_before {
                out.push('\n');
            }
        }
        if let Some(cb) = &item.comment_before {
            if first {
                // Can't put a before-comment on the same line as the parent `-`
                out.push('\n');
            }
            for line in cb.lines() {
                out.push_str(&indent_str(indent));
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(&indent_str(indent));
        } else if !first {
            out.push_str(&indent_str(indent));
        }
        out.push_str("- ");
        match &item.value {
            YamlNode::Mapping(nested) if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow => {
                emit_mapping_flow(nested, out);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                if let Some(ci) = &item.comment_inline {
                    out.push_str("# ");
                    out.push_str(ci);
                    out.push('\n');
                    emit_mapping(nested, indent + 2, out);
                } else {
                    emit_mapping_inline_first(nested, indent + 2, out);
                }
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() && nested.style == ContainerStyle::Flow => {
                emit_sequence_flow(nested, out);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                if let Some(ci) = &item.comment_inline {
                    out.push_str("# ");
                    out.push_str(ci);
                    out.push('\n');
                    emit_sequence(nested, indent + 2, out);
                } else {
                    emit_sequence_inline_first(nested, indent + 2, out);
                }
            }
            YamlNode::Scalar(scalar) if is_block_scalar(scalar) => {
                emit_block_scalar(scalar, indent + 2, out);
            }
            node => {
                emit_node_inline(node, indent + 2, out);
                if let Some(ci) = &item.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
        }
        first = false;
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
            YamlNode::Mapping(nested) if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow => {
                out.push(' ');
                emit_mapping_flow(nested, out);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_mapping(nested, indent + 2, out);
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() && nested.style == ContainerStyle::Flow => {
                out.push(' ');
                emit_sequence_flow(nested, out);
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push('\n');
                emit_sequence(nested, indent + 2, out);
            }
            YamlNode::Scalar(s) if is_block_scalar(s) => {
                if let Some(ci) = &entry.comment_inline {
                    out.push_str("  # ");
                    out.push_str(ci);
                }
                out.push(' ');
                emit_block_scalar(s, indent + 2, out);
            }
            node => {
                out.push(' ');
                emit_node_inline(node, indent + 2, out);
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

/// Returns true if this scalar should be emitted as a block scalar (`|` or `>`).
fn is_block_scalar(s: &YamlScalar) -> bool {
    matches!(s.style, ScalarStyle::Literal | ScalarStyle::Folded)
}

/// Emit a block scalar (`|` or `>`), writing the indicator on the current line
/// and the content on subsequent indented lines.
fn emit_block_scalar(s: &YamlScalar, indent: usize, out: &mut String) {
    let indicator = if s.style == ScalarStyle::Literal {
        '|'
    } else {
        '>'
    };
    let content = match &s.value {
        ScalarValue::Str(text) => text.as_str(),
        _ => "",
    };
    // Choose chomping indicator: '-' to strip final newline if content doesn't end with \n.
    let chomping = if content.ends_with('\n') { "" } else { "-" };
    out.push(indicator);
    out.push_str(chomping);
    out.push('\n');
    let prefix = indent_str(indent);
    for line in content.split('\n') {
        // The last element of split('\n') is empty if content ends with '\n'.
        if !line.is_empty() || !content.ends_with('\n') {
            out.push_str(&prefix);
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// Emit a scalar value in the appropriate style.
pub fn emit_scalar(s: &YamlScalar, out: &mut String) {
    out.push_str(&emit_scalar_value_with_style(&s.value, s.style));
}

fn emit_scalar_value_with_style(v: &ScalarValue, style: ScalarStyle) -> String {
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
        ScalarValue::Str(s) => emit_string_with_style(s, style),
    }
}

/// Emit a key string (always safe to quote if needed, but prefer plain).
fn emit_key(key: &str) -> String {
    emit_string_with_style(key, ScalarStyle::Plain)
}

/// Emit a string value honoring the requested style.
/// - `Plain` → unquoted if safe, otherwise single-quoted
/// - `SingleQuoted` → always single-quoted
/// - `DoubleQuoted` → always double-quoted
/// - `Literal` / `Folded` → block scalars are handled by `emit_block_scalar` directly;
///   this path falls back to single-quoted for string values stored as Str.
fn emit_string_with_style(s: &str, style: ScalarStyle) -> String {
    match style {
        ScalarStyle::SingleQuoted => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('\'');
            for c in s.chars() {
                if c == '\'' { out.push_str("''"); } else { out.push(c); }
            }
            out.push('\'');
            out
        }
        ScalarStyle::DoubleQuoted => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            for c in s.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '"'  => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c    => out.push(c),
                }
            }
            out.push('"');
            out
        }
        ScalarStyle::Literal | ScalarStyle::Folded => {
            // Should have been handled by emit_block_scalar; if we reach here the node
            // isn't in a key position, fall back to single-quoted.
            if needs_quoting(s) {
                single_quote(s)
            } else {
                s.to_owned()
            }
        }
        ScalarStyle::Plain => {
            if needs_quoting(s) {
                single_quote(s)
            } else {
                s.to_owned()
            }
        }
    }
}

#[inline]
fn single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' { out.push_str("''"); } else { out.push(c); }
    }
    out.push('\'');
    out
}

/// Return true if the string needs to be quoted in YAML plain style.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Check if it would be parsed as a non-string type.
    // Mirrors ScalarValue::from_str but avoids allocating a String.
    match s {
        "null" | "Null" | "NULL" | "~"
        | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES" | "on" | "On" | "ON"
        | "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off" | "OFF"
        | ".inf" | ".Inf" | ".INF" | "-.inf" | "-.Inf" | "-.INF"
        | ".nan" | ".NaN" | ".NAN" => return true,
        _ => {}
    }
    // Numeric: hex/octal prefix → int; decimal int; float with . or e
    let b = s.as_bytes();
    let start = if b[0] == b'-' || b[0] == b'+' { 1 } else { 0 };
    if start < b.len() {
        let rest = &s[start..];
        if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            if i64::from_str_radix(hex, 16).is_ok() {
                return true;
            }
        } else if let Some(oct) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
            if i64::from_str_radix(oct, 8).is_ok() {
                return true;
            }
        } else if s.parse::<i64>().is_ok()
            || ((s.contains('.') || s.contains('e') || s.contains('E')) && s.parse::<f64>().is_ok())
        {
            return true;
        }
    }
    // Check for characters that require quoting
    let first = b[0] as char;
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
