// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::borrow::Cow;

use crate::types::*;

/// Format a stored tag for emission.
///
/// Three cases:
/// - `tag:yaml.org,2002:T` → `!!T`  (built-in YAML secondary handle)
/// - `!…` (already starts with `!`) → returned unchanged  (`!custom`, `!local`, …)
/// - any other full URI `tag:…` → `!<tag:…>`  (YAML verbatim-tag form)
fn format_tag(tag: &str) -> Cow<'_, str> {
    if let Some(suffix) = tag.strip_prefix("tag:yaml.org,2002:") {
        Cow::Owned(format!("!!{suffix}"))
    } else if tag.starts_with('!') {
        Cow::Borrowed(tag)
    } else {
        Cow::Owned(format!("!<{tag}>"))
    }
}

/// Emit a YAML node to a string, with the given indentation level.
pub fn emit_node(node: &YamlNode, indent: usize, out: &mut String) {
    match node {
        YamlNode::Mapping(m) => emit_mapping(m, indent, out),
        YamlNode::Sequence(s) => emit_sequence(s, indent, out),
        // Block scalars (`|` / `>`) are routed to `emit_block_scalar` so that
        // the indicator and indented content are always emitted correctly,
        // whether the scalar is a top-level document or a nested value.
        YamlNode::Scalar(s) if is_block_scalar(s) => emit_block_scalar(s, indent, None, out),
        YamlNode::Scalar(s) => emit_scalar(s, out),
        YamlNode::Null => {
            out.push_str("null");
        }
        YamlNode::Alias { name, .. } => {
            out.push('*');
            out.push_str(name);
        }
    }
}

/// Emit a full document list to a string.
/// `explicit_starts[i]` and `explicit_ends[i]` control whether `---` / `...` are emitted.
/// `yaml_versions[i]` emits a `%YAML` directive before `---` when `Some`.
/// `tag_directives[i]` emits `%TAG` directives before `---` when non-empty.
/// Any slice may be shorter than `docs`; missing entries default to `false` / `None` / empty.
pub fn emit_docs(
    docs: &[YamlNode],
    explicit_starts: &[bool],
    explicit_ends: &[bool],
    yaml_versions: &[Option<(u8, u8)>],
    tag_directives: &[Vec<(String, String)>],
) -> String {
    let mut out = String::with_capacity(256);
    for (i, doc) in docs.iter().enumerate() {
        let want_start = explicit_starts.get(i).copied().unwrap_or(false);
        let want_end = explicit_ends.get(i).copied().unwrap_or(false);
        let version = yaml_versions.get(i).and_then(|v| *v);
        let tags = tag_directives.get(i).map(Vec::as_slice).unwrap_or(&[]);
        let has_directives = version.is_some() || !tags.is_empty();
        if has_directives || docs.len() > 1 || want_start {
            if let Some((major, minor)) = version {
                out.push_str(&format!("%YAML {major}.{minor}\n"));
            }
            for (handle, prefix) in tags {
                out.push_str(&format!("%TAG {handle} {prefix}\n"));
            }
            out.push_str("---\n");
        }
        emit_node(doc, 0, &mut out);
        // Always ensure a trailing newline after document content (valid YAML convention).
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if want_end {
            out.push_str("...\n");
        }
    }
    out
}

/// Append `"  # "` and the comment text to `out`, if the comment is present.
fn push_inline_comment(comment: Option<&str>, out: &mut String) {
    if let Some(ci) = comment {
        out.push_str("  # ");
        out.push_str(ci);
    }
}

/// Emit a block comment (lines prefixed with `# `) at the given indentation.
fn emit_comment_before(comment: Option<&str>, indent: usize, out: &mut String) {
    if let Some(cb) = comment {
        for line in cb.lines() {
            out.push_str(&indent_str(indent));
            out.push_str("# ");
            out.push_str(line);
            out.push('\n');
        }
    }
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

/// Emit a mapping key (alias / complex / block-scalar / plain scalar).
/// The caller is responsible for pushing any leading indentation before this call.
fn emit_mapping_key(key: &str, entry: &YamlEntry, indent: usize, out: &mut String) {
    // Alias key: `? *name\n: value` — explicit form avoids ambiguity with
    // `*alias:` being misinterpreted in block context by some parsers.
    if let Some(alias) = &entry.key_alias {
        out.push_str("? *");
        out.push_str(alias);
        out.push('\n');
        out.push_str(&indent_str(indent));
        out.push(':');
    } else if let Some(key_node) = &entry.key_node {
        // Complex (non-scalar) key: `? <key_node>\n: <value>`
        out.push_str("? ");
        // For block collections, add a newline after `? ` so the content starts
        // on its own line at indent+2, avoiding `?   - item` ambiguity.
        match key_node.as_ref() {
            YamlNode::Sequence(s) if s.style == ContainerStyle::Block => {
                out.push('\n');
                emit_sequence(s, indent + 2, out);
            }
            YamlNode::Mapping(m) if m.style == ContainerStyle::Block => {
                out.push('\n');
                emit_mapping(m, indent + 2, out);
            }
            _ => {
                emit_node(key_node, indent + 2, out);
            }
        }
        out.push_str(&indent_str(indent));
        out.push(':');
    } else if matches!(entry.key_style, ScalarStyle::Literal | ScalarStyle::Folded) {
        // Block-scalar key: `? |\n  content\n: `
        let key_scalar = YamlScalar {
            value: ScalarValue::Str(key.to_owned()),
            style: entry.key_style,
            tag: entry.key_tag.clone(),
            original: None,
            anchor: entry.key_anchor.clone(),
        };
        out.push_str("? ");
        emit_block_scalar(&key_scalar, indent + 2, None, out);
        out.push_str(&indent_str(indent));
        out.push(':');
    } else {
        // Plain / quoted scalar key: optional anchor + tag, then key text.
        if let Some(anchor) = &entry.key_anchor {
            out.push('&');
            out.push_str(anchor);
            out.push(' ');
        }
        if let Some(tag) = &entry.key_tag {
            out.push_str(&format_tag(tag));
            out.push(' ');
        }
        out.push_str(&emit_key(key, entry.key_style));
        out.push(':');
    }
}

/// Emit a mapping entry value (the part after the `:` on the key line).
fn emit_mapping_value(entry: &YamlEntry, indent: usize, out: &mut String) {
    match &entry.value {
        YamlNode::Mapping(nested)
            if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow =>
        {
            // Flow mapping value: emit inline on same line as key.
            out.push(' ');
            emit_mapping_flow(nested, out);
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push('\n');
        }
        YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
            // Block mapping value: anchor (if any) + inline comment, then content below.
            if let Some(anchor) = &nested.anchor {
                out.push_str(" &");
                out.push_str(anchor);
            }
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push('\n');
            emit_mapping(nested, indent + 2, out);
        }
        YamlNode::Sequence(nested)
            if !nested.items.is_empty() && nested.style == ContainerStyle::Flow =>
        {
            // Flow sequence value: emit inline on same line as key.
            out.push(' ');
            emit_sequence_flow(nested, out);
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push('\n');
        }
        YamlNode::Sequence(nested) if !nested.items.is_empty() => {
            // Block sequence value: anchor (if any) + inline comment, then content below.
            if let Some(anchor) = &nested.anchor {
                out.push_str(" &");
                out.push_str(anchor);
            }
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push('\n');
            emit_sequence(nested, indent + 2, out);
        }
        YamlNode::Mapping(_) => {
            // Empty mapping — always inline.
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push_str(" {}\n");
        }
        YamlNode::Sequence(_) => {
            // Empty sequence — always inline.
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push_str(" []\n");
        }
        YamlNode::Scalar(s) if is_block_scalar(s) => {
            // Block scalar: indicator goes on the key line; inline comment follows
            // the indicator (YAML allows `key: |  # comment`).
            out.push(' ');
            emit_block_scalar(s, indent + 2, entry.comment_inline.as_deref(), out);
        }
        node => {
            out.push(' ');
            emit_node_inline(node, indent + 2, out);
            push_inline_comment(entry.comment_inline.as_deref(), out);
            out.push('\n');
        }
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
        emit_comment_before(entry.comment_before.as_deref(), indent, out);
        out.push_str(&indent_str(indent));
        emit_mapping_key(key, entry, indent, out);
        emit_mapping_value(entry, indent, out);
    }
    for _ in 0..m.trailing_blank_lines {
        out.push('\n');
    }
}

fn emit_mapping_flow(m: &YamlMapping, out: &mut String) {
    if let Some(anchor) = &m.anchor {
        out.push('&');
        out.push_str(anchor);
        out.push(' ');
    }
    if let Some(tag) = &m.tag {
        out.push_str(&format_tag(tag));
        out.push(' ');
    }
    out.push('{');
    let mut first = true;
    for (key, entry) in &m.entries {
        if !first {
            out.push_str(", ");
        }
        first = false;
        // Emit key: complex key_node, alias key, or plain scalar key.
        if let Some(key_node) = &entry.key_node {
            // Flow context supports `? <node>: <value>` or plain `<node>: <value>` syntax.
            emit_node_inline(key_node, 0, out);
        } else if let Some(alias) = &entry.key_alias {
            out.push('*');
            out.push_str(alias);
            // Space required: colon is a valid anchor-name character per YAML spec,
            // so `*alias:` is parsed as alias `alias:` rather than alias `alias` + `:`.
            out.push(' ');
        } else {
            if let Some(anchor) = &entry.key_anchor {
                out.push('&');
                out.push_str(anchor);
                out.push(' ');
            }
            if let Some(tag) = &entry.key_tag {
                out.push_str(&format_tag(tag));
                out.push(' ');
            }
            out.push_str(&emit_key(key, entry.key_style));
        }
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
        emit_comment_before(item.comment_before.as_deref(), indent, out);
        out.push_str(&indent_str(indent));
        out.push_str("- ");
        match &item.value {
            YamlNode::Mapping(nested)
                if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow =>
            {
                // Flow mapping in sequence: emit inline
                emit_mapping_flow(nested, out);
                push_inline_comment(item.comment_inline.as_deref(), out);
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
            YamlNode::Sequence(nested)
                if !nested.items.is_empty() && nested.style == ContainerStyle::Flow =>
            {
                // Flow sequence in sequence: emit inline
                emit_sequence_flow(nested, out);
                push_inline_comment(item.comment_inline.as_deref(), out);
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
                emit_block_scalar(scalar, indent + 2, None, out);
            }
            node => {
                emit_node_inline(node, indent + 2, out);
                push_inline_comment(item.comment_inline.as_deref(), out);
                out.push('\n');
            }
        }
    }
    for _ in 0..s.trailing_blank_lines {
        out.push('\n');
    }
}

fn emit_sequence_flow(s: &YamlSequence, out: &mut String) {
    if let Some(anchor) = &s.anchor {
        out.push('&');
        out.push_str(anchor);
        out.push(' ');
    }
    if let Some(tag) = &s.tag {
        out.push_str(&format_tag(tag));
        out.push(' ');
    }
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
        if item.comment_before.is_some() {
            if first {
                // Can't put a before-comment on the same line as the parent `-`
                out.push('\n');
            }
            emit_comment_before(item.comment_before.as_deref(), indent, out);
            out.push_str(&indent_str(indent));
        } else if !first {
            out.push_str(&indent_str(indent));
        }
        out.push_str("- ");
        match &item.value {
            YamlNode::Mapping(nested)
                if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow =>
            {
                emit_mapping_flow(nested, out);
                push_inline_comment(item.comment_inline.as_deref(), out);
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
            YamlNode::Sequence(nested)
                if !nested.items.is_empty() && nested.style == ContainerStyle::Flow =>
            {
                emit_sequence_flow(nested, out);
                push_inline_comment(item.comment_inline.as_deref(), out);
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
                emit_block_scalar(scalar, indent + 2, None, out);
            }
            node => {
                emit_node_inline(node, indent + 2, out);
                push_inline_comment(item.comment_inline.as_deref(), out);
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
        if entry.comment_before.is_some() {
            if first {
                // Can't put before-comment on the same line as `-`; put it on a new line.
                out.push('\n');
            }
            emit_comment_before(entry.comment_before.as_deref(), indent, out);
            out.push_str(&indent_str(indent));
        } else if !first {
            out.push_str(&indent_str(indent));
        }
        // For the first entry the cursor is already positioned after `- ` by the caller.
        emit_mapping_key(key, entry, indent, out);
        emit_mapping_value(entry, indent, out);
        first = false;
    }
}

/// Returns true if this scalar should be emitted as a block scalar (`|` or `>`).
fn is_block_scalar(s: &YamlScalar) -> bool {
    matches!(s.style, ScalarStyle::Literal | ScalarStyle::Folded)
}

/// Emit a block scalar (`|` or `>`), writing the indicator on the current line
/// and the content on subsequent indented lines.
///
/// `inline_comment` is appended after the indicator on the header line (before
/// the trailing newline), matching the YAML syntax `key: |  # comment\n  content`.
///
/// For folded (`>`) scalars the scanner has already joined consecutive source lines
/// with spaces and turned blank-line separators into `\n` characters in the stored
/// value.  To prevent the YAML parser from re-folding those `\n` separators into
/// spaces again on re-parse, this function emits a blank "paragraph separator" line
/// after every non-empty content line that has more content following it.
fn emit_block_scalar(
    s: &YamlScalar,
    indent: usize,
    inline_comment: Option<&str>,
    out: &mut String,
) {
    let indicator = if s.style == ScalarStyle::Literal {
        '|'
    } else {
        '>'
    };
    let content = match &s.value {
        ScalarValue::Str(text) => text.as_str(),
        _ => "",
    };
    // Choose chomping indicator based on trailing newlines:
    //   strip (`-`)  → 0 trailing newlines
    //   clip (none)  → exactly 1 trailing newline
    //   keep (`+`)   → 2 or more trailing newlines
    let trailing_newlines = content.bytes().rev().take_while(|&b| b == b'\n').count();
    let chomping = match trailing_newlines {
        0 => "-",
        1 => "",
        _ => "+",
    };
    // Emit all lines except the artifact empty string produced by a trailing '\n'.
    let lines: Vec<&str> = content.split('\n').collect();
    // Determine whether an explicit indentation indicator is needed:
    //
    // Case A — every non-empty line starts with at least `min_leading` spaces:
    //   The original used `|N` / `>N` with N = min_leading.  Emit with
    //   content_indent = indent - min_leading so that the parser (using
    //   base = self.indent + N) strips exactly (indent - min_leading) + N = indent
    //   spaces, leaving the stored leading spaces in the value.
    //
    // Case B — min_leading == 0 but the FIRST non-empty line starts with spaces:
    //   Auto-detection would pick that line's indentation as the base, which is
    //   larger than the emitter's content indent, causing lines at content_indent
    //   to appear outside the scalar.  Force base = content_indent by emitting `>2`
    //   (or `|2`).  The standard emitter increment is always 2, so
    //   self.indent + 2 = content_indent in every nesting context.
    //
    // Case C — min_leading == 0 and first non-empty line has no leading spaces:
    //   Auto-detection works correctly; no explicit indicator needed.
    let non_empty_lines: Vec<&&str> = lines.iter().filter(|l| !l.is_empty()).collect();
    let min_leading: usize = non_empty_lines
        .iter()
        .map(|l| l.bytes().take_while(|&b| b == b' ').count())
        .min()
        .unwrap_or(0);
    let first_leading: usize = non_empty_lines
        .first()
        .map(|l| l.bytes().take_while(|&b| b == b' ').count())
        .unwrap_or(0);

    let (explicit_indicator, content_indent) = if min_leading > 0 {
        // Case A
        (min_leading, indent.saturating_sub(min_leading))
    } else if first_leading > 0 {
        // Case B — hard-code indicator = 2 (= standard emitter indent step)
        (2, indent)
    } else {
        // Case C
        (0, indent)
    };

    out.push(indicator);
    if explicit_indicator > 0 {
        // Digit before chomping (YAML spec allows either order; digit-first is conventional).
        out.push(char::from_digit(explicit_indicator as u32, 10).unwrap_or('1'));
    }
    out.push_str(chomping);
    push_inline_comment(inline_comment, out);
    out.push('\n');
    let prefix = indent_str(content_indent);
    let emit_count = if content.ends_with('\n') {
        lines.len() - 1
    } else {
        lines.len()
    };
    if indicator == '>' {
        // Folded: emit a blank paragraph-separator line after a base-level content
        // line only when the NEXT non-empty content line is also base-level (B→B
        // transition).  In that case one extra blank is always required because
        // the YAML folder consumes the base line's break via b-l-trimmed, so N
        // blank lines → N newlines; we need N+1 blanks to reproduce N+1 stored
        // newlines.  For all other transitions (B→more-indented, more-indented→B,
        // more-indented→more-indented) the break is preserved, so the stored
        // blank-string count already equals the required YAML blank count.
        //
        // "More-indented" means the line starts with a space or tab.
        // Whitespace-only lines (e.g. a tab-only line) must not emit a separator.
        for (i, line) in lines[..emit_count].iter().enumerate() {
            if line.is_empty() {
                out.push('\n'); // blank line preserved from stored value
            } else {
                out.push_str(&prefix);
                out.push_str(line);
                out.push('\n');
                // A separator is needed when:
                //   • the current line is base-level (not more-indented, not whitespace-only)
                //   • the next non-empty content line is also base-level
                let is_more_indented = |s: &str| s.starts_with(' ') || s.starts_with('\t');
                let next_non_empty_is_base = i + 1 < emit_count
                    && lines[i + 1..emit_count]
                        .iter()
                        .find(|l| !l.is_empty())
                        .is_some_and(|l| !is_more_indented(l));
                let needs_sep =
                    !is_more_indented(line) && !line.trim().is_empty() && next_non_empty_is_base;
                if needs_sep {
                    out.push('\n'); // paragraph separator
                }
            }
        }
    } else {
        // Literal: emit lines verbatim.
        for line in &lines[..emit_count] {
            if line.is_empty() {
                out.push('\n'); // blank line inside block scalar — no indent
            } else {
                out.push_str(&prefix);
                out.push_str(line);
                out.push('\n');
            }
        }
    }
}

/// Emit a scalar value in the appropriate style.
pub fn emit_scalar(s: &YamlScalar, out: &mut String) {
    if let Some(anchor) = &s.anchor {
        out.push('&');
        out.push_str(anchor);
        out.push(' ');
    }
    if let Some(tag) = &s.tag {
        out.push_str(&format_tag(tag));
        out.push(' ');
    }
    // Use preserved source text when available (e.g. float exponent form `1.5e10`,
    // non-canonical null/bool/int forms, tagged plain scalars).
    if let Some(orig) = &s.original {
        out.push_str(orig);
        return;
    }
    out.push_str(&emit_scalar_value_with_style(&s.value, s.style));
}

fn emit_scalar_value_with_style(v: &ScalarValue, style: ScalarStyle) -> Cow<'_, str> {
    match v {
        ScalarValue::Null => Cow::Borrowed("null"),
        ScalarValue::Bool(true) => Cow::Borrowed("true"),
        ScalarValue::Bool(false) => Cow::Borrowed("false"),
        ScalarValue::Int(n) => Cow::Owned(n.to_string()),
        ScalarValue::Float(f) => {
            let s = if f.is_nan() {
                ".nan".to_owned()
            } else if f.is_infinite() {
                if *f > 0.0 {
                    ".inf".to_owned()
                } else {
                    "-.inf".to_owned()
                }
            } else {
                // Ensure it has a decimal point so it round-trips as float
                let s = format!("{f}");
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    s
                } else {
                    format!("{f}.0")
                }
            };
            Cow::Owned(s)
        }
        ScalarValue::Str(s) => Cow::Owned(emit_string_with_style(s, style)),
    }
}

/// Emit a key string with its original quoting style.
/// For `Plain` style, numeric-looking strings are left unquoted since `1:` is
/// valid YAML and our library always stores keys as strings anyway.
fn emit_key(key: &str, style: ScalarStyle) -> String {
    match style {
        ScalarStyle::SingleQuoted => single_quote(key),
        ScalarStyle::DoubleQuoted => double_quote(key),
        _ => {
            if needs_quoting_for_key(key) {
                single_quote(key)
            } else {
                key.to_owned()
            }
        }
    }
}

/// Like `needs_quoting` but for mapping keys: numeric strings (`1`, `3.14`,
/// `0xFF`) are left unquoted since they are valid plain-style YAML keys and
/// our library stores all keys as strings regardless of their YAML type.
fn needs_quoting_for_key(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Check for characters that are structurally significant in YAML
    let b = s.as_bytes();
    let first = b[0] as char;
    if matches!(
        first,
        '#' | '&'
            | '*'
            | '?'
            | '|'
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
    if s.contains(" #") || s.contains('\n') || s.contains('\r') {
        return true;
    }
    false
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
            // Single-quoted strings emit literal newlines, which YAML folds back to
            // spaces on re-parse.  Switch to double-quoted (with \n escapes) when the
            // string contains newlines so the value is preserved on round-trip.
            if s.contains('\n') {
                double_quote(s)
            } else {
                single_quote(s)
            }
        }
        ScalarStyle::DoubleQuoted => double_quote(s),
        ScalarStyle::Literal | ScalarStyle::Folded => {
            // Should have been handled by emit_block_scalar; if we reach here the node
            // is in a context where block scalars are not valid (e.g. flow or key
            // position).  Use double-quoted if the string contains newlines so that
            // `\n` escape sequences are emitted rather than literal newlines, which
            // would cause indentation errors on re-parse.
            if needs_quoting(s) {
                if s.contains('\n') {
                    double_quote(s)
                } else {
                    single_quote(s)
                }
            } else {
                s.to_owned()
            }
        }
        ScalarStyle::Plain => {
            if needs_quoting(s) {
                if s.contains('\n') {
                    double_quote(s)
                } else {
                    single_quote(s)
                }
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
        if c == '\'' {
            out.push_str("''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[inline]
fn double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn plain_str(s: &str) -> YamlNode {
        YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Str(s.to_owned()),
            style: ScalarStyle::Plain,
            tag: None,
            original: None,
            anchor: None,
        })
    }

    fn plain_int(n: i64) -> YamlNode {
        YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Int(n),
            style: ScalarStyle::Plain,
            tag: None,
            original: None,
            anchor: None,
        })
    }

    fn scalar_emit(s: &YamlScalar) -> String {
        let mut out = String::new();
        emit_scalar(s, &mut out);
        out
    }

    fn node_emit(node: &YamlNode) -> String {
        let mut out = String::new();
        emit_node(node, 0, &mut out);
        out
    }

    fn make_scalar_node(
        value: ScalarValue,
        style: ScalarStyle,
        tag: Option<&str>,
        original: Option<&str>,
        anchor: Option<&str>,
    ) -> YamlScalar {
        YamlScalar {
            value,
            style,
            tag: tag.map(|s| s.to_owned()),
            original: original.map(|s| s.to_owned()),
            anchor: anchor.map(|s| s.to_owned()),
        }
    }

    fn make_entry(value: YamlNode) -> YamlEntry {
        YamlEntry {
            value,
            comment_before: None,
            comment_inline: None,
            blank_lines_before: 0,
            key_style: ScalarStyle::Plain,
            key_anchor: None,
            key_alias: None,
            key_tag: None,
            key_node: None,
        }
    }

    fn make_mapping(pairs: &[(&str, YamlNode)]) -> YamlMapping {
        let mut m = YamlMapping::new();
        for (k, v) in pairs {
            m.entries.insert(k.to_string(), make_entry(v.clone()));
        }
        m
    }

    fn make_seq(items: &[YamlNode]) -> YamlSequence {
        let mut s = YamlSequence::new();
        for item in items {
            s.items.push(YamlItem {
                value: item.clone(),
                comment_before: None,
                comment_inline: None,
                blank_lines_before: 0,
            });
        }
        s
    }

    // ── emit_scalar: value types ──────────────────────────────────────────────

    #[test]
    fn emit_null_value() {
        let s = make_scalar_node(ScalarValue::Null, ScalarStyle::Plain, None, None, None);
        assert_eq!(scalar_emit(&s), "null");
    }

    #[test]
    fn emit_bool_true() {
        let s = make_scalar_node(
            ScalarValue::Bool(true),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "true");
    }

    #[test]
    fn emit_bool_false() {
        let s = make_scalar_node(
            ScalarValue::Bool(false),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "false");
    }

    #[test]
    fn emit_int() {
        let s = make_scalar_node(ScalarValue::Int(42), ScalarStyle::Plain, None, None, None);
        assert_eq!(scalar_emit(&s), "42");
    }

    #[test]
    fn emit_negative_int() {
        let s = make_scalar_node(ScalarValue::Int(-1), ScalarStyle::Plain, None, None, None);
        assert_eq!(scalar_emit(&s), "-1");
    }

    #[test]
    fn emit_float() {
        let s = make_scalar_node(
            ScalarValue::Float(3.14),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        let out = scalar_emit(&s);
        assert!(out.contains('.'), "float must have decimal point: {out}");
    }

    #[test]
    fn emit_float_infinity() {
        let s = make_scalar_node(
            ScalarValue::Float(f64::INFINITY),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), ".inf");
    }

    #[test]
    fn emit_float_neg_infinity() {
        let s = make_scalar_node(
            ScalarValue::Float(f64::NEG_INFINITY),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "-.inf");
    }

    #[test]
    fn emit_float_nan() {
        let s = make_scalar_node(
            ScalarValue::Float(f64::NAN),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), ".nan");
    }

    // ── emit_scalar: styles ───────────────────────────────────────────────────

    #[test]
    fn emit_single_quoted_str() {
        let s = make_scalar_node(
            ScalarValue::Str("hello".to_owned()),
            ScalarStyle::SingleQuoted,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "'hello'");
    }

    #[test]
    fn emit_double_quoted_str() {
        let s = make_scalar_node(
            ScalarValue::Str("hello".to_owned()),
            ScalarStyle::DoubleQuoted,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "\"hello\"");
    }

    #[test]
    fn emit_single_quoted_with_embedded_quote() {
        let s = make_scalar_node(
            ScalarValue::Str("it's".to_owned()),
            ScalarStyle::SingleQuoted,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "'it''s'");
    }

    #[test]
    fn emit_double_quoted_escapes_newline() {
        let s = make_scalar_node(
            ScalarValue::Str("a\nb".to_owned()),
            ScalarStyle::DoubleQuoted,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "\"a\\nb\"");
    }

    #[test]
    fn emit_plain_safe_str_unquoted() {
        let s = make_scalar_node(
            ScalarValue::Str("hello".to_owned()),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        assert_eq!(scalar_emit(&s), "hello");
    }

    #[test]
    fn emit_plain_null_str_gets_quoted() {
        // "null" as a Str value in Plain style must be quoted to avoid being parsed as null
        let s = make_scalar_node(
            ScalarValue::Str("null".to_owned()),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        let out = scalar_emit(&s);
        assert!(
            out.starts_with('\'') || out.starts_with('"'),
            "expected quotes: {out}"
        );
    }

    #[test]
    fn emit_plain_empty_str_gets_quoted() {
        let s = make_scalar_node(
            ScalarValue::Str(String::new()),
            ScalarStyle::Plain,
            None,
            None,
            None,
        );
        let out = scalar_emit(&s);
        assert!(!out.is_empty(), "empty string must not emit empty");
    }

    // ── emit_scalar: original preservation ───────────────────────────────────

    #[test]
    fn emit_original_preserved_over_value() {
        // When original is set, it should be emitted verbatim
        let s = make_scalar_node(
            ScalarValue::Bool(true),
            ScalarStyle::Plain,
            None,
            Some("yes"),
            None,
        );
        assert_eq!(scalar_emit(&s), "yes");
    }

    #[test]
    fn emit_hex_original_preserved() {
        let s = make_scalar_node(
            ScalarValue::Int(255),
            ScalarStyle::Plain,
            None,
            Some("0xFF"),
            None,
        );
        assert_eq!(scalar_emit(&s), "0xFF");
    }

    #[test]
    fn emit_tilde_null_preserved() {
        let s = make_scalar_node(ScalarValue::Null, ScalarStyle::Plain, None, Some("~"), None);
        assert_eq!(scalar_emit(&s), "~");
    }

    // ── emit_scalar: tags ────────────────────────────────────────────────────

    #[test]
    fn emit_tag_prefix_before_value() {
        let s = make_scalar_node(
            ScalarValue::Str("42".to_owned()),
            ScalarStyle::Plain,
            Some("tag:yaml.org,2002:str"),
            Some("42"),
            None,
        );
        let out = scalar_emit(&s);
        assert!(out.starts_with("!!str "), "expected '!!str ' prefix: {out}");
        assert!(out.ends_with("42"), "expected '42' value: {out}");
    }

    #[test]
    fn emit_custom_tag_unchanged() {
        let s = make_scalar_node(
            ScalarValue::Str("val".to_owned()),
            ScalarStyle::Plain,
            Some("!custom"),
            Some("val"),
            None,
        );
        let out = scalar_emit(&s);
        assert!(
            out.starts_with("!custom "),
            "expected '!custom ' prefix: {out}"
        );
    }

    // ── emit_scalar: anchors ─────────────────────────────────────────────────

    #[test]
    fn emit_anchor_prefix_before_value() {
        let s = make_scalar_node(
            ScalarValue::Int(10),
            ScalarStyle::Plain,
            None,
            None,
            Some("myanchor"),
        );
        let out = scalar_emit(&s);
        assert_eq!(out, "&myanchor 10");
    }

    #[test]
    fn emit_anchor_before_tag_before_value() {
        let s = make_scalar_node(
            ScalarValue::Str("42".to_owned()),
            ScalarStyle::Plain,
            Some("tag:yaml.org,2002:str"),
            Some("42"),
            Some("a"),
        );
        let out = scalar_emit(&s);
        assert!(
            out.starts_with("&a !!str "),
            "expected '&a !!str ' prefix: {out}"
        );
    }

    // ── emit_node: Null / Alias ───────────────────────────────────────────────

    #[test]
    fn emit_null_node() {
        assert_eq!(node_emit(&YamlNode::Null), "null");
    }

    #[test]
    fn emit_alias_node() {
        let node = YamlNode::Alias {
            name: "myref".to_owned(),
            resolved: Box::new(YamlNode::Null),
        };
        assert_eq!(node_emit(&node), "*myref");
    }

    // ── emit_docs: single doc ─────────────────────────────────────────────────

    #[test]
    fn emit_single_doc_no_markers() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[false], &[false], &[None], &[vec![]]);
        assert_eq!(out, "a: 1\n");
    }

    #[test]
    fn emit_single_doc_with_start_marker() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[true], &[false], &[None], &[vec![]]);
        assert_eq!(out, "---\na: 1\n");
    }

    #[test]
    fn emit_single_doc_with_end_marker() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[false], &[true], &[None], &[vec![]]);
        assert_eq!(out, "a: 1\n...\n");
    }

    #[test]
    fn emit_single_doc_with_both_markers() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[true], &[true], &[None], &[vec![]]);
        assert_eq!(out, "---\na: 1\n...\n");
    }

    // ── emit_docs: multiple docs ──────────────────────────────────────────────

    #[test]
    fn emit_two_docs_adds_start_markers() {
        let d1 = plain_str("hello");
        let d2 = plain_str("world");
        let out = emit_docs(
            &[d1, d2],
            &[false, false],
            &[false, false],
            &[None, None],
            &[vec![], vec![]],
        );
        assert!(
            out.starts_with("---\n"),
            "expected --- before first doc: {out}"
        );
        assert!(
            out.contains("\n---\n"),
            "expected --- before second doc: {out}"
        );
    }

    #[test]
    fn emit_empty_docs_slice() {
        let out = emit_docs(&[], &[], &[], &[], &[]);
        assert_eq!(out, "");
    }

    // ── emit mapping ──────────────────────────────────────────────────────────

    #[test]
    fn emit_mapping_preserves_order() {
        let m = make_mapping(&[
            ("z", plain_int(1)),
            ("a", plain_int(2)),
            ("m", plain_int(3)),
        ]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "z: 1");
        assert_eq!(lines[1], "a: 2");
        assert_eq!(lines[2], "m: 3");
    }

    #[test]
    fn emit_empty_mapping() {
        let m = make_mapping(&[]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "{}\n");
    }

    #[test]
    fn emit_flow_mapping() {
        let mut m = make_mapping(&[("a", plain_int(1)), ("b", plain_int(2))]);
        m.style = ContainerStyle::Flow;
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "{a: 1, b: 2}");
    }

    #[test]
    fn emit_mapping_with_inline_comment() {
        let mut m = YamlMapping::new();
        let mut entry = make_entry(plain_int(1));
        entry.comment_inline = Some("a comment".to_owned());
        m.entries.insert("key".to_owned(), entry);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "key: 1  # a comment\n");
    }

    #[test]
    fn emit_mapping_with_before_comment() {
        let mut m = YamlMapping::new();
        let mut entry = make_entry(plain_int(1));
        entry.comment_before = Some("header".to_owned());
        m.entries.insert("key".to_owned(), entry);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "# header\nkey: 1\n");
    }

    #[test]
    fn emit_mapping_with_blank_line_before_entry() {
        let mut m = make_mapping(&[("a", plain_int(1)), ("b", plain_int(2))]);
        m.entries.get_mut("b").unwrap().blank_lines_before = 1;
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "a: 1\n\nb: 2\n");
    }

    #[test]
    fn emit_mapping_with_empty_nested_mapping() {
        let empty = YamlNode::Mapping(make_mapping(&[]));
        let m = make_mapping(&[("key", empty)]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "key: {}\n");
    }

    #[test]
    fn emit_mapping_with_empty_nested_sequence() {
        let empty = YamlNode::Sequence(make_seq(&[]));
        let m = make_mapping(&[("key", empty)]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert_eq!(out, "key: []\n");
    }

    // ── emit sequence ─────────────────────────────────────────────────────────

    #[test]
    fn emit_sequence_items() {
        let s = make_seq(&[plain_int(1), plain_int(2), plain_int(3)]);
        let mut out = String::new();
        emit_node(&YamlNode::Sequence(s), 0, &mut out);
        assert_eq!(out, "- 1\n- 2\n- 3\n");
    }

    #[test]
    fn emit_empty_sequence() {
        let s = make_seq(&[]);
        let mut out = String::new();
        emit_node(&YamlNode::Sequence(s), 0, &mut out);
        assert_eq!(out, "[]\n");
    }

    #[test]
    fn emit_flow_sequence() {
        let mut s = make_seq(&[plain_int(1), plain_int(2), plain_int(3)]);
        s.style = ContainerStyle::Flow;
        let mut out = String::new();
        emit_node(&YamlNode::Sequence(s), 0, &mut out);
        assert_eq!(out, "[1, 2, 3]");
    }

    #[test]
    fn emit_sequence_with_inline_comment() {
        let mut s = YamlSequence::new();
        s.items.push(YamlItem {
            value: plain_int(1),
            comment_before: None,
            comment_inline: Some("one".to_owned()),
            blank_lines_before: 0,
        });
        let mut out = String::new();
        emit_node(&YamlNode::Sequence(s), 0, &mut out);
        assert_eq!(out, "- 1  # one\n");
    }

    // ── block scalars ────────────────────────────────────────────────────────

    #[test]
    fn emit_literal_block_scalar() {
        let m = make_mapping(&[(
            "text",
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str("hello\nworld\n".to_owned()),
                style: ScalarStyle::Literal,
                tag: None,
                original: None,
                anchor: None,
            }),
        )]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert!(
            out.starts_with("text: |\n"),
            "expected block indicator: {out}"
        );
        assert!(
            out.contains("  hello\n"),
            "expected indented content: {out}"
        );
    }

    #[test]
    fn emit_folded_block_scalar() {
        let m = make_mapping(&[(
            "text",
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str("hello world\n".to_owned()),
                style: ScalarStyle::Folded,
                tag: None,
                original: None,
                anchor: None,
            }),
        )]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert!(
            out.starts_with("text: >\n"),
            "expected folded indicator: {out}"
        );
    }

    #[test]
    fn emit_folded_block_multiline() {
        // Multi-paragraph folded content must survive a full emit → re-parse cycle.
        let cases: &[(&str, &str)] = &[
            // Two paragraphs, one blank-line separator
            ("ab cd\nef\n", "ab cd\nef\n"),
            // Three paragraphs, double blank-line separator between 2nd and 3rd
            ("ab cd\nef\n\ngh\n", "ab cd\nef\n\ngh\n"),
        ];
        for (content, expected_value) in cases {
            let m = make_mapping(&[(
                "text",
                YamlNode::Scalar(YamlScalar {
                    value: ScalarValue::Str((*content).to_owned()),
                    style: ScalarStyle::Folded,
                    tag: None,
                    original: None,
                    anchor: None,
                }),
            )]);
            let mut out = String::new();
            emit_node(&YamlNode::Mapping(m), 0, &mut out);
            let re_parsed = crate::builder::parse_str(&out).expect("re-parse failed");
            let re_docs = re_parsed.docs;
            if let YamlNode::Mapping(m2) = &re_docs[0] {
                if let YamlNode::Scalar(s) = &m2.entries["text"].value {
                    assert_eq!(
                        s.value,
                        ScalarValue::Str((*expected_value).to_owned()),
                        "value mismatch for content={content:?}\nemitted:\n{out}"
                    );
                }
            }
        }
    }

    #[test]
    fn emit_literal_strip_chomping() {
        // Content with no trailing newline gets strip chomping (`|-`)
        let m = make_mapping(&[(
            "text",
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str("no trailing newline".to_owned()),
                style: ScalarStyle::Literal,
                tag: None,
                original: None,
                anchor: None,
            }),
        )]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert!(
            out.starts_with("text: |-\n"),
            "expected strip chomping: {out}"
        );
    }

    #[test]
    fn emit_literal_keep_chomping() {
        // Content with two trailing newlines gets keep chomping (`|+`)
        let m = make_mapping(&[(
            "text",
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str("two newlines\n\n".to_owned()),
                style: ScalarStyle::Literal,
                tag: None,
                original: None,
                anchor: None,
            }),
        )]);
        let mut out = String::new();
        emit_node(&YamlNode::Mapping(m), 0, &mut out);
        assert!(
            out.starts_with("text: |+\n"),
            "expected keep chomping: {out}"
        );
    }

    // ── format_tag helper ────────────────────────────────────────────────────

    #[test]
    fn format_tag_yaml_org_maps_to_bang_bang() {
        assert_eq!(format_tag("tag:yaml.org,2002:str").as_ref(), "!!str");
        assert_eq!(format_tag("tag:yaml.org,2002:int").as_ref(), "!!int");
        assert_eq!(format_tag("tag:yaml.org,2002:float").as_ref(), "!!float");
    }

    #[test]
    fn format_tag_custom_tag_unchanged() {
        assert_eq!(format_tag("!custom").as_ref(), "!custom");
        assert_eq!(format_tag("!!python/tuple").as_ref(), "!!python/tuple");
    }

    // ── needs_quoting ────────────────────────────────────────────────────────

    #[test]
    fn needs_quoting_empty_str() {
        assert!(needs_quoting(""));
    }

    #[test]
    fn needs_quoting_null_keyword() {
        assert!(needs_quoting("null"));
        assert!(needs_quoting("~"));
        assert!(needs_quoting("Null"));
    }

    #[test]
    fn needs_quoting_bool_keywords() {
        for s in &["true", "false", "yes", "no", "on", "off", "True", "False"] {
            assert!(needs_quoting(s), "should need quoting: {s}");
        }
    }

    #[test]
    fn needs_quoting_integer_str() {
        assert!(needs_quoting("42"));
        assert!(needs_quoting("-1"));
        assert!(needs_quoting("0xFF"));
        assert!(needs_quoting("0o77"));
    }

    #[test]
    fn needs_quoting_float_str() {
        assert!(needs_quoting("3.14"));
        assert!(needs_quoting("1e5"));
        assert!(needs_quoting(".inf"));
        assert!(needs_quoting(".nan"));
    }

    #[test]
    fn needs_quoting_regular_string_safe() {
        assert!(!needs_quoting("hello"));
        assert!(!needs_quoting("world"));
        assert!(!needs_quoting("some-value"));
    }

    #[test]
    fn needs_quoting_inline_comment_trigger() {
        assert!(needs_quoting("value # comment"));
    }

    #[test]
    fn needs_quoting_colon_space() {
        assert!(needs_quoting("key: val"));
    }

    // ── needs_quoting_for_key ────────────────────────────────────────────────

    #[test]
    fn needs_quoting_for_key_empty() {
        assert!(needs_quoting_for_key(""));
    }

    #[test]
    fn needs_quoting_for_key_hash_leading() {
        assert!(needs_quoting_for_key("#comment"));
    }

    #[test]
    fn needs_quoting_for_key_star_leading() {
        assert!(needs_quoting_for_key("*alias"));
    }

    #[test]
    fn needs_quoting_for_key_question_leading() {
        assert!(needs_quoting_for_key("?complex"));
    }

    #[test]
    fn needs_quoting_for_key_bang_leading() {
        assert!(needs_quoting_for_key("!tag"));
    }

    #[test]
    fn needs_quoting_for_key_ends_with_colon() {
        assert!(needs_quoting_for_key("key:"));
    }

    #[test]
    fn needs_quoting_for_key_colon_space_inside() {
        assert!(needs_quoting_for_key("key: value"));
    }

    #[test]
    fn needs_quoting_for_key_space_hash_inside() {
        assert!(needs_quoting_for_key("key #comment"));
    }

    #[test]
    fn needs_quoting_for_key_newline() {
        assert!(needs_quoting_for_key("a\nb"));
    }

    #[test]
    fn needs_quoting_for_key_numeric_not_quoted() {
        // Keys differ from values: numeric strings are valid plain-style keys
        assert!(!needs_quoting_for_key("42"));
        assert!(!needs_quoting_for_key("3.14"));
        assert!(!needs_quoting_for_key("0xFF"));
    }

    #[test]
    fn needs_quoting_for_key_plain_safe() {
        assert!(!needs_quoting_for_key("simple"));
        assert!(!needs_quoting_for_key("snake_case"));
        assert!(!needs_quoting_for_key("kebab-case"));
    }

    // ── emit_key ─────────────────────────────────────────────────────────────

    fn do_emit_key(key: &str, style: ScalarStyle) -> String {
        emit_key(key, style)
    }

    #[test]
    fn emit_key_plain_safe_unquoted() {
        assert_eq!(do_emit_key("simple", ScalarStyle::Plain), "simple");
    }

    #[test]
    fn emit_key_plain_needs_quoting_gets_single_quoted() {
        let out = do_emit_key("key:", ScalarStyle::Plain);
        assert!(out.starts_with('\''), "expected single-quoted: {out}");
    }

    #[test]
    fn emit_key_explicit_single_quoted() {
        assert_eq!(do_emit_key("hello", ScalarStyle::SingleQuoted), "'hello'");
    }

    #[test]
    fn emit_key_explicit_double_quoted() {
        assert_eq!(do_emit_key("hello", ScalarStyle::DoubleQuoted), "\"hello\"");
    }

    #[test]
    fn emit_key_numeric_plain_unchanged() {
        // Numeric keys are NOT quoted (they parse back as strings anyway)
        assert_eq!(do_emit_key("42", ScalarStyle::Plain), "42");
    }

    // ── emit_string_with_style ───────────────────────────────────────────────

    #[test]
    fn emit_string_single_quoted_with_newline_uses_double_quotes() {
        // SingleQuoted + embedded newline falls back to double-quoted with \n escapes
        let out = emit_string_with_style("a\nb", ScalarStyle::SingleQuoted);
        assert!(
            out.starts_with('"'),
            "expected double-quoted fallback for newline in single-quoted: {out}"
        );
        assert!(out.contains("\\n"), "expected \\n escape: {out}");
    }

    #[test]
    fn emit_string_single_quoted_no_newline() {
        let out = emit_string_with_style("hello", ScalarStyle::SingleQuoted);
        assert_eq!(out, "'hello'");
    }

    #[test]
    fn emit_string_double_quoted() {
        let out = emit_string_with_style("hello", ScalarStyle::DoubleQuoted);
        assert_eq!(out, "\"hello\"");
    }

    #[test]
    fn emit_string_plain_safe_unquoted() {
        let out = emit_string_with_style("hello", ScalarStyle::Plain);
        assert_eq!(out, "hello");
    }

    #[test]
    fn emit_string_plain_needs_quoting() {
        let out = emit_string_with_style("true", ScalarStyle::Plain);
        assert!(
            out.starts_with('\'') || out.starts_with('"'),
            "expected quotes: {out}"
        );
    }
}

/// Return true if the string needs to be quoted in YAML plain style.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Check if it would be parsed as a non-string type.
    // Mirrors ScalarValue::from_str but avoids allocating a String.
    match s {
        "null" | "Null" | "NULL" | "~" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES"
        | "on" | "On" | "ON" | "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off"
        | "OFF" | ".inf" | ".Inf" | ".INF" | "-.inf" | "-.Inf" | "-.INF" | ".nan" | ".NaN"
        | ".NAN" => return true,
        _ => {}
    }
    // Numeric: hex/octal prefix → int; decimal int; float with . or e
    // s is non-empty (checked above); safe to index b[0].
    let b = s.as_bytes();
    let start = if b[0] == b'-' || b[0] == b'+' { 1 } else { 0 };
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
