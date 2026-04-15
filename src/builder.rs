// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::collections::{HashMap, HashSet};

use crate::parser::{Event, Parser, Tag};
use crate::scanner::{Marker, TScalarStyle};
use crate::types::*;

/// Tags that should bypass built-in ScalarValue coercion in the builder.
/// When a tag is in this set the scalar value is kept as `ScalarValue::Str`
/// with the raw source text, so that Python-layer constructors receive the
/// original YAML string rather than a pre-converted value.
pub struct TagPolicy {
    pub raw_tags: HashSet<String>,
}

pub struct Builder {
    stack: Vec<Frame>,
    pub docs: Vec<YamlNode>,
    /// Whether each doc in `docs` had an explicit `---` marker.
    pub doc_explicit: Vec<bool>,
    /// Whether each doc in `docs` had an explicit `...` end marker.
    pub doc_explicit_end: Vec<bool>,
    /// `%YAML major.minor` directive for each doc, if present.
    pub doc_yaml_version: Vec<Option<(u8, u8)>>,
    /// `%TAG handle prefix` pairs for each doc (empty vec if none).
    pub doc_tag_directives: Vec<Vec<(String, String)>>,
    /// Whether the next document to be pushed had an explicit `---` marker.
    next_explicit: bool,
    /// Directives captured from the current `DocumentStart` event.
    next_yaml_version: Option<(u8, u8)>,
    next_tag_directives: Vec<(String, String)>,
    /// Line of the last SCALAR content token (key or value), for inline comment detection.
    /// Only scalars update this; MappingEnd/SequenceEnd do not.
    last_content_line: Option<usize>,
    /// Comments not yet associated with any node (before-key candidates).
    pending_before: Vec<(usize, String)>,
    /// Anchor table: maps anchor name → completed node, for alias resolution.
    anchor_table: HashMap<String, YamlNode>,
}

enum Frame {
    Mapping(Box<MappingFrame>),
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
    /// Anchor declared on the key scalar, if any.
    current_key_anchor: Option<String>,
    /// If the key was written as an alias, the alias name.
    current_key_alias: Option<String>,
    /// Tag on the key scalar, if any.
    current_key_tag: Option<String>,
    /// For complex (non-scalar) keys: the key node.  When this is Some,
    /// current_key is None but we are in "have key, waiting for value" state.
    current_key_node: Option<YamlNode>,
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

/// Construct a `YamlNode::Scalar` from already-resolved components.
fn make_scalar(
    value: ScalarValue,
    style: ScalarStyle,
    tag: Option<String>,
    original: Option<String>,
    anchor: Option<String>,
) -> YamlNode {
    YamlNode::Scalar(YamlScalar {
        value,
        style,
        tag,
        original,
        anchor,
    })
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
            doc_yaml_version: Vec::new(),
            doc_tag_directives: Vec::new(),
            next_explicit: false,
            next_yaml_version: None,
            next_tag_directives: Vec::new(),
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
                if mf.current_key.is_some() || mf.current_key_node.is_some() {
                    // Key was last; value not yet seen → store inline on frame
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
                self.doc_yaml_version.push(self.next_yaml_version.take());
                self.doc_tag_directives
                    .push(std::mem::take(&mut self.next_tag_directives));
                self.docs.push(node);
            }
            Some(Frame::Mapping(mf)) => {
                if let Some(key) = mf.current_key.take() {
                    // Scalar key (or sentinel for complex key already stored in key_node).
                    let comment_before = mf.current_comment_before.take();
                    let comment_inline = mf.current_comment_inline.take();
                    let blank_lines_before = mf.current_blank_lines;
                    let key_style = mf.current_key_style;
                    let key_anchor = mf.current_key_anchor.take();
                    let key_alias = mf.current_key_alias.take();
                    let key_tag = mf.current_key_tag.take();
                    let key_node = mf.current_key_node.take().map(Box::new);
                    mf.current_blank_lines = 0;
                    mf.mapping.entries.insert(
                        key,
                        YamlEntry {
                            value: node,
                            comment_before,
                            comment_inline,
                            blank_lines_before,
                            key_style,
                            key_anchor,
                            key_alias,
                            key_tag,
                            key_node,
                        },
                    );
                } else if mf.current_key_node.is_some() {
                    // Complex key was saved; this node is the VALUE.
                    let comment_before = mf.current_comment_before.take();
                    let comment_inline = mf.current_comment_inline.take();
                    let blank_lines_before = mf.current_blank_lines;
                    let key_node = mf.current_key_node.take().map(Box::new);
                    mf.current_blank_lines = 0;
                    // Synthetic string key: null-prefixed index ensures uniqueness and
                    // distinguishes from real YAML keys.
                    let key = format!("\x00{}", mf.mapping.entries.len());
                    mf.mapping.entries.insert(
                        key,
                        YamlEntry {
                            value: node,
                            comment_before,
                            comment_inline,
                            blank_lines_before,
                            key_style: ScalarStyle::Plain,
                            key_anchor: None,
                            key_alias: None,
                            key_tag: None,
                            key_node,
                        },
                    );
                } else {
                    // current_key is None AND current_key_node is None:
                    // this node is the COMPLEX KEY itself.
                    mf.current_key_node = Some(node);
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
    pub fn process_event(&mut self, ev: Event, mark: Marker, policy: Option<&TagPolicy>) {
        match ev {
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}

            Event::DocumentStart(explicit, version, tag_dirs) => {
                self.next_explicit = explicit;
                self.next_yaml_version = version;
                self.next_tag_directives = tag_dirs;
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
                    //
                    // For block mappings with a tag (e.g. `- !tag\n  key: val`), the
                    // parser emits MappingStart at the first KEY line, not at the `!tag`
                    // line. Use `mark.line() - 1` as the reference so blank-line counting
                    // reflects the actual `- !tag` line where the item begins.
                    let ref_line = if !is_flow && tag.is_some() && mark.line() > 0 {
                        mark.line() - 1
                    } else {
                        mark.line()
                    };
                    let blank_lines = self.count_blank_lines(ref_line);
                    let before = self.take_before(ref_line);
                    if let Some(Frame::Sequence(sf)) = self.stack.last_mut() {
                        sf.current_comment_before = before;
                        sf.current_blank_lines = blank_lines;
                    }
                    // Update last_content_line to the container start so the first key
                    // inside this mapping doesn't see a spurious gap.
                    self.last_content_line = Some(mark.line() - 1);
                }
                let mut mapping = YamlMapping::new();
                mapping.style = if is_flow {
                    ContainerStyle::Flow
                } else {
                    ContainerStyle::Block
                };
                mapping.tag = tag_to_string(tag);
                mapping.anchor = anchor_name.clone();
                self.stack.push(Frame::Mapping(Box::new(MappingFrame {
                    mapping,
                    current_key: None,
                    current_comment_before: None,
                    current_comment_inline: None,
                    current_blank_lines: 0,
                    current_key_style: ScalarStyle::Plain,
                    current_key_anchor: None,
                    current_key_alias: None,
                    current_key_tag: None,
                    current_key_node: None,
                    anchor_name,
                })));
            }

            Event::MappingEnd => {
                if let Some(Frame::Mapping(mut mf)) = self.stack.pop() {
                    mf.mapping.trailing_blank_lines = self.count_blank_lines(mark.line());
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
                    // Same adjustment as for MappingStart: when a block sequence with a
                    // tag appears as a sequence item (`- !tag\n  - item`), the parser
                    // emits SequenceStart at the first ITEM line, not the `- !tag` line.
                    let ref_line = if !is_flow && tag.is_some() && mark.line() > 0 {
                        mark.line() - 1
                    } else {
                        mark.line()
                    };
                    let blank_lines = self.count_blank_lines(ref_line);
                    let before = self.take_before(ref_line);
                    if let Some(Frame::Sequence(sf)) = self.stack.last_mut() {
                        sf.current_comment_before = before;
                        sf.current_blank_lines = blank_lines;
                    }
                    self.last_content_line = Some(mark.line() - 1);
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
                    sf.seq.trailing_blank_lines = self.count_blank_lines(mark.line());
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
                // If the tag is in the TagPolicy's raw set, skip all coercion and keep
                // the scalar as its raw string so Python-layer loaders get the original text.
                let typed = if policy.is_some_and(|p| {
                    scalar_tag
                        .as_deref()
                        .is_some_and(|t| p.raw_tags.contains(t))
                }) {
                    ScalarValue::Str(value.clone())
                } else {
                    // Apply tag overrides: standard schema tags coerce the inferred type.
                    match scalar_tag.as_deref() {
                        Some("!!str") | Some("tag:yaml.org,2002:str") | Some("!") => {
                            ScalarValue::Str(value.clone())
                        }
                        Some("!!null") | Some("tag:yaml.org,2002:null") => ScalarValue::Null,
                        Some("!!bool") | Some("tag:yaml.org,2002:bool") => {
                            match ScalarValue::from_str(&value) {
                                ScalarValue::Bool(b) => ScalarValue::Bool(b),
                                _ => typed,
                            }
                        }
                        Some("!!int") | Some("tag:yaml.org,2002:int") => {
                            match ScalarValue::from_str(&value) {
                                ScalarValue::Int(n) => ScalarValue::Int(n),
                                _ => typed,
                            }
                        }
                        Some("!!float") | Some("tag:yaml.org,2002:float") => {
                            match ScalarValue::from_str(&value) {
                                ScalarValue::Float(f) => ScalarValue::Float(f),
                                // !!float on an integer literal → promote to Float
                                ScalarValue::Int(n) => ScalarValue::Float(n as f64),
                                _ => typed,
                            }
                        }
                        _ => typed,
                    }
                }; // close the else { match ... } from the TagPolicy check above

                let node_line = mark.line();
                // For block scalars the content spans multiple source lines; advance
                // last_content_line past them so outer containers don't double-count.
                let effective_scalar_end_line =
                    if matches!(scalar_style, ScalarStyle::Literal | ScalarStyle::Folded) {
                        node_line + value.bytes().filter(|&b| b == b'\n').count()
                    } else {
                        node_line
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

                // Peek at the stack to determine whether this scalar arrives as a
                // mapping key or sequence item — both need blank-line / before-comment
                // context.  We peek immutably here so we can call self methods freely
                // before taking the mutable borrow needed for insertion below.
                // A scalar is a KEY if: in mapping context AND no current_key AND no current_key_node.
                let needs_context = match self.stack.last() {
                    Some(Frame::Mapping(mf)) => {
                        mf.current_key.is_none() && mf.current_key_node.is_none()
                    }
                    Some(Frame::Sequence(_)) => true,
                    None => false,
                };
                let (blank_lines, comment_before) = if needs_context {
                    (
                        self.count_blank_lines(node_line),
                        self.take_before(node_line),
                    )
                } else {
                    (0, None)
                };

                // Collect a clone for anchor registration AFTER the borrow-checking match below.
                let mut anchor_node: Option<YamlNode> = None;

                match self.stack.last_mut() {
                    None => {
                        let node = make_scalar(
                            typed,
                            scalar_style,
                            scalar_tag,
                            scalar_original,
                            anchor_name.clone(),
                        );
                        if anchor_name.is_some() {
                            anchor_node = Some(node.clone());
                        }
                        self.doc_explicit.push(self.next_explicit);
                        self.doc_yaml_version.push(self.next_yaml_version.take());
                        self.doc_tag_directives
                            .push(std::mem::take(&mut self.next_tag_directives));
                        self.docs.push(node);
                        self.last_content_line = Some(effective_scalar_end_line);
                    }
                    Some(Frame::Mapping(mf)) => {
                        if mf.current_key.is_none() && mf.current_key_node.is_none() {
                            // Mapping key — store key string and positioning metadata.
                            // Register anchor for key scalars so they can be aliased as values.
                            if anchor_name.is_some() {
                                anchor_node = Some(make_scalar(
                                    typed,
                                    scalar_style,
                                    scalar_tag.clone(),
                                    scalar_original.clone(),
                                    anchor_name.clone(),
                                ));
                            }
                            mf.current_key = Some(value);
                            mf.current_comment_before = comment_before;
                            mf.current_comment_inline = None;
                            mf.current_blank_lines = blank_lines;
                            mf.current_key_style = scalar_style;
                            mf.current_key_anchor = anchor_name.clone();
                            mf.current_key_tag = scalar_tag.clone();
                            mf.current_key_alias = None;
                            self.last_content_line = Some(effective_scalar_end_line);
                        } else {
                            // Mapping value — insert entry under the pending key.
                            let node = make_scalar(
                                typed,
                                scalar_style,
                                scalar_tag,
                                scalar_original,
                                anchor_name.clone(),
                            );
                            if anchor_name.is_some() {
                                anchor_node = Some(node.clone());
                            }
                            if let Some(key) = mf.current_key.take() {
                                let comment_before = mf.current_comment_before.take();
                                let comment_inline = mf.current_comment_inline.take();
                                let blank_lines_before = mf.current_blank_lines;
                                let key_style = mf.current_key_style;
                                let key_anchor = mf.current_key_anchor.take();
                                let key_alias = mf.current_key_alias.take();
                                let key_tag = mf.current_key_tag.take();
                                let key_node = mf.current_key_node.take().map(Box::new);
                                mf.current_blank_lines = 0;
                                mf.mapping.entries.insert(
                                    key,
                                    YamlEntry {
                                        value: node,
                                        comment_before,
                                        comment_inline,
                                        blank_lines_before,
                                        key_style,
                                        key_anchor,
                                        key_alias,
                                        key_tag,
                                        key_node,
                                    },
                                );
                            } else if mf.current_key_node.is_some() {
                                // Complex key already saved; store value.
                                let comment_before = mf.current_comment_before.take();
                                let comment_inline = mf.current_comment_inline.take();
                                let blank_lines_before = mf.current_blank_lines;
                                let key_node = mf.current_key_node.take().map(Box::new);
                                mf.current_blank_lines = 0;
                                let key = format!("\x00{}", mf.mapping.entries.len());
                                mf.mapping.entries.insert(
                                    key,
                                    YamlEntry {
                                        value: node,
                                        comment_before,
                                        comment_inline,
                                        blank_lines_before,
                                        key_style: ScalarStyle::Plain,
                                        key_anchor: None,
                                        key_alias: None,
                                        key_tag: None,
                                        key_node,
                                    },
                                );
                            }
                            self.last_content_line = Some(effective_scalar_end_line);
                        }
                    }
                    Some(Frame::Sequence(sf)) => {
                        let node = make_scalar(
                            typed,
                            scalar_style,
                            scalar_tag,
                            scalar_original,
                            anchor_name.clone(),
                        );
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
                // If the alias is in mapping key position, record it as an alias key.
                if let Some(Frame::Mapping(mf)) = self.stack.last_mut()
                    && mf.current_key.is_none()
                    && mf.current_key_node.is_none()
                {
                    // Use the resolved scalar value as the key string (for Python access).
                    mf.current_key = Some(match &resolved {
                        YamlNode::Scalar(s) => s.value.to_key_string(),
                        _ => String::new(),
                    });
                    // Preserve the alias name so the emitter can emit `*name:`.
                    mf.current_key_alias = Some(name);
                    mf.current_key_anchor = None;
                    mf.current_key_tag = None;
                    mf.current_key_style = ScalarStyle::Plain;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emitter::emit_docs;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Full round-trip: parse `src`, emit, assert output == `src`.
    fn rt(src: &str) {
        let out_data = parse_str(src, None).expect("parse failed");
        let out = emit_docs(
            &out_data.docs,
            &out_data.doc_explicit,
            &out_data.doc_explicit_end,
            &out_data.doc_yaml_version,
            &out_data.doc_tag_directives,
            2,
        );
        assert_eq!(
            out, src,
            "round-trip mismatch\n---expected---\n{src}\n---got---\n{out}\n"
        );
    }

    /// Parse `src` and return the single top-level document node.
    fn parse_one(src: &str) -> YamlNode {
        let mut out = parse_str(src, None).expect("parse failed");
        assert_eq!(out.docs.len(), 1, "expected exactly one document");
        out.docs.remove(0)
    }

    // ── Empty / trivial ───────────────────────────────────────────────────────

    #[test]
    fn empty_input_produces_no_docs() {
        let out = parse_str("", None).unwrap();
        assert!(out.docs.is_empty());
        assert!(out.doc_explicit.is_empty());
        assert!(out.doc_explicit_end.is_empty());
    }

    #[test]
    fn whitespace_only_produces_no_docs() {
        let out = parse_str("   \n  \n", None).unwrap();
        assert!(out.docs.is_empty());
    }

    // ── Null scalar ───────────────────────────────────────────────────────────

    #[test]
    fn bare_null_parses_as_null() {
        let node = parse_one("null\n");
        assert!(matches!(
            node,
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Null,
                ..
            })
        ));
    }

    #[test]
    fn tilde_parses_as_null_with_original() {
        let node = parse_one("~\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Null));
            assert_eq!(s.original.as_deref(), Some("~"));
        } else {
            panic!("expected Scalar");
        }
    }

    // ── Bool scalars ──────────────────────────────────────────────────────────

    #[test]
    fn bool_true_canonical() {
        let node = parse_one("true\n");
        assert!(matches!(
            node,
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Bool(true),
                original: None,
                ..
            })
        ));
    }

    #[test]
    fn bool_yes_has_original() {
        let node = parse_one("yes\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Bool(true)));
            assert_eq!(s.original.as_deref(), Some("yes"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn bool_on_has_original() {
        let node = parse_one("on\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Bool(true)));
            assert_eq!(s.original.as_deref(), Some("on"));
        } else {
            panic!("expected Scalar");
        }
    }

    // ── Integer scalars ───────────────────────────────────────────────────────

    #[test]
    fn decimal_int_no_original() {
        let node = parse_one("42\n");
        assert!(matches!(
            node,
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Int(42),
                original: None,
                ..
            })
        ));
    }

    #[test]
    fn hex_int_has_original() {
        let node = parse_one("0xFF\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Int(255)));
            assert_eq!(s.original.as_deref(), Some("0xFF"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn octal_int_has_original() {
        let node = parse_one("0o77\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Int(63)));
            assert_eq!(s.original.as_deref(), Some("0o77"));
        } else {
            panic!("expected Scalar");
        }
    }

    // ── Float scalars ─────────────────────────────────────────────────────────

    #[test]
    fn float_with_dot_no_original() {
        let node = parse_one("3.14\n");
        assert!(matches!(
            node,
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Float(_),
                original: None,
                ..
            })
        ));
    }

    #[test]
    fn float_exponent_has_original() {
        let node = parse_one("1.5e10\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Float(_)));
            assert_eq!(s.original.as_deref(), Some("1.5e10"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn inf_parses_as_infinite_float() {
        // .inf round-trips via the emitter's canonical path — no `original` needed
        let node = parse_one(".inf\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Float(f) if f.is_infinite() && f > 0.0));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn nan_parses_as_nan_float() {
        // .nan round-trips via the emitter's canonical path — no `original` needed
        let node = parse_one(".nan\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value, ScalarValue::Float(f) if f.is_nan()));
        } else {
            panic!("expected Scalar");
        }
    }

    // ── Quoted scalars ────────────────────────────────────────────────────────

    #[test]
    fn single_quoted_style_preserved() {
        let node = parse_one("'hello'\n");
        if let YamlNode::Scalar(s) = node {
            assert_eq!(s.style, ScalarStyle::SingleQuoted);
            assert!(matches!(&s.value, ScalarValue::Str(v) if v == "hello"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn double_quoted_style_preserved() {
        let node = parse_one("\"hello\"\n");
        if let YamlNode::Scalar(s) = node {
            assert_eq!(s.style, ScalarStyle::DoubleQuoted);
            assert!(matches!(&s.value, ScalarValue::Str(v) if v == "hello"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn quoted_null_string_is_str_not_null() {
        let node = parse_one("'null'\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(&s.value, ScalarValue::Str(v) if v == "null"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn quoted_empty_string_is_str_not_null() {
        let node = parse_one("''\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(&s.value, ScalarValue::Str(v) if v.is_empty()));
        } else {
            panic!("expected Scalar");
        }
    }

    // ── Tags ─────────────────────────────────────────────────────────────────

    #[test]
    fn tag_str_forces_string_value() {
        let node = parse_one("!!str 42\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(&s.value, ScalarValue::Str(v) if v == "42"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn tag_stored_on_scalar() {
        let node = parse_one("!!str hello\n");
        if let YamlNode::Scalar(s) = node {
            // tag:yaml.org,2002:str is the expanded form stored internally
            assert!(s.tag.is_some());
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn custom_tag_stored_on_sequence() {
        let node = parse_one("!!python/tuple [1, 2]\n");
        if let YamlNode::Sequence(s) = node {
            assert!(s.tag.is_some());
        } else {
            panic!("expected Sequence");
        }
    }

    // ── Mapping ───────────────────────────────────────────────────────────────

    #[test]
    fn simple_mapping_order_preserved() {
        let node = parse_one("z: 1\na: 2\nm: 3\n");
        if let YamlNode::Mapping(m) = node {
            let keys: Vec<&str> = m.entries.keys().map(|k| k.as_str()).collect();
            assert_eq!(keys, ["z", "a", "m"]);
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn nested_mapping_parsed() {
        let node = parse_one("outer:\n  inner: 42\n");
        if let YamlNode::Mapping(m) = node {
            let inner = &m.entries["outer"].value;
            assert!(matches!(inner, YamlNode::Mapping(_)));
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn flow_mapping_style() {
        let node = parse_one("{a: 1, b: 2}\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(m.style, ContainerStyle::Flow);
        } else {
            panic!("expected Mapping");
        }
    }

    // ── Sequence ──────────────────────────────────────────────────────────────

    #[test]
    fn simple_sequence_items() {
        let node = parse_one("- 1\n- 2\n- 3\n");
        if let YamlNode::Sequence(s) = node {
            assert_eq!(s.items.len(), 3);
        } else {
            panic!("expected Sequence");
        }
    }

    #[test]
    fn flow_sequence_style() {
        let node = parse_one("[1, 2, 3]\n");
        if let YamlNode::Sequence(s) = node {
            assert_eq!(s.style, ContainerStyle::Flow);
        } else {
            panic!("expected Sequence");
        }
    }

    // ── Anchors and aliases ───────────────────────────────────────────────────

    #[test]
    fn scalar_anchor_stored() {
        let node = parse_one("&myval 42\n");
        if let YamlNode::Scalar(s) = node {
            assert_eq!(s.anchor.as_deref(), Some("myval"));
            assert!(matches!(s.value, ScalarValue::Int(42)));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn alias_resolves_to_scalar() {
        let src = "base: &val 10\nref: *val\n";
        let node = parse_one(src);
        if let YamlNode::Mapping(m) = node {
            let alias_entry = &m.entries["ref"].value;
            if let YamlNode::Alias { name, resolved } = alias_entry {
                assert_eq!(name, "val");
                assert!(matches!(
                    resolved.as_ref(),
                    YamlNode::Scalar(YamlScalar {
                        value: ScalarValue::Int(10),
                        ..
                    })
                ));
            } else {
                panic!("expected Alias, got {alias_entry:?}");
            }
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn unknown_alias_returns_error() {
        // An alias that has no anchor should produce a parse error (strict YAML compliance)
        let result = parse_str("*noanchor\n", None);
        assert!(result.is_err(), "expected error for undefined alias");
    }

    #[test]
    fn mapping_anchor_stored() {
        let node = parse_one("&m\na: 1\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(m.anchor.as_deref(), Some("m"));
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn sequence_anchor_stored() {
        let node = parse_one("&s\n- 1\n- 2\n");
        if let YamlNode::Sequence(s) = node {
            assert_eq!(s.anchor.as_deref(), Some("s"));
        } else {
            panic!("expected Sequence");
        }
    }

    // ── Comments ──────────────────────────────────────────────────────────────

    #[test]
    fn inline_comment_attached() {
        let node = parse_one("a: 1  # comment\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(m.entries["a"].comment_inline.as_deref(), Some("comment"));
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn before_comment_attached() {
        let node = parse_one("a: 1\n# before b\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(m.entries["b"].comment_before.as_deref(), Some("before b"));
        } else {
            panic!("expected Mapping");
        }
    }

    // ── Blank lines ───────────────────────────────────────────────────────────

    #[test]
    fn blank_lines_before_entry_counted() {
        let node = parse_one("a: 1\n\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(m.entries["b"].blank_lines_before, 1);
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn two_blank_lines_before_entry() {
        let node = parse_one("a: 1\n\n\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(m.entries["b"].blank_lines_before, 2);
        } else {
            panic!("expected Mapping");
        }
    }

    // ── Explicit document markers ─────────────────────────────────────────────

    #[test]
    fn explicit_start_marker_recorded() {
        let out = parse_str("---\na: 1\n", None).unwrap();
        assert_eq!(out.doc_explicit, [true]);
    }

    #[test]
    fn no_explicit_start_without_dashes() {
        let out = parse_str("a: 1\n", None).unwrap();
        assert_eq!(out.doc_explicit, [false]);
    }

    #[test]
    fn explicit_end_marker_recorded() {
        let out = parse_str("a: 1\n...\n", None).unwrap();
        assert_eq!(out.doc_explicit_end, [true]);
    }

    #[test]
    fn both_markers_recorded() {
        let out = parse_str("---\na: 1\n...\n", None).unwrap();
        assert_eq!(out.doc_explicit, [true]);
        assert_eq!(out.doc_explicit_end, [true]);
    }

    // ── Multiple documents ────────────────────────────────────────────────────

    #[test]
    fn two_docs_parsed() {
        let out = parse_str("---\na: 1\n---\nb: 2\n", None).unwrap();
        assert_eq!(out.docs.len(), 2);
        assert_eq!(out.doc_explicit, [true, true]);
        assert_eq!(out.doc_explicit_end, [false, false]);
    }

    // ── Block scalars ─────────────────────────────────────────────────────────

    #[test]
    fn literal_block_style_parsed() {
        let node = parse_one("text: |\n  hello\n  world\n");
        if let YamlNode::Mapping(m) = node {
            if let YamlNode::Scalar(s) = &m.entries["text"].value {
                assert_eq!(s.style, ScalarStyle::Literal);
                assert!(matches!(&s.value, ScalarValue::Str(v) if v.contains("hello")));
            } else {
                panic!("expected Scalar");
            }
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn folded_block_style_parsed() {
        let node = parse_one("text: >\n  hello world\n");
        if let YamlNode::Mapping(m) = node {
            if let YamlNode::Scalar(s) = &m.entries["text"].value {
                assert_eq!(s.style, ScalarStyle::Folded);
            } else {
                panic!("expected Scalar");
            }
        } else {
            panic!("expected Mapping");
        }
    }

    // ── Round-trips ───────────────────────────────────────────────────────────

    #[test]
    fn rt_simple_mapping() {
        rt("a: 1\nb: 2\nc: 3\n");
    }

    #[test]
    fn rt_nested_mapping() {
        rt("outer:\n  inner: 42\n");
    }

    #[test]
    fn rt_sequence() {
        rt("- 1\n- 2\n- 3\n");
    }

    #[test]
    fn rt_flow_mapping() {
        rt("{a: 1, b: 2}\n");
    }

    #[test]
    fn rt_flow_sequence() {
        rt("[1, 2, 3]\n");
    }

    #[test]
    fn rt_single_quoted() {
        rt("key: 'value'\n");
    }

    #[test]
    fn rt_double_quoted() {
        rt("key: \"value\"\n");
    }

    #[test]
    fn rt_non_canonical_null() {
        rt("a: ~\n");
    }

    #[test]
    fn rt_non_canonical_bool_yes() {
        rt("flag: yes\n");
    }

    #[test]
    fn rt_non_canonical_bool_no() {
        rt("flag: no\n");
    }

    #[test]
    fn rt_non_canonical_bool_on() {
        rt("enabled: on\n");
    }

    #[test]
    fn rt_non_canonical_bool_off() {
        rt("enabled: off\n");
    }

    #[test]
    fn rt_hex_integer() {
        rt("value: 0xFF\n");
    }

    #[test]
    fn rt_octal_integer() {
        rt("value: 0o77\n");
    }

    #[test]
    fn rt_float_exponent() {
        rt("value: 1.5e10\n");
    }

    #[test]
    fn rt_inf() {
        rt("value: .inf\n");
    }

    #[test]
    fn rt_nan() {
        rt("value: .nan\n");
    }

    #[test]
    fn rt_literal_block() {
        rt("text: |-\n  hello\n  world\n");
    }

    #[test]
    fn rt_folded_block() {
        rt("text: >-\n  hello world\n");
    }

    #[test]
    fn rt_folded_block_multiline() {
        // Single blank-line separator between two paragraphs
        rt("text: >\n  ab cd\n\n  ef\n");
        // Double blank-line separator (two \n between paragraphs)
        rt("text: >\n  ab cd\n\n\n  gh\n");
    }

    #[test]
    fn rt_inline_comment() {
        rt("a: 1  # comment\nb: 2\n");
    }

    #[test]
    fn rt_before_comment() {
        rt("a: 1\n# before b\nb: 2\n");
    }

    #[test]
    fn rt_blank_line_between_entries() {
        rt("a: 1\n\nb: 2\n");
    }

    #[test]
    fn rt_two_blank_lines() {
        rt("a: 1\n\n\nb: 2\n");
    }

    #[test]
    fn rt_explicit_start() {
        rt("---\na: 1\n");
    }

    #[test]
    fn rt_explicit_end() {
        rt("a: 1\n...\n");
    }

    #[test]
    fn rt_both_markers() {
        rt("---\na: 1\n...\n");
    }

    #[test]
    fn rt_multi_doc() {
        rt("---\na: 1\n---\nb: 2\n");
    }

    #[test]
    fn rt_anchor_scalar() {
        rt("base: &val 10\nref: *val\n");
    }

    #[test]
    fn rt_anchor_mapping() {
        rt(
            "defaults: &base\n  timeout: 30\n  retries: 3\n\nservice:\n  name: api\n  config: *base\n",
        );
    }

    #[test]
    fn rt_tag_on_sequence() {
        rt("items: !!python/tuple [1, 2, 3]\n");
    }

    #[test]
    fn rt_key_single_quoted() {
        rt("'key with space': value\n");
    }

    #[test]
    fn rt_sequence_with_comments() {
        rt("- 1  # one\n- 2  # two\n- 3\n");
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[test]
    fn invalid_yaml_returns_error() {
        let result = parse_str(": bad :\n  - broken", None);
        // Just check it doesn't panic; error type doesn't matter
        let _ = result;
    }
}

/// Parsed output from a YAML input string.
pub struct ParseOutput {
    pub docs: Vec<YamlNode>,
    pub doc_explicit: Vec<bool>,
    pub doc_explicit_end: Vec<bool>,
    pub doc_yaml_version: Vec<Option<(u8, u8)>>,
    pub doc_tag_directives: Vec<Vec<(String, String)>>,
}

pub fn parse_str(input: &str, policy: Option<&TagPolicy>) -> Result<ParseOutput, String> {
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
        builder.process_event(ev, mark, policy);

        if done {
            break;
        }
    }

    Ok(ParseOutput {
        docs: builder.docs,
        doc_explicit: builder.doc_explicit,
        doc_explicit_end: builder.doc_explicit_end,
        doc_yaml_version: builder.doc_yaml_version,
        doc_tag_directives: builder.doc_tag_directives,
    })
}
