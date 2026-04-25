// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::borrow::Cow;
use std::fmt::{self, Write as FmtWrite};

use super::builder::DocMetadata;
use super::char_traits::{is_tag_char, is_uri_char};
use super::types::{
    Chomping, ContainerStyle, ScalarStyle, ScalarValue, YamlEntry, YamlMapping, YamlNode,
    YamlScalar, YamlSequence,
};

// ─── LastCharTracker ─────────────────────────────────────────────────────────

/// A `fmt::Write` wrapper that remembers the last character written.
/// Used to check whether a trailing `\n` needs to be appended.
struct LastCharTracker<W> {
    inner: W,
    last: Option<char>,
}

impl<W: FmtWrite> LastCharTracker<W> {
    fn new(inner: W) -> Self {
        LastCharTracker { inner, last: None }
    }

    fn ends_with_newline(&self) -> bool {
        self.last == Some('\n')
    }
}

impl<W: FmtWrite> FmtWrite for LastCharTracker<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(c) = s.chars().last() {
            self.last = Some(c);
        }
        self.inner.write_str(s)
    }
}

// ─── Emitter ─────────────────────────────────────────────────────────────────

/// Holds the write sink + per-level indentation step. Recursion-frame state
/// (`indent`, `flow_context`) stays as method parameters because it changes per
/// call; folding it into the struct would force every helper to save-restore.
struct Emitter<'w, W: FmtWrite> {
    out: &'w mut W,
    step: usize,
}

/// Borrow target for `emit_nested_in_seq` — the two cases sequences nest into.
#[derive(Clone, Copy)]
enum NestedKind<'a> {
    Sequence(&'a YamlSequence),
    Mapping(&'a YamlMapping),
}

impl<'w, W: FmtWrite> Emitter<'w, W> {
    fn new(out: &'w mut W, step: usize) -> Self {
        Self { out, step }
    }

    /// Emit the `&anchor TAG ` inline prefix with a trailing space after each component.
    /// Used when a prefix precedes other content on the same line (flow containers,
    /// key prefixes, `emit_scalar`).
    fn write_anchor_tag_inline(&mut self, anchor: Option<&str>, tag: Option<&str>) -> fmt::Result {
        if let Some(anchor) = anchor {
            self.out.write_char('&')?;
            self.out.write_str(anchor)?;
            self.out.write_char(' ')?;
        }
        if let Some(tag) = tag {
            self.out.write_str(&format_tag(tag))?;
            self.out.write_char(' ')?;
        }
        Ok(())
    }

    /// Emit the ` &anchor TAG` block-suffix prefix with a leading space before each
    /// component and no trailing space. Used to extend a key/value line (after `:`
    /// or `-`) with an anchor and/or tag before the newline that introduces the body.
    fn write_anchor_tag_block_suffix(
        &mut self,
        anchor: Option<&str>,
        tag: Option<&str>,
    ) -> fmt::Result {
        if let Some(anchor) = anchor {
            self.out.write_str(" &")?;
            self.out.write_str(anchor)?;
        }
        if let Some(tag) = tag {
            self.out.write_char(' ')?;
            self.out.write_str(&format_tag(tag))?;
        }
        Ok(())
    }

    /// Emit a YAML node with the given indentation level.
    ///
    /// `flow_context` is true once we are inside a `[ … ]` / `{ … }` collection;
    /// it propagates to scalar emission so plain values containing flow indicators
    /// (`,` `[` `]` `{` `}`) get quoted.
    fn emit_node(&mut self, node: &YamlNode, indent: usize, flow_context: bool) -> fmt::Result {
        match node {
            YamlNode::Mapping(m) => self.emit_mapping(m, indent)?,
            YamlNode::Sequence(s) => self.emit_sequence(s, indent)?,
            // Block scalars (`|` / `>`) are routed to `emit_block_scalar` so that
            // the indicator and indented content are always emitted correctly,
            // whether the scalar is a top-level document or a nested value.
            //
            // At document root (indent = 0), content emitted at column 0 would be
            // mis-parsed: `#` lines become comments, `---` / `...` lines become
            // document markers, and the next doc's start indicator gets folded
            // into the scalar's content. Bump to one step so content sits at
            // column `step`, safely inside the block scalar.
            YamlNode::Scalar(s) if is_block_scalar(s) => {
                let effective = if indent == 0 { self.step } else { indent };
                self.emit_block_scalar(s, effective, None)?;
            }
            YamlNode::Scalar(s) => self.emit_scalar(s, flow_context)?,
            YamlNode::Null => {
                self.out.write_str("null")?;
            }
            YamlNode::Alias { name, .. } => {
                self.out.write_char('*')?;
                self.out.write_str(name)?;
            }
        }
        Ok(())
    }

    /// Append `"  # "` and the comment text, if the comment is present.
    fn push_inline_comment(&mut self, comment: Option<&str>) -> fmt::Result {
        if let Some(ci) = comment {
            self.out.write_str("  # ")?;
            self.out.write_str(ci)?;
        }
        Ok(())
    }

    /// Append the inline comment (if any) followed by the line-terminating newline.
    /// Used after emitting a value that completes a line.
    fn finish_inline_line(&mut self, comment: Option<&str>) -> fmt::Result {
        self.push_inline_comment(comment)?;
        self.out.write_char('\n')
    }

    /// Emit `n` consecutive newlines.
    fn write_blank_lines(&mut self, n: u8) -> fmt::Result {
        for _ in 0..n {
            self.out.write_char('\n')?;
        }
        Ok(())
    }

    /// Emit a block comment (lines prefixed with `# `) at the given indentation.
    fn emit_comment_before(&mut self, comment: Option<&str>, indent: usize) -> fmt::Result {
        if let Some(cb) = comment {
            for line in cb.lines() {
                self.out.write_str(&indent_str(indent))?;
                self.out.write_str("# ")?;
                self.out.write_str(line)?;
                self.out.write_char('\n')?;
            }
        }
        Ok(())
    }

    /// Emit `node` without a trailing newline.
    /// Used for scalar values that appear inline after `: ` or `- `.
    ///
    /// `flow_context` propagates the surrounding `[ … ]` / `{ … }` context
    /// downward so quoted-scalar decisions widen as needed.
    fn emit_node_inline(
        &mut self,
        node: &YamlNode,
        indent: usize,
        flow_context: bool,
    ) -> fmt::Result {
        // Fast path: scalars (non-block), null, aliases, and flow-style containers never
        // emit trailing newlines, so we can forward directly without a temp allocation.
        let needs_strip = match node {
            YamlNode::Scalar(s) if is_block_scalar(s) => true,
            YamlNode::Mapping(m) if m.style == ContainerStyle::Block => true,
            YamlNode::Sequence(s) if s.style == ContainerStyle::Block => true,
            _ => false,
        };
        if !needs_strip {
            return self.emit_node(node, indent, flow_context);
        }
        // Slow path: emit to a temp buffer so trailing newline(s) can be stripped.
        let mut tmp = String::new();
        {
            let mut tmp_emitter = Emitter::new(&mut tmp, self.step);
            tmp_emitter.emit_node(node, indent, flow_context)?;
        }
        self.out.write_str(tmp.trim_end_matches('\n'))
    }

    /// Emit a mapping key (alias / complex / block-scalar / plain scalar).
    /// The caller is responsible for pushing any leading indentation before this call.
    fn emit_mapping_key(&mut self, key: &str, entry: &YamlEntry, indent: usize) -> fmt::Result {
        // Alias key: `? *name\n: value` — explicit form avoids ambiguity with
        // `*alias:` being misinterpreted in block context by some parsers.
        if let Some(alias) = &entry.key_alias {
            self.out.write_str("? *")?;
            self.out.write_str(alias)?;
            self.out.write_char('\n')?;
            self.out.write_str(&indent_str(indent))?;
            self.out.write_char(':')?;
        } else if let Some(key_node) = &entry.key_node {
            // Complex (non-scalar) key: `? <key_node>\n: <value>`
            self.out.write_str("? ")?;
            // For block collections, add a newline after `? ` so the content starts
            // on its own line at indent+step, avoiding `?   - item` ambiguity.
            match key_node.as_ref() {
                YamlNode::Sequence(s) if s.style == ContainerStyle::Block => {
                    self.out.write_char('\n')?;
                    self.emit_sequence(s, indent + self.step)?;
                }
                YamlNode::Mapping(m) if m.style == ContainerStyle::Block => {
                    self.out.write_char('\n')?;
                    self.emit_mapping(m, indent + self.step)?;
                }
                _ => {
                    self.emit_node(key_node, indent + self.step, false)?;
                    // Flow collections (and any other inline form) end without a
                    // newline; complex-key syntax needs `:` on its own line.
                    self.out.write_char('\n')?;
                }
            }
            self.out.write_str(&indent_str(indent))?;
            self.out.write_char(':')?;
        } else if matches!(entry.key_style, ScalarStyle::Literal | ScalarStyle::Folded) {
            // Block-scalar key: `? |\n  content\n: `
            let key_scalar = YamlScalar {
                value: ScalarValue::Str(key.to_owned()),
                style: entry.key_style,
                tag: entry.key_tag.clone(),
                original: None,
                chomping: None,
                anchor: entry.key_anchor.clone(),
                comment_inline: None,
                comment_before: None,
                blank_lines_before: 0,
            };
            self.out.write_str("? ")?;
            self.emit_block_scalar(&key_scalar, indent + self.step, None)?;
            self.out.write_str(&indent_str(indent))?;
            self.out.write_char(':')?;
        } else {
            // Plain / quoted scalar key: optional anchor + tag, then key text.
            self.write_anchor_tag_inline(entry.key_anchor.as_deref(), entry.key_tag.as_deref())?;
            self.out.write_str(&emit_key(key, entry.key_style, false))?;
            self.out.write_char(':')?;
        }
        Ok(())
    }

    /// Emit a mapping entry value (the part after the `:` on the key line).
    fn emit_mapping_value(&mut self, entry: &YamlEntry, indent: usize) -> fmt::Result {
        let inline = entry.value.comment_inline();
        match &entry.value {
            YamlNode::Mapping(nested)
                if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow =>
            {
                // Flow mapping value: emit inline on same line as key.
                self.out.write_char(' ')?;
                self.emit_mapping_flow(nested)?;
                self.finish_inline_line(inline)?;
            }
            YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                // Block mapping value: anchor + tag (if any) + inline comment, then content below.
                self.write_anchor_tag_block_suffix(
                    nested.anchor.as_deref(),
                    nested.tag.as_deref(),
                )?;
                self.finish_inline_line(inline)?;
                self.emit_mapping(nested, indent + self.step)?;
            }
            YamlNode::Sequence(nested)
                if !nested.items.is_empty() && nested.style == ContainerStyle::Flow =>
            {
                // Flow sequence value: emit inline on same line as key.
                self.out.write_char(' ')?;
                self.emit_sequence_flow(nested)?;
                self.finish_inline_line(inline)?;
            }
            YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                // Block sequence value: anchor + tag (if any) + inline comment, then content below.
                self.write_anchor_tag_block_suffix(
                    nested.anchor.as_deref(),
                    nested.tag.as_deref(),
                )?;
                self.finish_inline_line(inline)?;
                self.emit_sequence(nested, indent + self.step)?;
            }
            YamlNode::Mapping(_) => {
                // Empty mapping — always inline.
                self.push_inline_comment(inline)?;
                self.out.write_str(" {}\n")?;
            }
            YamlNode::Sequence(_) => {
                // Empty sequence — always inline.
                self.push_inline_comment(inline)?;
                self.out.write_str(" []\n")?;
            }
            YamlNode::Scalar(s) if is_block_scalar(s) => {
                // Block scalar: indicator goes on the key line; inline comment follows
                // the indicator (YAML allows `key: |  # comment`).
                self.out.write_char(' ')?;
                self.emit_block_scalar(s, indent + self.step, inline)?;
            }
            YamlNode::Scalar(s) => {
                self.out.write_char(' ')?;
                self.emit_scalar(s, false)?;
                self.finish_inline_line(inline)?;
            }
            node => {
                self.out.write_char(' ')?;
                self.emit_node_inline(node, indent + self.step, false)?;
                self.finish_inline_line(inline)?;
            }
        }
        Ok(())
    }

    fn emit_mapping(&mut self, m: &YamlMapping, indent: usize) -> fmt::Result {
        if m.style == ContainerStyle::Flow {
            return self.emit_mapping_flow(m);
        }
        // Top-level anchor: emit `&name` on its own line before the entries.
        // Nested anchors are already emitted by emit_mapping_value / emit_sequence;
        // all non-top-level calls to emit_mapping pass indent > 0.
        if indent == 0
            && let Some(anchor) = &m.anchor
        {
            self.out.write_char('&')?;
            self.out.write_str(anchor)?;
            self.out.write_char('\n')?;
        }
        if m.entries.is_empty() {
            return self.out.write_str("{}\n");
        }
        for (key, entry) in &m.entries {
            self.write_blank_lines(entry.value.blank_lines_before())?;
            self.emit_comment_before(entry.value.comment_before(), indent)?;
            self.out.write_str(&indent_str(indent))?;
            self.emit_mapping_key(key, entry, indent)?;
            self.emit_mapping_value(entry, indent)?;
        }
        self.write_blank_lines(m.trailing_blank_lines)?;
        Ok(())
    }

    fn emit_mapping_flow(&mut self, m: &YamlMapping) -> fmt::Result {
        self.write_anchor_tag_inline(m.anchor.as_deref(), m.tag.as_deref())?;
        self.out.write_char('{')?;
        let mut first = true;
        for (key, entry) in &m.entries {
            if !first {
                self.out.write_str(", ")?;
            }
            first = false;
            // Emit key: complex key_node, alias key, or plain scalar key.
            if let Some(key_node) = &entry.key_node {
                // Flow context supports `? <node>: <value>` or plain `<node>: <value>` syntax.
                self.emit_node_inline(key_node, 0, true)?;
            } else if let Some(alias) = &entry.key_alias {
                self.out.write_char('*')?;
                self.out.write_str(alias)?;
                // Space required: colon is a valid anchor-name character per YAML spec,
                // so `*alias:` is parsed as alias `alias:` rather than alias `alias` + `:`.
                self.out.write_char(' ')?;
            } else {
                self.write_anchor_tag_inline(
                    entry.key_anchor.as_deref(),
                    entry.key_tag.as_deref(),
                )?;
                self.out.write_str(&emit_key(key, entry.key_style, true))?;
            }
            self.out.write_str(": ")?;
            self.emit_node_inline(&entry.value, 0, true)?;
        }
        self.out.write_char('}')
    }

    /// Emit a tagged/commented container nested in a sequence on a separate
    /// line, or use the inline-first variant when neither a tag, anchor, nor
    /// inline comment is present.
    ///
    /// `indent` is the indentation of the nested container's content (i.e. the
    /// caller has already added its `step`).
    fn emit_nested_in_seq(
        &mut self,
        kind: NestedKind<'_>,
        anchor: Option<&str>,
        tag: Option<&str>,
        comment_inline: Option<&str>,
        indent: usize,
    ) -> fmt::Result {
        if anchor.is_some() || tag.is_some() || comment_inline.is_some() {
            if let Some(anchor) = anchor {
                self.out.write_char('&')?;
                self.out.write_str(anchor)?;
                if tag.is_some() || comment_inline.is_some() {
                    self.out.write_char(' ')?;
                }
            }
            if let Some(tag) = tag {
                self.out.write_str(&format_tag(tag))?;
                if comment_inline.is_some() {
                    self.out.write_char(' ')?;
                }
            }
            if let Some(ci) = comment_inline {
                self.out.write_str("# ")?;
                self.out.write_str(ci)?;
            }
            self.out.write_char('\n')?;
            match kind {
                NestedKind::Mapping(m) => self.emit_mapping(m, indent),
                NestedKind::Sequence(s) => self.emit_sequence(s, indent),
            }
        } else {
            match kind {
                NestedKind::Mapping(m) => self.emit_mapping_inline_first(m, indent),
                NestedKind::Sequence(s) => self.emit_sequence_inline_first(s, indent),
            }
        }
    }

    fn emit_sequence(&mut self, s: &YamlSequence, indent: usize) -> fmt::Result {
        if s.style == ContainerStyle::Flow {
            return self.emit_sequence_flow(s);
        }
        // Top-level anchor: emit `&name` on its own line before the items.
        // All non-top-level calls to emit_sequence pass indent > 0.
        if indent == 0
            && let Some(anchor) = &s.anchor
        {
            self.out.write_char('&')?;
            self.out.write_str(anchor)?;
            self.out.write_char('\n')?;
        }
        if s.items.is_empty() {
            return self.out.write_str("[]\n");
        }
        for item in &s.items {
            self.write_blank_lines(item.blank_lines_before())?;
            self.emit_comment_before(item.comment_before(), indent)?;
            self.out.write_str(&indent_str(indent))?;
            self.out.write_str("- ")?;
            let inline = item.comment_inline();
            match item {
                YamlNode::Mapping(nested)
                    if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow =>
                {
                    // Flow mapping in sequence: emit inline
                    self.emit_mapping_flow(nested)?;
                    self.finish_inline_line(inline)?;
                }
                YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                    self.emit_nested_in_seq(
                        NestedKind::Mapping(nested),
                        nested.anchor.as_deref(),
                        nested.tag.as_deref(),
                        inline,
                        indent + self.step,
                    )?;
                }
                YamlNode::Sequence(nested)
                    if !nested.items.is_empty() && nested.style == ContainerStyle::Flow =>
                {
                    // Flow sequence in sequence: emit inline
                    self.emit_sequence_flow(nested)?;
                    self.finish_inline_line(inline)?;
                }
                YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                    self.emit_nested_in_seq(
                        NestedKind::Sequence(nested),
                        nested.anchor.as_deref(),
                        nested.tag.as_deref(),
                        inline,
                        indent + self.step,
                    )?;
                }
                YamlNode::Scalar(scalar) if is_block_scalar(scalar) => {
                    // Block scalar directly in sequence
                    self.emit_block_scalar(scalar, indent + self.step, inline)?;
                }
                YamlNode::Scalar(scalar) => {
                    self.emit_scalar(scalar, false)?;
                    self.finish_inline_line(inline)?;
                }
                node => {
                    self.emit_node_inline(node, indent + self.step, false)?;
                    self.finish_inline_line(inline)?;
                }
            }
        }
        self.write_blank_lines(s.trailing_blank_lines)?;
        Ok(())
    }

    fn emit_sequence_flow(&mut self, s: &YamlSequence) -> fmt::Result {
        self.write_anchor_tag_inline(s.anchor.as_deref(), s.tag.as_deref())?;
        self.out.write_char('[')?;
        let mut first = true;
        for item in &s.items {
            if !first {
                self.out.write_str(", ")?;
            }
            first = false;
            // A block mapping/sequence nested inside a flow sequence must be emitted as
            // flow — block syntax is invalid inside flow context.
            match item {
                YamlNode::Mapping(m) if m.style == ContainerStyle::Block => {
                    self.emit_mapping_flow(m)?;
                }
                YamlNode::Sequence(inner) if inner.style == ContainerStyle::Block => {
                    self.emit_sequence_flow(inner)?;
                }
                node => self.emit_node_inline(node, 0, true)?,
            }
        }
        self.out.write_char(']')
    }

    /// Emit a sequence where the first item shares the line with the parent `-`.
    fn emit_sequence_inline_first(&mut self, s: &YamlSequence, indent: usize) -> fmt::Result {
        let mut first = true;
        for item in &s.items {
            if !first {
                self.write_blank_lines(item.blank_lines_before())?;
            }
            let before = item.comment_before();
            let inline = item.comment_inline();
            if before.is_some() {
                if first {
                    // Can't put a before-comment on the same line as the parent `-`
                    self.out.write_char('\n')?;
                }
                self.emit_comment_before(before, indent)?;
                self.out.write_str(&indent_str(indent))?;
            } else if !first {
                self.out.write_str(&indent_str(indent))?;
            }
            self.out.write_str("- ")?;
            match item {
                YamlNode::Mapping(nested)
                    if !nested.entries.is_empty() && nested.style == ContainerStyle::Flow =>
                {
                    self.emit_mapping_flow(nested)?;
                    self.finish_inline_line(inline)?;
                }
                YamlNode::Mapping(nested) if !nested.entries.is_empty() => {
                    if let Some(ci) = inline {
                        self.out.write_str("# ")?;
                        self.out.write_str(ci)?;
                        self.out.write_char('\n')?;
                        self.emit_mapping(nested, indent + self.step)?;
                    } else {
                        self.emit_mapping_inline_first(nested, indent + self.step)?;
                    }
                }
                YamlNode::Sequence(nested)
                    if !nested.items.is_empty() && nested.style == ContainerStyle::Flow =>
                {
                    self.emit_sequence_flow(nested)?;
                    self.finish_inline_line(inline)?;
                }
                YamlNode::Sequence(nested) if !nested.items.is_empty() => {
                    if let Some(ci) = inline {
                        self.out.write_str("# ")?;
                        self.out.write_str(ci)?;
                        self.out.write_char('\n')?;
                        self.emit_sequence(nested, indent + self.step)?;
                    } else {
                        self.emit_sequence_inline_first(nested, indent + self.step)?;
                    }
                }
                YamlNode::Scalar(scalar) if is_block_scalar(scalar) => {
                    self.emit_block_scalar(scalar, indent + self.step, inline)?;
                }
                YamlNode::Scalar(scalar) => {
                    self.emit_scalar(scalar, false)?;
                    self.finish_inline_line(inline)?;
                }
                node => {
                    self.emit_node_inline(node, indent + self.step, false)?;
                    self.finish_inline_line(inline)?;
                }
            }
            first = false;
        }
        Ok(())
    }

    /// Emit a mapping where the first entry shares the line with the parent `-`.
    fn emit_mapping_inline_first(&mut self, m: &YamlMapping, indent: usize) -> fmt::Result {
        let mut first = true;
        for (key, entry) in &m.entries {
            let before = entry.value.comment_before();
            if before.is_some() {
                if first {
                    // Can't put before-comment on the same line as `-`; put it on a new line.
                    self.out.write_char('\n')?;
                }
                self.emit_comment_before(before, indent)?;
                self.out.write_str(&indent_str(indent))?;
            } else if !first {
                self.out.write_str(&indent_str(indent))?;
            }
            // For the first entry the cursor is already positioned after `- ` by the caller.
            self.emit_mapping_key(key, entry, indent)?;
            self.emit_mapping_value(entry, indent)?;
            first = false;
        }
        Ok(())
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
        &mut self,
        s: &YamlScalar,
        indent: usize,
        inline_comment: Option<&str>,
    ) -> fmt::Result {
        let indicator = if s.style == ScalarStyle::Literal {
            '|'
        } else {
            '>'
        };
        let content = match &s.value {
            ScalarValue::Str(text) => text.as_str(),
            _ => "",
        };
        // Emit all lines except the artifact empty string produced by a trailing '\n'.
        let lines: Vec<&str> = content.split('\n').collect();
        // Choose chomping indicator. Honour the source indicator (`s.chomping`)
        // when it's consistent with the value's trailing-newline count, so that
        // `>+` on a value with exactly one trailing `\n` round-trips as `>+`
        // rather than the inference-picked `>`. Fall back to pure inference
        // when the field is absent or when a value mutation has made the
        // stored indicator inconsistent (e.g. loaded `>-`, value mutated to
        // have trailings — `>-` would strip them, losing data).
        //
        // Consistency rules:
        //   Strip  → requires 0 trailings
        //   Clip   → requires exactly 1 trailing AND at least one non-empty
        //            content line (clip drops all trailings when content is
        //            blank, so a `"\n"`-only value parses back as `""`)
        //   Keep   → always consistent (preserves whatever is there)
        let trailing_newlines = content.bytes().rev().take_while(|&b| b == b'\n').count();
        let emit_count = if content.ends_with('\n') {
            lines.len() - 1
        } else {
            lines.len()
        };
        let has_content_line = lines[..emit_count].iter().any(|l| !l.is_empty());
        let chomping = match s.chomping {
            Some(Chomping::Strip) if trailing_newlines == 0 => "-",
            Some(Chomping::Clip) if trailing_newlines == 1 && has_content_line => "",
            Some(Chomping::Keep) => "+",
            _ => match (trailing_newlines, has_content_line) {
                (0, _) => "-",
                (1, true) => "",
                // Newline-only content or 2+ trailings: only keep can preserve
                // every trailing break on re-parse.
                _ => "+",
            },
        };
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
        let mut min_leading = usize::MAX;
        let mut first_leading: Option<usize> = None;
        for line in lines.iter().filter(|l| !l.is_empty()) {
            let n = line.bytes().take_while(|&b| b == b' ').count();
            if first_leading.is_none() {
                first_leading = Some(n);
            }
            min_leading = min_leading.min(n);
        }
        let min_leading: usize = if min_leading == usize::MAX {
            0
        } else {
            min_leading
        };
        let first_leading: usize = first_leading.unwrap_or(0);

        let (explicit_indicator, content_indent) = if min_leading > 0 {
            // Case A
            (min_leading, indent.saturating_sub(min_leading))
        } else if first_leading > 0 {
            // Case B — explicit indicator equals the indent step so that the parser
            // (using base = parent_indent + N) strips exactly content_indent + N = indent
            // spaces, leaving the stored leading spaces in the value.
            (self.step, indent)
        } else {
            // Case C
            (0, indent)
        };

        self.write_anchor_tag_inline(s.anchor.as_deref(), s.tag.as_deref())?;
        self.out.write_char(indicator)?;
        if explicit_indicator > 0 {
            // Digit before chomping (YAML spec allows either order; digit-first is conventional).
            let digit = u32::try_from(explicit_indicator).unwrap_or(1);
            self.out
                .write_char(char::from_digit(digit, 10).unwrap_or('1'))?;
        }
        self.out.write_str(chomping)?;
        self.finish_inline_line(inline_comment)?;
        let prefix = indent_str(content_indent);
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
                    self.out.write_char('\n')?; // blank line preserved from stored value
                } else {
                    self.out.write_str(&prefix)?;
                    self.out.write_str(line)?;
                    self.out.write_char('\n')?;
                    // A separator is needed when:
                    //   • the current line is base-level (not more-indented, not whitespace-only)
                    //   • the next non-empty content line is also base-level
                    let is_more_indented = |s: &str| s.starts_with(' ') || s.starts_with('\t');
                    let next_non_empty_is_base = i + 1 < emit_count
                        && lines[i + 1..emit_count]
                            .iter()
                            .find(|l| !l.is_empty())
                            .is_some_and(|l| !is_more_indented(l));
                    let needs_sep = !is_more_indented(line)
                        && !line.trim().is_empty()
                        && next_non_empty_is_base;
                    if needs_sep {
                        self.out.write_char('\n')?; // paragraph separator
                    }
                }
            }
        } else {
            // Literal: emit lines verbatim.
            for line in &lines[..emit_count] {
                if line.is_empty() {
                    self.out.write_char('\n')?; // blank line inside block scalar — no indent
                } else {
                    self.out.write_str(&prefix)?;
                    self.out.write_str(line)?;
                    self.out.write_char('\n')?;
                }
            }
        }
        Ok(())
    }

    /// Emit a scalar value in the appropriate style.
    ///
    /// `flow_context` is true when the scalar is being written inside `[ … ]` or
    /// `{ … }`; it widens the set of strings that need quoting (see
    /// [`needs_quoting`]).
    fn emit_scalar(&mut self, s: &YamlScalar, flow_context: bool) -> fmt::Result {
        self.write_anchor_tag_inline(s.anchor.as_deref(), s.tag.as_deref())?;
        // Use preserved source text when available (e.g. float exponent form `1.5e10`,
        // non-canonical null/bool/int forms, tagged plain scalars).
        if let Some(orig) = &s.original {
            return self.out.write_str(orig);
        }
        self.out.write_str(&emit_scalar_value_with_style(
            &s.value,
            s.style,
            flow_context,
        ))
    }
}

fn emit_scalar_value_with_style(
    v: &ScalarValue,
    style: ScalarStyle,
    flow_context: bool,
) -> Cow<'_, str> {
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
        ScalarValue::Str(s) => Cow::Owned(emit_string_with_style(s, style, flow_context)),
    }
}

/// Emit a key string with its original quoting style.
///
/// For `Plain` style, numeric-looking strings are left unquoted since `1:` is
/// valid YAML and our library always stores keys as strings anyway.
///
/// `flow_context` is `true` when emitting a key inside `{ … }` — flow
/// indicators terminate a plain key there, so they force quoting.
///
/// Any style containing a character that can't survive plain/single-quoted
/// emission (C0 controls other than `\t`, plus DEL) is upgraded to double-
/// quoted so the char is preserved via its `\…` escape.
fn emit_key(key: &str, style: ScalarStyle, flow_context: bool) -> String {
    if needs_double_quote(key) {
        return double_quote(key);
    }
    match style {
        ScalarStyle::SingleQuoted => single_quote(key),
        ScalarStyle::DoubleQuoted => double_quote(key),
        _ => {
            if needs_quoting_for_key(key, flow_context) {
                single_quote(key)
            } else {
                key.to_owned()
            }
        }
    }
}

/// Like `needs_quoting` but for mapping keys.
///
/// yarutsk stores all mapping keys as strings regardless of their YAML
/// type, so its own round-trip is safe either way — but another YAML
/// parser (ruamel, pyyaml) would load `true:`, `42:`, or `null:` as a
/// boolean / int / None *key*, yielding a different Python dict. To keep
/// the output interop-safe we quote keys whose plain form would re-parse
/// as a non-string scalar (same rule as `needs_quoting`).
///
/// `flow_context` is `true` for keys inside `{ … }` — flow indicators
/// terminate a plain key there, so they force quoting.
fn needs_quoting_for_key(s: &str, flow_context: bool) -> bool {
    if s.is_empty() {
        return true;
    }
    // Plain keys have leading/trailing whitespace stripped on re-parse.
    if s.starts_with([' ', '\t']) || s.ends_with([' ', '\t']) {
        return true;
    }
    // Document-start/end markers would collide with YAML directives on re-parse.
    if s == "---" || s == "..." {
        return true;
    }
    // Keyword / numeric lookalikes — quoted so other YAML parsers read the
    // key as a string rather than resolving it to bool/int/float/null.
    if would_parse_as_non_string(s) {
        return true;
    }
    // In a flow mapping, an embedded flow indicator ends the key.
    if flow_context
        && s.bytes()
            .any(|b| matches!(b, b',' | b'[' | b']' | b'{' | b'}'))
    {
        return true;
    }
    // Check for characters that are structurally significant in YAML.
    // Per YAML 1.2 the c-indicators that can never start a plain scalar are:
    //   # & * ! | > % @ ` { } [ ] , ' "
    // The conditional indicators `-` `?` `:` are safe as the first character
    // when followed by a non-whitespace byte (in flow context, also not a
    // flow indicator) — `: ` / `": "` / trailing `:` are caught below.
    let b = s.as_bytes();
    let first = b[0] as char;
    if matches!(
        first,
        '#' | '&'
            | '*'
            | '|'
            | '>'
            | '!'
            | '%'
            | '@'
            | '`'
            | '{'
            | '}'
            | '['
            | ']'
            | ','
            | '\''
            | '"'
    ) {
        return true;
    }
    if matches!(first, '-' | '?') {
        match b.get(1) {
            None | Some(b' ' | b'\t') => return true,
            _ => {}
        }
    }
    if s.contains(": ") || s.starts_with(": ") || s.ends_with(':') {
        return true;
    }
    if s.contains(" #") {
        return true;
    }
    // C0 controls (other than `\t`) and DEL can't appear raw in plain form;
    // force quoting so emission can route them through a double-quoted escape.
    if needs_double_quote(s) {
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
///
/// `flow_context` is forwarded to [`needs_quoting`].
fn emit_string_with_style(s: &str, style: ScalarStyle, flow_context: bool) -> String {
    match style {
        ScalarStyle::SingleQuoted => {
            // Single-quoted strings fold literal line breaks to a space, strip
            // CR, and cannot hold other C0 controls or DEL. Switch to double-
            // quoted (with `\…` escapes) whenever any of those appear, so the
            // value is preserved on round-trip.
            if needs_double_quote(s) {
                double_quote(s)
            } else {
                single_quote(s)
            }
        }
        ScalarStyle::DoubleQuoted => double_quote(s),
        ScalarStyle::Literal | ScalarStyle::Folded => {
            // Should have been handled by emit_block_scalar; if we reach here the node
            // is in a context where block scalars are not valid (e.g. flow or key
            // position).  Use double-quoted when line breaks or other controls
            // are present so they become escape sequences rather than literal
            // characters, which would break the surrounding structure.
            if needs_quoting(s, flow_context) {
                if needs_double_quote(s) {
                    double_quote(s)
                } else {
                    single_quote(s)
                }
            } else {
                s.to_owned()
            }
        }
        ScalarStyle::Plain => {
            if needs_quoting(s, flow_context) {
                if needs_double_quote(s) {
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

/// Return true if the string contains a character that cannot survive
/// plain/single-quoted emission: any C0 control other than `\t`, or DEL.
/// These characters must be escaped via a `\…` sequence in a double-
/// quoted scalar to round-trip.
#[inline]
fn needs_double_quote(s: &str) -> bool {
    s.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7F)
}

/// Return `true` when the bare string would re-parse as a non-string YAML
/// scalar (bool, null, int, or float) under the spec's resolution rules.
///
/// Used by both the value- and key-side quoting checks so any string whose
/// plain form would be mis-interpreted as another type gets quoted. Mirrors
/// `ScalarValue::from_str`, but works on `&str` without allocating.
fn would_parse_as_non_string(s: &str) -> bool {
    #[allow(clippy::match_same_arms)]
    match s {
        "null" | "Null" | "NULL" | "~" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES"
        | "on" | "On" | "ON" | "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off"
        | "OFF" | ".inf" | ".Inf" | ".INF" | "-.inf" | "-.Inf" | "-.INF" | ".nan" | ".NaN"
        | ".NAN" => return true,
        _ => {}
    }
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    // Numeric: hex/octal prefix → int; decimal int; float with `.` or `e`/`E`.
    let start = usize::from(b[0] == b'-' || b[0] == b'+');
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
    false
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
            '\0' => out.push_str("\\0"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0B' => out.push_str("\\v"),
            '\x0C' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\x1B' => out.push_str("\\e"),
            // Remaining C0 controls + DEL: `\xNN` hex escape.
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\x{:02X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Return true if the string needs to be quoted in YAML plain style.
///
/// `flow_context` widens the unsafe set: inside `[ … ]` / `{ … }` the flow
/// indicators `,` `[` `]` `{` `}` terminate a plain scalar wherever they
/// appear, not just at the leading position.
fn needs_quoting(s: &str, flow_context: bool) -> bool {
    if s.is_empty() {
        return true;
    }
    // Plain scalars have leading/trailing whitespace stripped on re-parse.
    if s.starts_with([' ', '\t']) || s.ends_with([' ', '\t']) {
        return true;
    }
    // Inside a flow collection, an embedded flow indicator ends the scalar.
    if flow_context
        && s.bytes()
            .any(|b| matches!(b, b',' | b'[' | b']' | b'{' | b'}'))
    {
        return true;
    }
    // Force quotes on any string whose plain form would be resolved as a
    // non-string YAML scalar (bool, null, int, float).
    if would_parse_as_non_string(s) {
        return true;
    }
    // Document-start/end markers: emitting plain would collide with
    // directive-end / document-end markers on re-parse.
    if s == "---" || s == "..." {
        return true;
    }
    let b = s.as_bytes();
    // Check for characters that are structurally significant in YAML.
    // Per YAML 1.2 the c-indicators that can never start a plain scalar are:
    //   # & * ! | > % @ ` { } [ ] , ' "
    // The conditional indicators `-` `?` `:` are safe as the first character
    // when followed by a non-whitespace byte. In flow context an embedded
    // flow indicator is already caught above. `: ` / `": "` / trailing `:`
    // are caught below.
    let first = b[0] as char;
    if matches!(
        first,
        '#' | '&'
            | '*'
            | '|'
            | '>'
            | '!'
            | '%'
            | '@'
            | '`'
            | '{'
            | '}'
            | '['
            | ']'
            | ','
            | '\''
            | '"'
    ) {
        return true;
    }
    if matches!(first, '-' | '?') {
        match b.get(1) {
            None | Some(b' ' | b'\t') => return true,
            _ => {}
        }
    }
    if s.contains(": ") || s.starts_with(": ") || s.ends_with(':') {
        return true;
    }
    if s.contains(" #") {
        return true;
    }
    // C0 controls (other than `\t`) and DEL can't appear raw in plain form;
    // force quoting so emission can route them through a double-quoted escape.
    if needs_double_quote(s) {
        return true;
    }
    false
}

// ─── Indentation / tag / scalar-style helpers ────────────────────────────────

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

/// Format a stored tag for emission.
///
/// Three cases:
/// - `tag:yaml.org,2002:T` → `!!T`  (built-in YAML secondary handle)
/// - `!…` (already starts with `!`) → returned unchanged  (`!custom`, `!local`, …)
/// - any other full URI `tag:…` → `!<tag:…>`  (YAML verbatim-tag form)
fn format_tag(tag: &str) -> Cow<'_, str> {
    if let Some(suffix) = tag.strip_prefix("tag:yaml.org,2002:") {
        Cow::Owned(format!("!!{}", pct_encode_shorthand(suffix)))
    } else if let Some(suffix) = tag.strip_prefix("!!") {
        Cow::Owned(format!("!!{}", pct_encode_shorthand(suffix)))
    } else if let Some(suffix) = tag.strip_prefix('!') {
        Cow::Owned(format!("!{}", pct_encode_shorthand(suffix)))
    } else {
        // Verbatim form `!<URI>` — the angle brackets delimit the URI, so
        // flow indicators inside are unambiguous. Only chars outside the URI
        // char set need percent-encoding (spaces, controls, `<` / `>`).
        Cow::Owned(format!("!<{}>", pct_encode_uri(tag)))
    }
}

/// Percent-encode any character that isn't a valid YAML tag character.
/// Used for shorthand tags (`!foo`, `!!foo`) where flow indicators would
/// terminate the tag — so they must be escaped.
fn pct_encode_shorthand(s: &str) -> Cow<'_, str> {
    pct_encode_with(s, is_tag_char)
}

/// Percent-encode any character that isn't a valid URI character.
/// Used for verbatim tags (`!<…>`) where the angle brackets delimit the
/// URI, so flow indicators (`,` `[` `]` `{` `}`) are allowed.
fn pct_encode_uri(s: &str) -> Cow<'_, str> {
    pct_encode_with(s, is_uri_char)
}

fn pct_encode_with(s: &str, is_allowed: fn(char) -> bool) -> Cow<'_, str> {
    if s.chars().all(|c| c.is_ascii() && is_allowed(c)) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() && is_allowed(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for &b in c.encode_utf8(&mut buf).as_bytes() {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    Cow::Owned(out)
}

/// Returns true if this scalar should be emitted as a block scalar (`|` or `>`).
///
/// A block scalar can only carry printable text plus `\t` and `\n`. The YAML
/// parser normalises `\r\n` and lone `\r` to `\n` inside block scalars, and
/// other C0 controls (and DEL) aren't permitted there at all. When the value
/// contains any of those, we deny block emission and let
/// [`emit_string_with_style`] fall back to double-quoted, which can encode
/// every code point as a `\…` escape.
fn is_block_scalar(s: &YamlScalar) -> bool {
    if !matches!(s.style, ScalarStyle::Literal | ScalarStyle::Folded) {
        return false;
    }
    if let ScalarValue::Str(text) = &s.value {
        if text
            .bytes()
            .any(|b| b == b'\r' || b == 0x7F || (b < 0x20 && b != b'\t' && b != b'\n'))
        {
            return false;
        }
    }
    true
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Emit a full document list to any `fmt::Write` sink.
/// `explicit_starts[i]` and `explicit_ends[i]` control whether `---` / `...` are emitted.
/// `yaml_versions[i]` emits a `%YAML` directive before `---` when `Some`.
/// `tag_directives[i]` emits `%TAG` directives before `---` when non-empty.
/// `indent_step` is the per-level indentation increment (default: 2).
/// Any slice may be shorter than `docs`; missing entries default to `false` / `None` / empty.
pub fn emit_docs_to<W: FmtWrite>(
    docs: &[YamlNode],
    meta: &[DocMetadata],
    indent_step: usize,
    out: &mut W,
) -> fmt::Result {
    let step = if indent_step == 0 { 2 } else { indent_step };
    let empty_meta = DocMetadata::default();
    for (i, doc) in docs.iter().enumerate() {
        let m = meta.get(i).unwrap_or(&empty_meta);
        let want_start = m.explicit_start;
        let want_end = m.explicit_end;
        let version = m.yaml_version;
        let tags = m.tag_directives.as_slice();
        let has_directives = version.is_some() || !tags.is_empty();
        if has_directives || docs.len() > 1 || want_start {
            if let Some((major, minor)) = version {
                writeln!(out, "%YAML {major}.{minor}")?;
            }
            for (handle, prefix) in tags {
                writeln!(out, "%TAG {handle} {prefix}")?;
            }
            out.write_str("---\n")?;
        }
        // Use LastCharTracker to detect whether the node ended with a newline.
        {
            let mut tracker = LastCharTracker::new(&mut *out);
            {
                let mut emitter = Emitter::new(&mut tracker, step);
                // Root nodes have no parent to emit their blank_lines_before /
                // comment_before, so surface them here. Container roots have no
                // header line for comment_inline, so that is only handled for
                // scalar roots below.
                emitter.write_blank_lines(doc.blank_lines_before())?;
                emitter.emit_comment_before(doc.comment_before(), 0)?;
                if let YamlNode::Scalar(s) = doc {
                    if is_block_scalar(s) {
                        emitter.emit_block_scalar(s, step, s.comment_inline.as_deref())?;
                    } else {
                        emitter.emit_scalar(s, false)?;
                        emitter.push_inline_comment(s.comment_inline.as_deref())?;
                    }
                } else {
                    emitter.emit_node(doc, 0, false)?;
                }
            }
            if !tracker.ends_with_newline() {
                tracker.write_char('\n')?;
            }
        }
        if want_end {
            out.write_str("...\n")?;
        }
    }
    Ok(())
}

/// Emit a full document list to a `String`.
/// Convenience wrapper around [`emit_docs_to`] for callers that need a `String`.
#[must_use]
pub fn emit_docs(docs: &[YamlNode], meta: &[DocMetadata], indent_step: usize) -> String {
    let mut out = String::with_capacity(256);
    emit_docs_to(docs, meta, indent_step, &mut out).expect("writing to String is infallible");
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
            chomping: None,
            anchor: None,
            comment_inline: None,
            comment_before: None,
            blank_lines_before: 0,
        })
    }

    fn plain_int(n: i64) -> YamlNode {
        YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Int(n),
            style: ScalarStyle::Plain,
            tag: None,
            original: None,
            chomping: None,
            anchor: None,
            comment_inline: None,
            comment_before: None,
            blank_lines_before: 0,
        })
    }

    fn scalar_emit(s: &YamlScalar) -> String {
        let mut out = String::new();
        Emitter::new(&mut out, 2)
            .emit_scalar(s, false)
            .expect("emit to String is infallible");
        out
    }

    fn node_emit(node: &YamlNode) -> String {
        let mut out = String::new();
        Emitter::new(&mut out, 2)
            .emit_node(node, 0, false)
            .expect("emit to String is infallible");
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
            tag: tag.map(std::borrow::ToOwned::to_owned),
            original: original.map(std::borrow::ToOwned::to_owned),
            chomping: None,
            anchor: anchor.map(std::borrow::ToOwned::to_owned),
            comment_inline: None,
            comment_before: None,
            blank_lines_before: 0,
        }
    }

    fn make_entry(value: YamlNode) -> YamlEntry {
        YamlEntry {
            value,
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
            s.items.push(item.clone());
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
            ScalarValue::Float(2.5),
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
            comment_inline: None,
            comment_before: None,
            blank_lines_before: 0,
        };
        assert_eq!(node_emit(&node), "*myref");
    }

    // ── emit_docs: single doc ─────────────────────────────────────────────────

    fn meta(explicit_start: bool, explicit_end: bool) -> DocMetadata {
        DocMetadata {
            explicit_start,
            explicit_end,
            yaml_version: None,
            tag_directives: Vec::new(),
        }
    }

    #[test]
    fn emit_single_doc_no_markers() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[meta(false, false)], 2);
        assert_eq!(out, "a: 1\n");
    }

    #[test]
    fn emit_single_doc_with_start_marker() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[meta(true, false)], 2);
        assert_eq!(out, "---\na: 1\n");
    }

    #[test]
    fn emit_single_doc_with_end_marker() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[meta(false, true)], 2);
        assert_eq!(out, "a: 1\n...\n");
    }

    #[test]
    fn emit_single_doc_with_both_markers() {
        let m = YamlNode::Mapping(make_mapping(&[("a", plain_int(1))]));
        let out = emit_docs(&[m], &[meta(true, true)], 2);
        assert_eq!(out, "---\na: 1\n...\n");
    }

    // ── emit_docs: multiple docs ──────────────────────────────────────────────

    #[test]
    fn emit_two_docs_adds_start_markers() {
        let d1 = plain_str("hello");
        let d2 = plain_str("world");
        let out = emit_docs(&[d1, d2], &[meta(false, false), meta(false, false)], 2);
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
        let out = emit_docs(&[], &[], 2);
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
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "z: 1");
        assert_eq!(lines[1], "a: 2");
        assert_eq!(lines[2], "m: 3");
    }

    #[test]
    fn emit_empty_mapping() {
        let m = make_mapping(&[]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "{}\n");
    }

    #[test]
    fn emit_flow_mapping() {
        let mut m = make_mapping(&[("a", plain_int(1)), ("b", plain_int(2))]);
        m.style = ContainerStyle::Flow;
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "{a: 1, b: 2}");
    }

    #[test]
    fn emit_mapping_with_inline_comment() {
        let mut m = YamlMapping::new();
        let mut value = plain_int(1);
        value.set_comment_inline(Some("a comment".to_owned()));
        m.entries.insert("key".to_owned(), make_entry(value));
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "key: 1  # a comment\n");
    }

    #[test]
    fn emit_mapping_with_before_comment() {
        let mut m = YamlMapping::new();
        let mut value = plain_int(1);
        value.set_comment_before(Some("header".to_owned()));
        m.entries.insert("key".to_owned(), make_entry(value));
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "# header\nkey: 1\n");
    }

    #[test]
    fn emit_mapping_with_blank_line_before_entry() {
        let mut m = make_mapping(&[("a", plain_int(1)), ("b", plain_int(2))]);
        m.entries
            .get_mut("b")
            .unwrap()
            .value
            .set_blank_lines_before(1);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "a: 1\n\nb: 2\n");
    }

    #[test]
    fn emit_mapping_with_empty_nested_mapping() {
        let empty = YamlNode::Mapping(make_mapping(&[]));
        let m = make_mapping(&[("key", empty)]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "key: {}\n");
    }

    #[test]
    fn emit_mapping_with_empty_nested_sequence() {
        let empty = YamlNode::Sequence(make_seq(&[]));
        let m = make_mapping(&[("key", empty)]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert_eq!(out, "key: []\n");
    }

    // ── emit sequence ─────────────────────────────────────────────────────────

    #[test]
    fn emit_sequence_items() {
        let s = make_seq(&[plain_int(1), plain_int(2), plain_int(3)]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Sequence(s), 0, false);
        assert_eq!(out, "- 1\n- 2\n- 3\n");
    }

    #[test]
    fn emit_empty_sequence() {
        let s = make_seq(&[]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Sequence(s), 0, false);
        assert_eq!(out, "[]\n");
    }

    #[test]
    fn emit_flow_sequence() {
        let mut s = make_seq(&[plain_int(1), plain_int(2), plain_int(3)]);
        s.style = ContainerStyle::Flow;
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Sequence(s), 0, false);
        assert_eq!(out, "[1, 2, 3]");
    }

    #[test]
    fn emit_complex_key_flow_seq_puts_colon_on_next_line() {
        // `? [1, 2]` needs `\n: value` — the `:` must not share the line
        // with the flow key or it becomes part of the flow scalar and the
        // mapping entry is unparseable on re-load.
        let mut key_seq = make_seq(&[plain_int(1), plain_int(2)]);
        key_seq.style = ContainerStyle::Flow;
        let mut mapping = YamlMapping::new();
        let mut entry = make_entry(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Str("value".to_owned()),
            style: ScalarStyle::Plain,
            tag: None,
            original: None,
            chomping: None,
            anchor: None,
            comment_inline: None,
            comment_before: None,
            blank_lines_before: 0,
        }));
        entry.key_node = Some(Box::new(YamlNode::Sequence(key_seq)));
        mapping.entries.insert(String::new(), entry);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(mapping), 0, false);
        assert_eq!(out, "? [1, 2]\n: value\n");
    }

    #[test]
    fn emit_sequence_with_inline_comment() {
        let mut s = YamlSequence::new();
        let mut value = plain_int(1);
        value.set_comment_inline(Some("one".to_owned()));
        s.items.push(value);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Sequence(s), 0, false);
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
                chomping: None,
                anchor: None,
                comment_inline: None,
                comment_before: None,
                blank_lines_before: 0,
            }),
        )]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
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
                chomping: None,
                anchor: None,
                comment_inline: None,
                comment_before: None,
                blank_lines_before: 0,
            }),
        )]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
        assert!(
            out.starts_with("text: >\n"),
            "expected folded indicator: {out}"
        );
    }

    #[test]
    fn emit_root_block_scalar_indents_content() {
        // A block scalar at document root must emit content at column >= step.
        // Content at column 0 is mis-parsed: `#` becomes a comment and the next
        // document's `---` marker gets folded into the scalar's content.
        for style in [ScalarStyle::Folded, ScalarStyle::Literal] {
            let doc = YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str("#\n".to_owned()),
                style,
                tag: None,
                original: None,
                chomping: None,
                anchor: None,
                comment_inline: None,
                comment_before: None,
                blank_lines_before: 0,
            });
            let out = emit_docs(
                &[doc, YamlNode::Null],
                &[DocMetadata::default(), DocMetadata::default()],
                2,
            );
            let re = crate::core::builder::parse_str(&out, None).expect("re-parse failed");
            assert_eq!(
                re.docs.len(),
                2,
                "doc count drift for {style:?}: emitted:\n{out}"
            );
        }
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
                    chomping: None,
                    anchor: None,
                    comment_inline: None,
                    comment_before: None,
                    blank_lines_before: 0,
                }),
            )]);
            let mut out = String::new();
            let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
            let re_parsed = crate::core::builder::parse_str(&out, None).expect("re-parse failed");
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
                chomping: None,
                anchor: None,
                comment_inline: None,
                comment_before: None,
                blank_lines_before: 0,
            }),
        )]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
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
                chomping: None,
                anchor: None,
                comment_inline: None,
                comment_before: None,
                blank_lines_before: 0,
            }),
        )]);
        let mut out = String::new();
        let _ = Emitter::new(&mut out, 2).emit_node(&YamlNode::Mapping(m), 0, false);
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

    #[test]
    fn format_tag_verbatim_allows_flow_indicators() {
        // Inside `!<…>` the angle brackets delimit the URI so `,` `[` `]`
        // `{` `}` do not terminate the tag and must be emitted literally.
        assert_eq!(
            format_tag("tag:example.com,2020:thing").as_ref(),
            "!<tag:example.com,2020:thing>"
        );
        // `[`, `]`, `,` are URI chars (allowed in verbatim); `{`, `}` are not.
        assert_eq!(format_tag("scheme:a[b]c,d").as_ref(), "!<scheme:a[b]c,d>");
    }

    #[test]
    fn format_tag_shorthand_still_percent_encodes_flow() {
        // Shorthand (`!foo`, `!!foo`) — flow indicators would terminate the
        // tag so must be percent-encoded.
        assert_eq!(format_tag("!weird,tag").as_ref(), "!weird%2Ctag");
        assert_eq!(format_tag("!!has[bracket").as_ref(), "!!has%5Bbracket");
    }

    // ── needs_quoting ────────────────────────────────────────────────────────

    #[test]
    fn needs_quoting_empty_str() {
        assert!(needs_quoting("", false));
    }

    #[test]
    fn needs_quoting_null_keyword() {
        assert!(needs_quoting("null", false));
        assert!(needs_quoting("~", false));
        assert!(needs_quoting("Null", false));
    }

    #[test]
    fn needs_quoting_bool_keywords() {
        for s in &["true", "false", "yes", "no", "on", "off", "True", "False"] {
            assert!(needs_quoting(s, false), "should need quoting: {s}");
        }
    }

    #[test]
    fn needs_quoting_integer_str() {
        assert!(needs_quoting("42", false));
        assert!(needs_quoting("-1", false));
        assert!(needs_quoting("0xFF", false));
        assert!(needs_quoting("0o77", false));
    }

    #[test]
    fn needs_quoting_float_str() {
        assert!(needs_quoting("3.14", false));
        assert!(needs_quoting("1e5", false));
        assert!(needs_quoting(".inf", false));
        assert!(needs_quoting(".nan", false));
    }

    #[test]
    fn needs_quoting_regular_string_safe() {
        assert!(!needs_quoting("hello", false));
        assert!(!needs_quoting("world", false));
        assert!(!needs_quoting("some-value", false));
    }

    #[test]
    fn needs_quoting_inline_comment_trigger() {
        assert!(needs_quoting("value # comment", false));
    }

    #[test]
    fn needs_quoting_colon_space() {
        assert!(needs_quoting("key: val", false));
    }

    #[test]
    fn needs_quoting_document_markers() {
        // A root-level scalar of "---" or "..." must be quoted so it does not
        // collide with YAML directive-end / document-end markers on re-parse.
        assert!(needs_quoting("---", false));
        assert!(needs_quoting("...", false));
        assert!(needs_quoting_for_key("---", false));
        assert!(needs_quoting_for_key("...", false));
    }

    #[test]
    fn needs_quoting_whitespace_boundary() {
        assert!(needs_quoting(" leading", false));
        assert!(needs_quoting("trailing ", false));
        assert!(needs_quoting("\t", false));
        assert!(needs_quoting("\tleading-tab", false));
        assert!(needs_quoting("trailing-tab\t", false));
        assert!(!needs_quoting("a b", false));
        assert!(!needs_quoting("a\tb", false));
    }

    #[test]
    fn needs_quoting_conditional_indicators() {
        // `-` and `?` only force quoting when alone or followed by whitespace
        // (per YAML 1.2 ns-plain-first); followed by a non-whitespace byte
        // they may start a plain scalar.
        assert!(needs_quoting("-", false));
        assert!(needs_quoting("- rest", false));
        assert!(!needs_quoting("-foo", false));
        assert!(!needs_quoting("<hostname:abc>", false));
        assert!(needs_quoting("?", false));
        assert!(needs_quoting("? rest", false));
        assert!(!needs_quoting("?x", false));
    }

    #[test]
    fn needs_quoting_non_indicator_leading_safe() {
        // `<` and `=` are not YAML 1.2 indicators and must not force quoting.
        assert!(!needs_quoting("<tag>", false));
        assert!(!needs_quoting("=expr", false));
    }

    #[test]
    fn needs_quoting_quote_leading() {
        // `'` and `"` start quoted-scalar openers — plain emission would
        // be misread as a broken quoted scalar.
        assert!(needs_quoting("'apos", false));
        assert!(needs_quoting("\"quote", false));
    }

    #[test]
    fn needs_quoting_flow_context_indicators() {
        // Flow-context only: `,` `[` `]` `{` `}` end a plain scalar wherever
        // they appear, so a value like "a, b" inside `[ … ]` must be quoted
        // even though it is fine in block context.
        for s in &["a,b", "a, b", "a]b", "a}b", "x,y,z"] {
            assert!(
                needs_quoting(s, true),
                "flow-context should need quoting: {s}"
            );
            assert!(!needs_quoting(s, false), "block-context should not: {s}");
        }
    }

    // ── needs_quoting_for_key ────────────────────────────────────────────────

    #[test]
    fn needs_quoting_for_key_empty() {
        assert!(needs_quoting_for_key("", false));
    }

    #[test]
    fn needs_quoting_for_key_hash_leading() {
        assert!(needs_quoting_for_key("#comment", false));
    }

    #[test]
    fn needs_quoting_for_key_star_leading() {
        assert!(needs_quoting_for_key("*alias", false));
    }

    #[test]
    fn needs_quoting_for_key_question_leading() {
        // `?` followed by whitespace or alone starts a complex-key indicator;
        // `?` followed by a non-whitespace byte is a safe plain-scalar start.
        assert!(needs_quoting_for_key("?", false));
        assert!(needs_quoting_for_key("? rest", false));
        assert!(!needs_quoting_for_key("?complex", false));
    }

    #[test]
    fn needs_quoting_for_key_dash_leading() {
        // Same rule as `?`: only `- ` is the sequence indicator.
        assert!(needs_quoting_for_key("-", false));
        assert!(needs_quoting_for_key("- rest", false));
        assert!(!needs_quoting_for_key("-foo", false));
    }

    #[test]
    fn needs_quoting_for_key_non_indicator_leading_safe() {
        // `<` and `=` are not YAML 1.2 indicators and must not force quoting.
        assert!(!needs_quoting_for_key("<hostname>", false));
        assert!(!needs_quoting_for_key("=value", false));
    }

    #[test]
    fn needs_quoting_for_key_quote_leading() {
        // `'` and `"` start quoted-scalar openers — plain emission would
        // be misread as a broken quoted scalar.
        assert!(needs_quoting_for_key("'apos", false));
        assert!(needs_quoting_for_key("\"quote", false));
    }

    #[test]
    fn needs_quoting_for_key_bang_leading() {
        assert!(needs_quoting_for_key("!tag", false));
    }

    #[test]
    fn needs_quoting_for_key_ends_with_colon() {
        assert!(needs_quoting_for_key("key:", false));
    }

    #[test]
    fn needs_quoting_for_key_colon_space_inside() {
        assert!(needs_quoting_for_key("key: value", false));
    }

    #[test]
    fn needs_quoting_for_key_space_hash_inside() {
        assert!(needs_quoting_for_key("key #comment", false));
    }

    #[test]
    fn needs_quoting_for_key_newline() {
        assert!(needs_quoting_for_key("a\nb", false));
    }

    #[test]
    fn needs_quoting_for_key_whitespace_boundary() {
        assert!(needs_quoting_for_key(" leading", false));
        assert!(needs_quoting_for_key("trailing ", false));
        assert!(needs_quoting_for_key("\t", false));
        assert!(!needs_quoting_for_key("a b", false));
        assert!(!needs_quoting_for_key("a\tb", false));
    }

    #[test]
    fn needs_quoting_for_key_numeric_and_keyword_quoted() {
        // yarutsk stores all keys as strings, but other YAML parsers
        // resolve keyword/numeric-looking keys to bool/int/float/null.
        // Quote them for interop safety.
        assert!(needs_quoting_for_key("42", false));
        assert!(needs_quoting_for_key("3.14", false));
        assert!(needs_quoting_for_key("0xFF", false));
        assert!(needs_quoting_for_key("0o77", false));
        assert!(needs_quoting_for_key("-1", false));
        assert!(needs_quoting_for_key("1e5", false));
        assert!(needs_quoting_for_key("true", false));
        assert!(needs_quoting_for_key("False", false));
        assert!(needs_quoting_for_key("yes", false));
        assert!(needs_quoting_for_key("null", false));
        assert!(needs_quoting_for_key("~", false));
        assert!(needs_quoting_for_key(".inf", false));
        assert!(needs_quoting_for_key(".nan", false));
    }

    #[test]
    fn needs_quoting_for_key_plain_safe() {
        assert!(!needs_quoting_for_key("simple", false));
        assert!(!needs_quoting_for_key("snake_case", false));
        assert!(!needs_quoting_for_key("kebab-case", false));
    }

    // ── emit_key ─────────────────────────────────────────────────────────────

    fn do_emit_key(key: &str, style: ScalarStyle) -> String {
        emit_key(key, style, false)
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
    fn emit_key_numeric_keyword_quoted_for_interop() {
        // Plain numeric/keyword-looking keys would re-parse as int/bool/null
        // under other YAML libraries' key-type resolution. Force quotes so
        // the output stays string-keyed across parsers.
        assert_eq!(do_emit_key("42", ScalarStyle::Plain), "'42'");
        assert_eq!(do_emit_key("true", ScalarStyle::Plain), "'true'");
        assert_eq!(do_emit_key("null", ScalarStyle::Plain), "'null'");
    }

    #[test]
    fn emit_key_line_break_forces_double_quote() {
        // Single-quoted keys with a line break would span lines and break the
        // surrounding map; the line break must become a `\n` / `\r` escape.
        assert_eq!(
            emit_key("a\nb", ScalarStyle::Plain, false),
            "\"a\\nb\"".to_string()
        );
        assert_eq!(
            emit_key("a\rb", ScalarStyle::SingleQuoted, false),
            "\"a\\rb\"".to_string()
        );
    }

    #[test]
    fn emit_key_flow_context_flow_indicator_quoted() {
        // Inside `{ … }`, flow indicators terminate a plain key.
        assert!(emit_key("a, b", ScalarStyle::Plain, true).starts_with('\''));
        assert!(emit_key("a}b", ScalarStyle::Plain, true).starts_with('\''));
        // In block context the same key is safe unquoted.
        assert_eq!(emit_key("a, b", ScalarStyle::Plain, false), "a, b");
    }

    #[test]
    fn needs_quoting_for_key_flow_context_indicators() {
        for s in &["a,b", "a, b", "a]b", "a}b", "x,y,z"] {
            assert!(
                needs_quoting_for_key(s, true),
                "flow-context should need quoting: {s}"
            );
            assert!(
                !needs_quoting_for_key(s, false),
                "block-context should not: {s}"
            );
        }
    }

    // ── emit_string_with_style ───────────────────────────────────────────────

    #[test]
    fn emit_string_single_quoted_with_newline_uses_double_quotes() {
        // SingleQuoted + embedded newline falls back to double-quoted with \n escapes
        let out = emit_string_with_style("a\nb", ScalarStyle::SingleQuoted, false);
        assert!(
            out.starts_with('"'),
            "expected double-quoted fallback for newline in single-quoted: {out}"
        );
        assert!(out.contains("\\n"), "expected \\n escape: {out}");
    }

    #[test]
    fn emit_string_single_quoted_no_newline() {
        let out = emit_string_with_style("hello", ScalarStyle::SingleQuoted, false);
        assert_eq!(out, "'hello'");
    }

    #[test]
    fn emit_string_double_quoted() {
        let out = emit_string_with_style("hello", ScalarStyle::DoubleQuoted, false);
        assert_eq!(out, "\"hello\"");
    }

    #[test]
    fn emit_string_plain_safe_unquoted() {
        let out = emit_string_with_style("hello", ScalarStyle::Plain, false);
        assert_eq!(out, "hello");
    }

    #[test]
    fn emit_string_plain_needs_quoting() {
        let out = emit_string_with_style("true", ScalarStyle::Plain, false);
        assert!(
            out.starts_with('\'') || out.starts_with('"'),
            "expected quotes: {out}"
        );
    }
}
