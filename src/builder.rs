// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::collections::HashMap;

use crate::parser::{Event, Parser, Tag};
use crate::scanner::{Marker, TScalarStyle};
use crate::types::*;

pub struct Builder {
    stack: Vec<Frame>,
    pub docs: Vec<YamlNode>,
    /// Whether each doc in `docs` had an explicit `---` marker.
    pub doc_explicit: Vec<bool>,
    /// Whether each doc in `docs` had an explicit `...` end marker.
    pub doc_explicit_end: Vec<bool>,
    /// Whether the next document to be pushed had an explicit `---` marker.
    next_explicit: bool,
    /// Line of the last SCALAR content token (key or value), for inline comment detection.
    /// Only scalars update this; MappingEnd/SequenceEnd do not.
    last_content_line: Option<usize>,
    /// Comments not yet associated with any node (before-key candidates).
    pending_before: Vec<(usize, String)>,
    /// Anchor table: maps anchor name → completed node, for alias resolution.
    anchor_table: HashMap<String, YamlNode>,
}

enum Frame {
    Mapping(MappingFrame),
    Sequence(SequenceFrame),
}

struct MappingFrame {
    mapping: YamlMapping,
    current_key: Option<String>,
    current_comment_before: Option<String>,
    current_comment_inline: Option<String>,
    /// Blank lines before the current entry (computed when the key scalar is seen).
    current_blank_lines: u8,
    /// The quoting style of the current key scalar.
    current_key_style: ScalarStyle,
    /// Anchor name declared on the MappingStart event, if any.
    anchor_name: Option<String>,
}

struct SequenceFrame {
    seq: YamlSequence,
    /// Comment before the current complex item (saved before pushing nested frame).
    current_comment_before: Option<String>,
    /// Blank lines before the current complex item.
    current_blank_lines: u8,
    /// Anchor name declared on the SequenceStart event, if any.
    anchor_name: Option<String>,
}

/// Convert a parser `Tag` to the compact string we store (e.g. `"!!str"`, `"!custom"`).
fn tag_to_string(tag: Option<Tag>) -> Option<String> {
    tag.map(|t| format!("{}{}", t.handle, t.suffix))
}

/// Map a scanner scalar style to our stored style enum.
fn map_scalar_style(style: TScalarStyle) -> ScalarStyle {
    match style {
        TScalarStyle::Plain => ScalarStyle::Plain,
        TScalarStyle::SingleQuoted => ScalarStyle::SingleQuoted,
        TScalarStyle::DoubleQuoted => ScalarStyle::DoubleQuoted,
        TScalarStyle::Literal => ScalarStyle::Literal,
        TScalarStyle::Folded => ScalarStyle::Folded,
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            stack: Vec::new(),
            docs: Vec::new(),
            doc_explicit: Vec::new(),
            doc_explicit_end: Vec::new(),
            next_explicit: false,
            last_content_line: None,
            pending_before: Vec::new(),
            anchor_table: HashMap::new(),
        }
    }

    /// Process newly collected comments: inline if on same line as last scalar, else before-key.
    pub fn absorb_comments(&mut self, new_comments: Vec<(Marker, String)>) {
        for (mark, text) in new_comments {
            if self.last_content_line == Some(mark.line()) {
                self.attach_inline(text);
            } else {
                self.pending_before.push((mark.line(), text));
            }
        }
    }

    /// Retroactively attach an inline comment to the most-recently completed entry/item.
    fn attach_inline(&mut self, text: String) {
        match self.stack.last_mut() {
            Some(Frame::Mapping(mf)) => {
                if mf.current_key.is_some() {
                    // Key scalar was last; value not yet seen → store inline on frame
                    mf.current_comment_inline = Some(text);
                } else {
                    // Value was just finalized → update last inserted entry
                    if let Some((_, entry)) = mf.mapping.entries.last_mut() {
                        entry.comment_inline = Some(text);
                    }
                }
            }
            Some(Frame::Sequence(sf)) => {
                if let Some(item) = sf.seq.items.last_mut() {
                    item.comment_inline = Some(text);
                }
            }
            None => {
                // Stack is empty: the last doc was just pushed; retroactively update it
                retroactive_inline(self.docs.last_mut(), text);
            }
        }
    }

    /// Count blank lines between the last scalar content and `node_line`.
    /// Must be called BEFORE `take_before` drains `pending_before`.
    fn count_blank_lines(&self, node_line: usize) -> u8 {
        let last_line = match self.last_content_line {
            None => return 0,
            Some(l) => l,
        };
        if node_line <= last_line + 1 {
            return 0;
        }
        let comment_count = self
            .pending_before
            .iter()
            .filter(|(l, _)| *l < node_line)
            .count();
        let total_between = node_line - last_line - 1;
        total_between.saturating_sub(comment_count).min(255) as u8
    }

    /// Take all pending before-comments with line < node_line, join with newline.
    fn take_before(&mut self, node_line: usize) -> Option<String> {
        let mut result: Option<String> = None;
        for (_, text) in self
            .pending_before
            .drain(..)
            .filter(|(l, _)| *l < node_line)
        {
            match result.as_mut() {
                None => result = Some(text),
                Some(r) => {
                    r.push('\n');
                    r.push_str(&text);
                }
            }
        }
        result
    }

    /// Push a completed node into the current parent context.
    /// Does NOT update last_content_line (only scalars do that).
    fn push_node(&mut self, node: YamlNode) {
        match self.stack.last_mut() {
            None => {
                self.doc_explicit.push(self.next_explicit);
                self.docs.push(node);
            }
            Some(Frame::Mapping(mf)) => {
                if let Some(key) = mf.current_key.take() {
                    let comment_before = mf.current_comment_before.take();
                    let comment_inline = mf.current_comment_inline.take();
                    let blank_lines_before = mf.current_blank_lines;
                    let key_style = mf.current_key_style;
                    mf.current_blank_lines = 0;
                    mf.mapping.entries.insert(
                        key,
                        YamlEntry {
                            value: node,
                            comment_before,
                            comment_inline,
                            blank_lines_before,
                            key_style,
                        },
                    );
                }
            }
            Some(Frame::Sequence(sf)) => {
                let comment_before = sf.current_comment_before.take();
                let blank_lines_before = sf.current_blank_lines;
                sf.current_blank_lines = 0;
                sf.seq.items.push(YamlItem {
                    value: node,
                    comment_before,
                    comment_inline: None,
                    blank_lines_before,
                });
            }
        }
    }

    /// Register a node in the anchor table by name (if anchor_name is Some).
    fn register_anchor(&mut self, anchor_name: Option<&str>, node: &YamlNode) {
        if let Some(name) = anchor_name {
            self.anchor_table.insert(name.to_owned(), node.clone());
        }
    }

    /// Process a single parser event.
    pub fn process_event(&mut self, ev: Event, mark: Marker) {
        match ev {
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}

            Event::DocumentStart(explicit) => {
                self.next_explicit = explicit;
                // Record the document-start line so blank lines between `---` and
                // the first key are counted correctly by count_blank_lines.
                self.last_content_line = Some(mark.line());
            }

            Event::DocumentEnd(explicit_end) => {
                self.doc_explicit_end.push(explicit_end);
            }

            Event::MappingStart(anchor_name, tag, is_flow) => {
                let is_seq_parent = matches!(self.stack.last(), Some(Frame::Sequence(_)));
                if is_seq_parent {
                    // Only drain before-comments when our parent is a sequence item;
                    // for mapping/root parents, leave comments in pending_before so the
                    // first key scalar can pick them up.
                    let blank_lines = self.count_blank_lines(mark.line());
                    let before = self.take_before(mark.line());
                    if let Some(Frame::Sequence(sf)) = self.stack.last_mut() {
                        sf.current_comment_before = before;
                        sf.current_blank_lines = blank_lines;
                    }
                }
                let mut mapping = YamlMapping::new();
                mapping.style = if is_flow {
                    ContainerStyle::Flow
                } else {
                    ContainerStyle::Block
                };
                mapping.tag = tag_to_string(tag);
                mapping.anchor = anchor_name.clone();
                self.stack.push(Frame::Mapping(MappingFrame {
                    mapping,
                    current_key: None,
                    current_comment_before: None,
                    current_comment_inline: None,
                    current_blank_lines: 0,
                    current_key_style: ScalarStyle::Plain,
                    anchor_name,
                }));
            }

            Event::MappingEnd => {
                if let Some(Frame::Mapping(mut mf)) = self.stack.pop() {
                    // Count blank lines trailing after the last entry.
                    let trailing = if let Some(last_line) = self.last_content_line {
                        let end_line = mark.line();
                        if end_line > last_line + 1 {
                            let nc = self
                                .pending_before
                                .iter()
                                .filter(|(l, _)| *l < end_line)
                                .count();
                            (end_line - last_line - 1).saturating_sub(nc).min(255) as u8
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    mf.mapping.trailing_blank_lines = trailing;
                    // Advance last_content_line so outer containers don't double-count.
                    self.last_content_line = Some(mark.line());
                    let anchor_name = mf.anchor_name.as_deref();
                    let node = YamlNode::Mapping(mf.mapping);
                    self.register_anchor(anchor_name, &node);
                    self.push_node(node);
                }
            }

            Event::SequenceStart(anchor_name, tag, is_flow) => {
                let is_seq_parent = matches!(self.stack.last(), Some(Frame::Sequence(_)));
                if is_seq_parent {
                    let blank_lines = self.count_blank_lines(mark.line());
                    let before = self.take_before(mark.line());
                    if let Some(Frame::Sequence(sf)) = self.stack.last_mut() {
                        sf.current_comment_before = before;
                        sf.current_blank_lines = blank_lines;
                    }
                }
                let mut seq = YamlSequence::new();
                seq.style = if is_flow {
                    ContainerStyle::Flow
                } else {
                    ContainerStyle::Block
                };
                seq.tag = tag_to_string(tag);
                seq.anchor = anchor_name.clone();
                self.stack.push(Frame::Sequence(SequenceFrame {
                    seq,
                    current_comment_before: None,
                    current_blank_lines: 0,
                    anchor_name,
                }));
            }

            Event::SequenceEnd => {
                if let Some(Frame::Sequence(mut sf)) = self.stack.pop() {
                    // Count blank lines trailing after the last item.
                    let trailing = if let Some(last_line) = self.last_content_line {
                        let end_line = mark.line();
                        if end_line > last_line + 1 {
                            let nc = self
                                .pending_before
                                .iter()
                                .filter(|(l, _)| *l < end_line)
                                .count();
                            (end_line - last_line - 1).saturating_sub(nc).min(255) as u8
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    sf.seq.trailing_blank_lines = trailing;
                    // Advance last_content_line so outer containers don't double-count.
                    self.last_content_line = Some(mark.line());
                    let anchor_name = sf.anchor_name.as_deref();
                    let node = YamlNode::Sequence(sf.seq);
                    self.register_anchor(anchor_name, &node);
                    self.push_node(node);
                }
            }

            Event::Scalar(value, style, anchor_name, tag) => {
                let scalar_style = map_scalar_style(style);
                let scalar_tag = tag_to_string(tag);
                // Compute the type-inferred value, then apply tag overrides.
                let typed = match style {
                    TScalarStyle::Plain => ScalarValue::from_str(&value),
                    // Quoted scalars are always strings — even an empty "" or '' is ""
                    // not null (quoting is explicit intent to represent a string value).
                    _ => ScalarValue::Str(value.clone()),
                };
                // !!str and ! (non-specific) tags force the scalar to be a string,
                // overriding any type inference that would have produced bool/int/float/null.
                let typed = match scalar_tag.as_deref() {
                    Some("!!str") | Some("tag:yaml.org,2002:str") | Some("!") => {
                        ScalarValue::Str(value.clone())
                    }
                    _ => typed,
                };

                // For block scalars, the content spans multiple source lines.
                // Advance last_content_line past all those lines so outer containers
                // don't double-count the trailing blank lines embedded in the value.
                let effective_scalar_end_line =
                    if matches!(scalar_style, ScalarStyle::Literal | ScalarStyle::Folded) {
                        mark.line() + value.bytes().filter(|&b| b == b'\n').count()
                    } else {
                        mark.line()
                    };

                // Preserve the original source text when the plain-scalar representation
                // differs from what the emitter would produce canonically.  This covers:
                //   - float exponent form (`1.5e10` vs `15000000000.0`)
                //   - non-canonical null/bool forms (`~`, `Null`, `yes`, `True`, …)
                //   - hex/octal/underscore-separated integers (`0xFF`, `0o77`, `1_000_000`)
                //   - tagged plain scalars (tag disambiguates type; keep unquoted source)
                let scalar_original: Option<String> = if style == TScalarStyle::Plain {
                    let would_differ = match &typed {
                        ScalarValue::Float(_) => value.contains('e') || value.contains('E'),
                        ScalarValue::Null => value != "null",
                        ScalarValue::Bool(true) => value != "true",
                        ScalarValue::Bool(false) => value != "false",
                        // Hex (0x/0X), octal (0o/0O), or underscore-separated ints
                        ScalarValue::Int(_) => {
                            value.contains(|c: char| !c.is_ascii_digit() && c != '-')
                        }
                        ScalarValue::Str(_) => false,
                    };
                    if would_differ || scalar_tag.is_some() {
                        Some(value.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Collect a clone for anchor registration AFTER the borrow-checking match below.
                let mut anchor_node: Option<YamlNode> = None;

                match self.stack.last_mut() {
                    None => {
                        let node = YamlNode::Scalar(YamlScalar {
                            value: typed,
                            style: scalar_style,
                            tag: scalar_tag,
                            original: scalar_original,
                            anchor: anchor_name.clone(),
                        });
                        if anchor_name.is_some() {
                            anchor_node = Some(node.clone());
                        }
                        self.doc_explicit.push(self.next_explicit);
                        self.docs.push(node);
                        self.last_content_line = Some(effective_scalar_end_line);
                    }
                    Some(Frame::Mapping(mf)) => {
                        if mf.current_key.is_none() {
                            let node_line = mark.line();
                            let blank_lines = match self.last_content_line {
                                Some(last) if node_line > last + 1 => {
                                    let nc = self
                                        .pending_before
                                        .iter()
                                        .filter(|(l, _)| *l < node_line)
                                        .count();
                                    (node_line - last - 1).saturating_sub(nc).min(255) as u8
                                }
                                _ => 0,
                            };
                            let comment_before = {
                                let mut result: Option<String> = None;
                                for (_, text) in self
                                    .pending_before
                                    .drain(..)
                                    .filter(|(l, _)| *l < node_line)
                                {
                                    match result.as_mut() {
                                        None => result = Some(text),
                                        Some(r) => {
                                            r.push('\n');
                                            r.push_str(&text);
                                        }
                                    }
                                }
                                result
                            };
                            mf.current_key = Some(value);
                            mf.current_comment_before = comment_before;
                            mf.current_comment_inline = None;
                            mf.current_blank_lines = blank_lines;
                            mf.current_key_style = scalar_style;
                            self.last_content_line = Some(effective_scalar_end_line);
                            // Register anchor for key scalars so they can later be aliased as values.
                            if anchor_name.is_some() {
                                anchor_node = Some(YamlNode::Scalar(YamlScalar {
                                    value: typed,
                                    style: scalar_style,
                                    tag: scalar_tag,
                                    original: scalar_original.clone(),
                                    anchor: anchor_name.clone(),
                                }));
                            }
                        } else {
                            let node = YamlNode::Scalar(YamlScalar {
                                value: typed,
                                style: scalar_style,
                                tag: scalar_tag,
                                original: scalar_original,
                                anchor: anchor_name.clone(),
                            });
                            if anchor_name.is_some() {
                                anchor_node = Some(node.clone());
                            }
                            if let Some(key) = mf.current_key.take() {
                                let comment_before = mf.current_comment_before.take();
                                let comment_inline = mf.current_comment_inline.take();
                                let blank_lines_before = mf.current_blank_lines;
                                let key_style = mf.current_key_style;
                                mf.current_blank_lines = 0;
                                mf.mapping.entries.insert(
                                    key,
                                    YamlEntry {
                                        value: node,
                                        comment_before,
                                        comment_inline,
                                        blank_lines_before,
                                        key_style,
                                    },
                                );
                            }
                            self.last_content_line = Some(effective_scalar_end_line);
                        }
                    }
                    Some(Frame::Sequence(sf)) => {
                        let node_line = mark.line();
                        let blank_lines = match self.last_content_line {
                            Some(last) if node_line > last + 1 => {
                                let nc = self
                                    .pending_before
                                    .iter()
                                    .filter(|(l, _)| *l < node_line)
                                    .count();
                                (node_line - last - 1).saturating_sub(nc).min(255) as u8
                            }
                            _ => 0,
                        };
                        let comment_before = {
                            let mut result: Option<String> = None;
                            for (_, text) in self
                                .pending_before
                                .drain(..)
                                .filter(|(l, _)| *l < node_line)
                            {
                                match result.as_mut() {
                                    None => result = Some(text),
                                    Some(r) => {
                                        r.push('\n');
                                        r.push_str(&text);
                                    }
                                }
                            }
                            result
                        };
                        let node = YamlNode::Scalar(YamlScalar {
                            value: typed,
                            style: scalar_style,
                            tag: scalar_tag,
                            original: scalar_original,
                            anchor: anchor_name.clone(),
                        });
                        if anchor_name.is_some() {
                            anchor_node = Some(node.clone());
                        }
                        sf.seq.items.push(YamlItem {
                            value: node,
                            comment_before,
                            comment_inline: None,
                            blank_lines_before: blank_lines,
                        });
                        self.last_content_line = Some(effective_scalar_end_line);
                    }
                }

                // Register anchor after releasing the mutable borrow on self.stack.
                if let Some(node) = anchor_node {
                    self.register_anchor(anchor_name.as_deref(), &node);
                }
            }

            Event::Alias(name) => {
                // Resolve the alias and store YamlNode::Alias { name, resolved }.
                // The resolved copy is used by the Python layer; the name is used by the emitter.
                let resolved = self
                    .anchor_table
                    .get(&name)
                    .cloned()
                    .unwrap_or(YamlNode::Null);
                // If the alias is in mapping key position, use its resolved scalar value as the key string.
                if let Some(Frame::Mapping(mf)) = self.stack.last_mut()
                    && mf.current_key.is_none()
                {
                    mf.current_key = Some(match &resolved {
                        YamlNode::Scalar(s) => match &s.value {
                            ScalarValue::Null => String::new(),
                            ScalarValue::Bool(b) => b.to_string(),
                            ScalarValue::Int(n) => n.to_string(),
                            ScalarValue::Float(f) => f.to_string(),
                            ScalarValue::Str(s) => s.clone(),
                        },
                        _ => String::new(),
                    });
                    return;
                }
                self.push_node(YamlNode::Alias {
                    name,
                    resolved: Box::new(resolved),
                });
            }
        }
    }
}

/// Retroactively set comment_inline on the last leaf entry of a node.
fn retroactive_inline(node: Option<&mut YamlNode>, text: String) {
    if let Some(node) = node {
        match node {
            YamlNode::Mapping(m) => {
                if let Some((_, entry)) = m.entries.last_mut()
                    && entry.comment_inline.is_none()
                {
                    entry.comment_inline = Some(text);
                }
            }
            YamlNode::Sequence(s) => {
                if let Some(item) = s.items.last_mut()
                    && item.comment_inline.is_none()
                {
                    item.comment_inline = Some(text);
                }
            }
            _ => {}
        }
    }
}

/// Parse YAML input into a list of top-level documents.
/// Returns `(docs, explicit_starts, explicit_ends)` where the flags are `true` when
/// the document had an explicit `---` / `...` marker in the source.
#[allow(clippy::type_complexity)]
pub fn parse_str(input: &str) -> Result<(Vec<YamlNode>, Vec<bool>, Vec<bool>), String> {
    let mut parser = Parser::new_from_str(input);
    let mut builder = Builder::new();

    loop {
        // Fetch the next event first; comments are accumulated *during* scanning.
        let (ev, mark) = parser
            .next_token()
            .map_err(|e| format!("YAML parse error: {e}"))?;

        // Drain comments that were collected while scanning for this event,
        // then absorb them before processing the event so that before-key
        // comments (accumulated while scanning the key token) are in
        // pending_before in time for the key scalar handler to pick them up.
        let comments = parser.drain_comments();
        builder.absorb_comments(comments);

        let done = matches!(ev, Event::StreamEnd);
        builder.process_event(ev, mark);

        if done {
            break;
        }
    }

    Ok((builder.docs, builder.doc_explicit, builder.doc_explicit_end))
}
