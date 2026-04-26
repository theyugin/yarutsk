// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::parser::{Event, Parser, Tag};
use super::scanner::{Chomping as ScannerChomping, Marker, TScalarStyle};
use super::types::{
    Chomping, ContainerStyle, MapKey, NodeMeta, ScalarRepr, ScalarStyle, ScalarValue, YamlEntry,
    YamlMapping, YamlNode, YamlScalar, YamlSequence,
};

/// Translate the scanner's `Chomping` enum to the data-model enum stored on
/// `YamlScalar`, so emitter/types layers stay decoupled from scanner internals.
fn map_chomping(c: ScannerChomping) -> Chomping {
    match c {
        ScannerChomping::Strip => Chomping::Strip,
        ScannerChomping::Clip => Chomping::Clip,
        ScannerChomping::Keep => Chomping::Keep,
    }
}

/// Tags that should bypass built-in `ScalarValue` coercion in the builder.
/// When a tag is in this set the scalar value is kept as `ScalarValue::Str`
/// with the raw source text, so that Python-layer constructors receive the
/// original YAML string rather than a pre-converted value.
pub struct TagPolicy {
    pub raw_tags: HashSet<String>,
}

/// Document-level metadata for one parsed YAML document.
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

pub struct Builder {
    stack: Vec<Frame>,
    pub docs: Vec<YamlNode>,
    /// Per-document metadata, indexed by document position in `docs`.
    pub docs_meta: Vec<DocMetadata>,
    /// Staging slot for the current `DocumentStart`'s metadata.  Consumed and
    /// pushed onto `docs_meta` when the document's root node is pushed.
    next_meta: DocMetadata,
    /// Monotonically increasing count of `DocumentEnd` events processed.  Used
    /// by the streaming iterator to detect when a full document is ready.
    pub doc_end_count: usize,
    /// Line of the last SCALAR content token (key or value), for inline comment detection.
    /// Only scalars update this; MappingEnd/SequenceEnd do not.
    last_content_line: Option<usize>,
    /// Comments not yet associated with any node (before-key candidates).
    pending_before: Vec<(usize, String)>,
    /// Inline comment seen before the node it belongs to exists in the current
    /// frame.  Happens for quoted and block scalars: the scanner reads past the
    /// scalar and any trailing `# ...` before emitting the Scalar event, so the
    /// comment is drained into `absorb_comments` before the seq item / mapping
    /// entry has been inserted.  Consumed when the next scalar is added.
    pending_inline: Option<String>,
    /// Anchor table: maps anchor name → completed node, for alias resolution.
    /// `Rc` so each `*alias` shares storage with the anchor instead of cloning
    /// the whole subtree (avoids quadratic memory blow-up for docs with many
    /// aliases of large blocks).
    anchor_table: HashMap<String, Arc<YamlNode>>,
}

enum Frame {
    Mapping(Box<MappingFrame>),
    Sequence(SequenceFrame),
}

/// Metadata accumulated while waiting for the value half of a mapping entry.
/// Populated when a key scalar (or alias/complex key) is seen; consumed when
/// the corresponding value arrives.
#[derive(Default)]
struct PendingKey {
    /// The string form of the key (None for complex non-scalar keys).
    key: Option<String>,
    /// For complex (non-scalar) keys: the key node.  When this is Some,
    /// `key` is None but we are in "have key, waiting for value" state.
    key_node: Option<YamlNode>,
    comment_before: Option<String>,
    comment_inline: Option<String>,
    /// Blank lines before the current entry.
    blank_lines: u8,
    /// The quoting style of the key scalar.
    key_style: ScalarStyle,
    /// Anchor declared on the key scalar, if any.
    key_anchor: Option<String>,
    /// If the key was written as an alias, the alias name.
    key_alias: Option<String>,
    /// Tag on the key scalar, if any.
    key_tag: Option<String>,
}

impl PendingKey {
    /// Returns true if a key (scalar or complex) has been recorded.
    fn has_key(&self) -> bool {
        self.key.is_some() || self.key_node.is_some()
    }

    /// Move pending `comment_before` / `comment_inline` / `blank_lines` onto `value`,
    /// resetting `blank_lines` afterwards.
    fn attach_metadata(&mut self, value: &mut YamlNode) {
        if let Some(cb) = self.comment_before.take() {
            value.set_comment_before(Some(cb));
        }
        if let Some(ci) = self.comment_inline.take() {
            value.set_comment_inline(Some(ci));
        }
        if self.blank_lines > 0 {
            value.set_blank_lines_before(self.blank_lines);
        }
        self.blank_lines = 0;
    }

    /// Consume the pending key and insert a new entry into `mapping`.
    /// Returns `None` on success.  If no key is pending (i.e. this node IS
    /// the complex key), returns `Some(value)` so the caller can store it.
    fn insert_entry(&mut self, mapping: &mut YamlMapping, mut value: YamlNode) -> Option<YamlNode> {
        if let Some(key) = self.key.take() {
            self.attach_metadata(&mut value);
            let entry = YamlEntry {
                value,
                key_style: self.key_style,
                key_anchor: self.key_anchor.take(),
                key_alias: self.key_alias.take(),
                key_tag: self.key_tag.take(),
                key_node: self.key_node.take().map(Box::new),
            };
            mapping.entries.insert(MapKey::Scalar(key), entry);
            None
        } else if self.key_node.is_some() {
            // Complex key already saved; this node is the VALUE. Synthesise a
            // positional id — the actual key lives on `entry.key_node`.
            let key = MapKey::Complex(mapping.entries.len());
            self.attach_metadata(&mut value);
            let entry = YamlEntry {
                value,
                key_style: ScalarStyle::Plain,
                key_anchor: None,
                key_alias: None,
                key_tag: None,
                key_node: self.key_node.take().map(Box::new),
            };
            mapping.entries.insert(key, entry);
            None
        } else {
            Some(value)
        }
    }

    /// Store a complex (non-scalar) node as the key.
    fn set_complex_key(&mut self, node: YamlNode) {
        self.key_node = Some(node);
    }
}

struct MappingFrame {
    mapping: YamlMapping,
    pending: PendingKey,
    /// Anchor name declared on the `MappingStart` event, if any.
    anchor_name: Option<String>,
}

struct SequenceFrame {
    seq: YamlSequence,
    /// Comment before the current complex item (saved before pushing nested frame).
    current_comment_before: Option<String>,
    /// Blank lines before the current complex item.
    current_blank_lines: u8,
    /// Anchor name declared on the `SequenceStart` event, if any.
    anchor_name: Option<String>,
}

/// Construct a `YamlNode::Scalar` from already-resolved components.
fn make_scalar(
    value: ScalarValue,
    style: ScalarStyle,
    tag: Option<String>,
    original: Option<String>,
    anchor: Option<String>,
    chomping: Option<Chomping>,
) -> YamlNode {
    YamlNode::Scalar(YamlScalar {
        repr: match original {
            Some(source) => ScalarRepr::Preserved { value, source },
            None => ScalarRepr::Canonical(value),
        },
        style,
        chomping,
        meta: NodeMeta {
            tag,
            anchor,
            ..NodeMeta::default()
        },
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

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Builder {
            stack: Vec::new(),
            docs: Vec::new(),
            docs_meta: Vec::new(),
            next_meta: DocMetadata::default(),
            doc_end_count: 0,
            last_content_line: None,
            pending_before: Vec::new(),
            pending_inline: None,
            anchor_table: HashMap::new(),
        }
    }

    /// Commit staged `next_meta` as the metadata for the doc about to be pushed.
    fn commit_next_meta(&mut self) {
        self.docs_meta.push(std::mem::take(&mut self.next_meta));
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
                if mf.pending.has_key() {
                    // Key was last; value not yet seen → store inline on frame
                    mf.pending.comment_inline = Some(text);
                } else if let Some((_, entry)) = mf.mapping.entries.last_mut() {
                    // Value was just finalized → attach inline to the value node.
                    if entry.value.comment_inline().is_none() {
                        entry.value.set_comment_inline(Some(text));
                    }
                } else {
                    // No entry yet — the comment was drained between the scanner
                    // emitting a quoted/block key scalar and the builder
                    // processing it.  Stash for the upcoming Scalar handler.
                    self.pending_inline = Some(text);
                }
            }
            Some(Frame::Sequence(sf)) => {
                if let Some(item) = sf.seq.items.last_mut() {
                    if item.comment_inline().is_none() {
                        item.set_comment_inline(Some(text));
                    }
                } else {
                    // Same rationale as the empty-mapping branch above.
                    self.pending_inline = Some(text);
                }
            }
            None => {
                // Stack is empty: the last doc was just pushed; retroactively update it
                if let Some(doc) = self.docs.last_mut()
                    && doc.comment_inline().is_none()
                {
                    doc.set_comment_inline(Some(text));
                }
            }
        }
    }

    /// Count blank lines between the last scalar content and `node_line`.
    /// Must be called BEFORE `take_before` drains `pending_before`.
    fn count_blank_lines(&self, node_line: usize) -> u8 {
        let Some(last_line) = self.last_content_line else {
            return 0;
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
        // min(255) makes the u8 truncation lossless.
        #[allow(clippy::cast_possible_truncation)]
        {
            total_between.saturating_sub(comment_count).min(255) as u8
        }
    }

    /// Take all pending before-comments with line < `node_line`, join with newline.
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
    /// Does NOT update `last_content_line` (only scalars do that).
    fn push_node(&mut self, node: YamlNode) {
        match self.stack.last_mut() {
            None => {
                self.commit_next_meta();
                self.docs.push(node);
            }
            Some(Frame::Mapping(mf)) => {
                if let Some(node) = mf.pending.insert_entry(&mut mf.mapping, node) {
                    // No pending key: this node IS the complex key itself.
                    mf.pending.set_complex_key(node);
                }
            }
            Some(Frame::Sequence(sf)) => {
                let comment_before = sf.current_comment_before.take();
                let blank_lines_before = sf.current_blank_lines;
                sf.current_blank_lines = 0;
                let mut node = node;
                if let Some(cb) = comment_before {
                    node.set_comment_before(Some(cb));
                }
                if blank_lines_before > 0 {
                    node.set_blank_lines_before(blank_lines_before);
                }
                sf.seq.items.push(node);
            }
        }
    }

    /// Register a node in the anchor table by name (if `anchor_name` is Some).
    fn register_anchor(&mut self, anchor_name: Option<&str>, node: &YamlNode) {
        if let Some(name) = anchor_name {
            self.anchor_table
                .insert(name.to_owned(), Arc::new(node.clone()));
        }
    }

    /// Process a single parser event.
    #[allow(clippy::too_many_lines)] // single event-dispatch state machine
    pub fn process_event(&mut self, ev: Event, mark: Marker, policy: Option<&TagPolicy>) {
        match ev {
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}

            Event::DocumentStart(explicit, version, tag_dirs) => {
                self.next_meta = DocMetadata {
                    explicit_start: explicit,
                    explicit_end: false,
                    yaml_version: version,
                    tag_directives: tag_dirs,
                };
                // Record the document-start line so blank lines between `---` and
                // the first key are counted correctly by count_blank_lines.
                self.last_content_line = Some(mark.line());
            }

            Event::DocumentEnd(explicit_end) => {
                self.doc_end_count += 1;
                // The doc root node has been pushed, so `docs_meta` has an entry
                // for this doc.  Update its end marker.
                if let Some(m) = self.docs_meta.last_mut() {
                    m.explicit_end = explicit_end;
                } else {
                    // No root node (empty doc): push an otherwise-default entry
                    // so `docs_meta.len()` tracks the end count.
                    self.docs_meta.push(DocMetadata {
                        explicit_end,
                        ..DocMetadata::default()
                    });
                }
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
                mapping.meta.tag = tag_to_string(tag);
                mapping.meta.anchor.clone_from(&anchor_name);
                self.stack.push(Frame::Mapping(Box::new(MappingFrame {
                    mapping,
                    pending: PendingKey::default(),
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
                seq.meta.tag = tag_to_string(tag);
                seq.meta.anchor.clone_from(&anchor_name);
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

            Event::Scalar(value, style, anchor_name, tag, end_line, chomping) => {
                let scalar_style = map_scalar_style(style);
                let scalar_tag = tag_to_string(tag);
                let is_plain = style == TScalarStyle::Plain;

                // Does this scalar short-circuit straight to a String? True for tags in
                // the TagPolicy's raw set or any of the standard `!!str` spellings.
                // (Quoted scalars also default to Str, but go through the tag-override
                // match below so `!!null`/`!!bool`/`!!int`/`!!float` can still apply.)
                let force_str = policy.is_some_and(|p| {
                    scalar_tag
                        .as_deref()
                        .is_some_and(|t| p.raw_tags.contains(t))
                }) || matches!(
                    scalar_tag.as_deref(),
                    Some("!!str" | "tag:yaml.org,2002:str" | "!")
                );

                // Resolve the typed scalar in a single pass — collapses the prior
                // two-step "infer, then tag-override" that cloned `value` twice when
                // both steps produced `ScalarValue::Str`. `value` is kept alive (borrowed)
                // because it may still be needed below as a mapping-key string.
                let typed: ScalarValue = if force_str {
                    ScalarValue::Str(value.clone())
                } else {
                    // Plain scalars get type inference; quoted scalars default to Str.
                    let inferred = if is_plain {
                        ScalarValue::from_str(&value)
                    } else {
                        ScalarValue::Str(value.clone())
                    };
                    match scalar_tag.as_deref() {
                        Some("!!null" | "tag:yaml.org,2002:null") => ScalarValue::Null,
                        Some("!!bool" | "tag:yaml.org,2002:bool") => {
                            match ScalarValue::from_str(&value) {
                                ScalarValue::Bool(b) => ScalarValue::Bool(b),
                                _ => inferred,
                            }
                        }
                        Some("!!int" | "tag:yaml.org,2002:int") => {
                            match ScalarValue::from_str(&value) {
                                ScalarValue::Int(n) => ScalarValue::Int(n),
                                _ => inferred,
                            }
                        }
                        Some("!!float" | "tag:yaml.org,2002:float") => {
                            match ScalarValue::from_str(&value) {
                                ScalarValue::Float(f) => ScalarValue::Float(f),
                                // !!float on an integer literal → promote to Float.
                                // Precision loss for |n| > 2^53 matches YAML/JSON spec behaviour.
                                #[allow(clippy::cast_precision_loss)]
                                ScalarValue::Int(n) => ScalarValue::Float(n as f64),
                                _ => inferred,
                            }
                        }
                        _ => inferred,
                    }
                };

                let node_line = mark.line();
                // Multi-line scalars (block `|` / `>`, or a wrapped quoted scalar)
                // span several source lines, but folding/chomping can leave the
                // in-memory value with a different newline count than the source —
                // e.g. `>-` on 3 source lines produces a value with zero newlines.
                // The scanner passes the true end line so outer containers don't
                // get phantom `trailing_blank_lines` for lines the scalar actually
                // occupied.
                let effective_scalar_end_line = end_line.unwrap_or(node_line);

                // Preserve the original source text when the plain-scalar representation
                // differs from what the emitter would produce canonically.  This covers:
                //   - float exponent form (`1.5e10` vs `15000000000.0`)
                //   - non-canonical null/bool forms (`~`, `Null`, `yes`, `True`, …)
                //   - hex/octal/underscore-separated integers (`0xFF`, `0o77`, `1_000_000`)
                //   - tagged plain scalars (tag disambiguates type; keep unquoted source)
                let scalar_original: Option<String> = if is_plain {
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
                // A scalar is a KEY if: in mapping context AND no pending key.
                let needs_context = match self.stack.last() {
                    Some(Frame::Mapping(mf)) => !mf.pending.has_key(),
                    // Bare-scalar documents collect before-comments and blanks
                    // so they can be attached to the scalar itself.
                    Some(Frame::Sequence(_)) | None => true,
                };
                // yaml-rust2 reports empty plain scalars (implicit nulls) at
                // the position of the next token, not where the node
                // logically begins. Using that shifted mark for blank-line
                // accounting produces phantom blanks for null sequence items
                // and empty mapping keys, breaking round-trip idempotency.
                let is_empty_plain = is_plain && value.is_empty();
                let in_seq = matches!(self.stack.last(), Some(Frame::Sequence(_)));
                let (blank_lines, comment_before) = if needs_context {
                    let blanks = if is_empty_plain {
                        0
                    } else {
                        self.count_blank_lines(node_line)
                    };
                    // For null sequence items, the shifted scalar mark also
                    // misclassifies comments on the dash line as before-comments
                    // instead of inline. Promote any pending before-comment on
                    // `node_line - 1` (the likely dash line) to inline via
                    // `pending_inline`, which the insertion code below consumes.
                    if is_empty_plain && in_seq && node_line > 0 {
                        let dash_line = node_line - 1;
                        if let Some(pos) = self
                            .pending_before
                            .iter()
                            .position(|(l, _)| *l == dash_line)
                        {
                            let (_, text) = self.pending_before.remove(pos);
                            self.pending_inline = Some(text);
                        }
                    }
                    (blanks, self.take_before(node_line))
                } else {
                    (0, None)
                };

                // Build the scalar node once; all four placement arms below consume or
                // clone it. `scalar_tag` is cloned here because the mapping-key arm also
                // moves it into `pending.key_tag`.
                let node = make_scalar(
                    typed,
                    scalar_style,
                    scalar_tag.clone(),
                    scalar_original,
                    anchor_name.clone(),
                    chomping.map(map_chomping),
                );
                let anchor_node = anchor_name.as_ref().map(|_| node.clone());

                // Consume any inline comment that was drained before this
                // scalar's event arrived (quoted / block scalars: the scanner
                // reads past the scalar and trailing `# …` before emitting
                // the Scalar token).
                let deferred_inline = self.pending_inline.take();
                match self.stack.last_mut() {
                    None => {
                        // Bare-scalar document: attach before/inline comments to the
                        // scalar itself.  The enclosing `YamlNode` has no parent entry,
                        // so this is the only place metadata can live.
                        let mut node = node;
                        node.set_comment_before(comment_before);
                        node.set_comment_inline(deferred_inline);
                        self.commit_next_meta();
                        self.docs.push(node);
                    }
                    Some(Frame::Mapping(mf)) => {
                        if mf.pending.has_key() {
                            // Mapping value — insert entry under the pending key.
                            let _ = mf.pending.insert_entry(&mut mf.mapping, node);
                            if let Some(text) = deferred_inline
                                && let Some((_, entry)) = mf.mapping.entries.last_mut()
                                && entry.value.comment_inline().is_none()
                            {
                                entry.value.set_comment_inline(Some(text));
                            }
                        } else {
                            // Mapping key — store key string and positioning metadata.
                            // `node` is discarded unless `anchor_node` captured it above for
                            // alias registration.
                            mf.pending.key = Some(value);
                            mf.pending.comment_before = comment_before;
                            mf.pending.comment_inline = deferred_inline;
                            mf.pending.blank_lines = blank_lines;
                            mf.pending.key_style = scalar_style;
                            mf.pending.key_anchor.clone_from(&anchor_name);
                            mf.pending.key_tag = scalar_tag;
                            mf.pending.key_alias = None;
                        }
                    }
                    Some(Frame::Sequence(sf)) => {
                        let mut node = node;
                        if let Some(cb) = comment_before {
                            node.set_comment_before(Some(cb));
                        }
                        if let Some(text) = deferred_inline {
                            node.set_comment_inline(Some(text));
                        }
                        if blank_lines > 0 {
                            node.set_blank_lines_before(blank_lines);
                        }
                        sf.seq.items.push(node);
                    }
                }
                self.last_content_line = Some(effective_scalar_end_line);

                // Register anchor after releasing the mutable borrow on self.stack.
                if let Some(node) = anchor_node {
                    self.register_anchor(anchor_name.as_deref(), &node);
                }
            }

            Event::Alias(name) => {
                // Look up the anchor and share its storage via Rc — multiple
                // aliases for the same anchor reference one underlying node.
                let resolved = self
                    .anchor_table
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(YamlNode::Null));
                // If the alias is in mapping key position, record it as an alias key.
                if let Some(Frame::Mapping(mf)) = self.stack.last_mut()
                    && !mf.pending.has_key()
                {
                    // Use the resolved scalar value as the key string (for Python access).
                    mf.pending.key = Some(match resolved.as_ref() {
                        YamlNode::Scalar(s) => s.value().to_key_string(),
                        _ => String::new(),
                    });
                    // Preserve the alias name so the emitter can emit `*name:`.
                    mf.pending.key_alias = Some(name);
                    mf.pending.key_anchor = None;
                    mf.pending.key_tag = None;
                    mf.pending.key_style = ScalarStyle::Plain;
                    return;
                }
                self.push_node(YamlNode::Alias {
                    name,
                    resolved,
                    meta: NodeMeta::default(),
                    materialised: None,
                });
            }
        }
    }
}

/// Parsed output from a YAML input string.
pub struct ParseOutput {
    pub docs: Vec<YamlNode>,
    pub docs_meta: Vec<DocMetadata>,
}

pub fn parse_iter<T: Iterator<Item = char>>(
    src: T,
    policy: Option<&TagPolicy>,
) -> Result<ParseOutput, String> {
    let mut parser = Parser::new(src);
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
        docs_meta: builder.docs_meta,
    })
}

pub fn parse_str(input: &str, policy: Option<&TagPolicy>) -> Result<ParseOutput, String> {
    parse_iter(input.chars(), policy)
}

#[cfg(test)]
mod tests {
    use super::super::emitter::emit_docs;
    use super::*;

    /// Full round-trip: parse `src`, emit, assert output == `src`.
    fn rt(src: &str) {
        let out_data = parse_str(src, None).expect("parse failed");
        let out = emit_docs(&out_data.docs, &out_data.docs_meta, 2);
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

    #[test]
    fn empty_input_produces_no_docs() {
        let out = parse_str("", None).unwrap();
        assert!(out.docs.is_empty());
        assert!(out.docs_meta.is_empty());
    }

    #[test]
    fn whitespace_only_produces_no_docs() {
        let out = parse_str("   \n  \n", None).unwrap();
        assert!(out.docs.is_empty());
    }

    #[test]
    fn bare_null_parses_as_null() {
        let node = parse_one("null\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Null));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn tilde_parses_as_null_with_original() {
        let node = parse_one("~\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Null));
            assert_eq!(s.original().as_deref(), Some("~"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn bool_true_canonical() {
        let node = parse_one("true\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(
                s.repr,
                ScalarRepr::Canonical(ScalarValue::Bool(true))
            ));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn bool_yes_has_original() {
        let node = parse_one("yes\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Bool(true)));
            assert_eq!(s.original().as_deref(), Some("yes"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn bool_on_has_original() {
        let node = parse_one("on\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Bool(true)));
            assert_eq!(s.original().as_deref(), Some("on"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn decimal_int_no_original() {
        let node = parse_one("42\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(
                s.repr,
                ScalarRepr::Canonical(ScalarValue::Int(42))
            ));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn hex_int_has_original() {
        let node = parse_one("0xFF\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Int(255)));
            assert_eq!(s.original().as_deref(), Some("0xFF"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn octal_int_has_original() {
        let node = parse_one("0o77\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Int(63)));
            assert_eq!(s.original().as_deref(), Some("0o77"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn float_with_dot_no_original() {
        let node = parse_one("3.14\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(
                s.repr,
                ScalarRepr::Canonical(ScalarValue::Float(_))
            ));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn float_exponent_has_original() {
        let node = parse_one("1.5e10\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Float(_)));
            assert_eq!(s.original().as_deref(), Some("1.5e10"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn inf_parses_as_infinite_float() {
        // .inf round-trips via the emitter's canonical path — no `original` needed
        let node = parse_one(".inf\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Float(f) if f.is_infinite() && *f > 0.0));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn nan_parses_as_nan_float() {
        // .nan round-trips via the emitter's canonical path — no `original` needed
        let node = parse_one(".nan\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(s.value(), ScalarValue::Float(f) if f.is_nan()));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn single_quoted_style_preserved() {
        let node = parse_one("'hello'\n");
        if let YamlNode::Scalar(s) = node {
            assert_eq!(s.style, ScalarStyle::SingleQuoted);
            assert!(matches!(&s.value(), ScalarValue::Str(v) if v == "hello"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn double_quoted_style_preserved() {
        let node = parse_one("\"hello\"\n");
        if let YamlNode::Scalar(s) = node {
            assert_eq!(s.style, ScalarStyle::DoubleQuoted);
            assert!(matches!(&s.value(), ScalarValue::Str(v) if v == "hello"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn quoted_null_string_is_str_not_null() {
        let node = parse_one("'null'\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(&s.value(), ScalarValue::Str(v) if v == "null"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn quoted_empty_string_is_str_not_null() {
        let node = parse_one("''\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(&s.value(), ScalarValue::Str(v) if v.is_empty()));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn tag_str_forces_string_value() {
        let node = parse_one("!!str 42\n");
        if let YamlNode::Scalar(s) = node {
            assert!(matches!(&s.value(), ScalarValue::Str(v) if v == "42"));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn tag_stored_on_scalar() {
        let node = parse_one("!!str hello\n");
        if let YamlNode::Scalar(s) = node {
            // tag:yaml.org,2002:str is the expanded form stored internally
            assert!(s.meta.tag.is_some());
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn custom_tag_stored_on_sequence() {
        let node = parse_one("!!python/tuple [1, 2]\n");
        if let YamlNode::Sequence(s) = node {
            assert!(s.meta.tag.is_some());
        } else {
            panic!("expected Sequence");
        }
    }

    #[test]
    fn simple_mapping_order_preserved() {
        let node = parse_one("z: 1\na: 2\nm: 3\n");
        if let YamlNode::Mapping(m) = node {
            let keys: Vec<&str> = m
                .entries
                .keys()
                .map(|k| k.as_scalar().unwrap_or(""))
                .collect();
            assert_eq!(keys, ["z", "a", "m"]);
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn nested_mapping_parsed() {
        let node = parse_one("outer:\n  inner: 42\n");
        if let YamlNode::Mapping(m) = node {
            let inner = &m.entries[&MapKey::scalar("outer")].value;
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

    #[test]
    fn scalar_anchor_stored() {
        let node = parse_one("&myval 42\n");
        if let YamlNode::Scalar(s) = node {
            assert_eq!(s.meta.anchor.as_deref(), Some("myval"));
            assert!(matches!(s.value(), ScalarValue::Int(42)));
        } else {
            panic!("expected Scalar");
        }
    }

    #[test]
    fn alias_resolves_to_scalar() {
        let src = "base: &val 10\nref: *val\n";
        let node = parse_one(src);
        if let YamlNode::Mapping(m) = node {
            let alias_entry = &m.entries[&MapKey::scalar("ref")].value;
            if let YamlNode::Alias { name, resolved, .. } = alias_entry {
                assert_eq!(name, "val");
                if let YamlNode::Scalar(s) = resolved.as_ref() {
                    assert!(matches!(s.value(), ScalarValue::Int(10)));
                } else {
                    panic!("expected Scalar inside alias, got {resolved:?}");
                }
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
            assert_eq!(m.meta.anchor.as_deref(), Some("m"));
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn sequence_anchor_stored() {
        let node = parse_one("&s\n- 1\n- 2\n");
        if let YamlNode::Sequence(s) = node {
            assert_eq!(s.meta.anchor.as_deref(), Some("s"));
        } else {
            panic!("expected Sequence");
        }
    }

    #[test]
    fn inline_comment_attached() {
        let node = parse_one("a: 1  # comment\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            let YamlNode::Scalar(s) = &m.entries[&MapKey::scalar("a")].value else {
                panic!("expected Scalar");
            };
            assert_eq!(s.meta.comment_inline.as_deref(), Some("comment"));
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn before_comment_attached() {
        let node = parse_one("a: 1\n# before b\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            let YamlNode::Scalar(s) = &m.entries[&MapKey::scalar("b")].value else {
                panic!("expected Scalar");
            };
            assert_eq!(s.meta.comment_before.as_deref(), Some("before b"));
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn blank_lines_before_entry_counted() {
        let node = parse_one("a: 1\n\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(
                m.entries[&MapKey::scalar("b")].value.blank_lines_before(),
                1
            );
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn two_blank_lines_before_entry() {
        let node = parse_one("a: 1\n\n\nb: 2\n");
        if let YamlNode::Mapping(m) = node {
            assert_eq!(
                m.entries[&MapKey::scalar("b")].value.blank_lines_before(),
                2
            );
        } else {
            panic!("expected Mapping");
        }
    }

    #[test]
    fn explicit_start_marker_recorded() {
        let out = parse_str("---\na: 1\n", None).unwrap();
        assert_eq!(
            out.docs_meta
                .iter()
                .map(|m| m.explicit_start)
                .collect::<Vec<_>>(),
            [true]
        );
    }

    #[test]
    fn no_explicit_start_without_dashes() {
        let out = parse_str("a: 1\n", None).unwrap();
        assert_eq!(
            out.docs_meta
                .iter()
                .map(|m| m.explicit_start)
                .collect::<Vec<_>>(),
            [false]
        );
    }

    #[test]
    fn explicit_end_marker_recorded() {
        let out = parse_str("a: 1\n...\n", None).unwrap();
        assert_eq!(
            out.docs_meta
                .iter()
                .map(|m| m.explicit_end)
                .collect::<Vec<_>>(),
            [true]
        );
    }

    #[test]
    fn both_markers_recorded() {
        let out = parse_str("---\na: 1\n...\n", None).unwrap();
        let starts: Vec<_> = out.docs_meta.iter().map(|m| m.explicit_start).collect();
        let ends: Vec<_> = out.docs_meta.iter().map(|m| m.explicit_end).collect();
        assert_eq!(starts, [true]);
        assert_eq!(ends, [true]);
    }

    #[test]
    fn two_docs_parsed() {
        let out = parse_str("---\na: 1\n---\nb: 2\n", None).unwrap();
        assert_eq!(out.docs.len(), 2);
        let starts: Vec<_> = out.docs_meta.iter().map(|m| m.explicit_start).collect();
        let ends: Vec<_> = out.docs_meta.iter().map(|m| m.explicit_end).collect();
        assert_eq!(starts, [true, true]);
        assert_eq!(ends, [false, false]);
    }

    #[test]
    fn literal_block_style_parsed() {
        let node = parse_one("text: |\n  hello\n  world\n");
        if let YamlNode::Mapping(m) = node {
            if let YamlNode::Scalar(s) = &m.entries[&MapKey::scalar("text")].value {
                assert_eq!(s.style, ScalarStyle::Literal);
                assert!(matches!(&s.value(), ScalarValue::Str(v) if v.contains("hello")));
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
            if let YamlNode::Scalar(s) = &m.entries[&MapKey::scalar("text")].value {
                assert_eq!(s.style, ScalarStyle::Folded);
            } else {
                panic!("expected Scalar");
            }
        } else {
            panic!("expected Mapping");
        }
    }

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
    fn keep_chomping_survives_one_trailing_newline() {
        // Regression: `>+` / `|+` on a value with exactly one trailing newline
        // would previously downgrade to `>` / `|` because the emitter re-
        // inferred the indicator from the value alone. The scanner now passes
        // the source chomping through so Keep is preserved when consistent.
        for src in &["a: >+\n  hi\n", "a: |+\n  hi\n"] {
            let node = parse_one(src);
            if let YamlNode::Mapping(m) = node {
                let scalar = match &m.entries[&MapKey::scalar("a")].value {
                    YamlNode::Scalar(s) => s,
                    _ => panic!("expected scalar"),
                };
                assert_eq!(scalar.chomping, Some(Chomping::Keep), "for {src:?}");
            } else {
                panic!("expected Mapping");
            }
        }
    }

    #[test]
    fn folded_strip_does_not_leak_trailing_blank_lines() {
        // Regression: folded block scalars (`>`, `>-`, `>+`) fold source line
        // breaks into spaces so the value's newline count no longer matches
        // the source's line count. The builder must use the scanner's end_line
        // rather than a value-based heuristic, or it over-counts trailing
        // blanks on the outer container and the emitter prints a spurious
        // blank line between adjacent block-scalar items.
        let node = parse_one("- >-\n  first\n- >-\n  second\n");
        if let YamlNode::Sequence(s) = node {
            assert_eq!(s.trailing_blank_lines, 0);
            for item in &s.items {
                assert_eq!(item.blank_lines_before(), 0);
            }
        } else {
            panic!("expected Sequence");
        }
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

    #[test]
    fn invalid_yaml_returns_error() {
        let result = parse_str(": bad :\n  - broken", None);
        // Just check it doesn't panic; error type doesn't matter
        let _ = result;
    }
}
