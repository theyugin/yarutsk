// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use crate::parser::{Event, Parser};
use crate::scanner::{Marker, TScalarStyle};
use crate::types::*;

pub struct Builder {
    stack: Vec<Frame>,
    pub docs: Vec<YamlNode>,
    /// Line of the last SCALAR content token (key or value), for inline comment detection.
    /// Only scalars update this; MappingEnd/SequenceEnd do not.
    last_content_line: Option<usize>,
    /// Comments not yet associated with any node (before-key candidates).
    pending_before: Vec<(usize, String)>,
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
}

struct SequenceFrame {
    seq: YamlSequence,
    /// Comment before the current complex item (saved before pushing nested frame).
    current_comment_before: Option<String>,
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            stack: Vec::new(),
            docs: Vec::new(),
            last_content_line: None,
            pending_before: Vec::new(),
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

    /// Take all pending before-comments with line < node_line, join with newline.
    fn take_before(&mut self, node_line: usize) -> Option<String> {
        let before: Vec<String> = self
            .pending_before
            .drain(..)
            .filter(|(line, _)| *line < node_line)
            .map(|(_, t)| t)
            .collect();
        if before.is_empty() {
            None
        } else {
            Some(before.join("\n"))
        }
    }

    /// Push a completed node into the current parent context.
    /// Does NOT update last_content_line (only scalars do that).
    fn push_node(&mut self, node: YamlNode) {
        match self.stack.last_mut() {
            None => {
                self.docs.push(node);
            }
            Some(Frame::Mapping(mf)) => {
                if let Some(key) = mf.current_key.take() {
                    let comment_before = mf.current_comment_before.take();
                    let comment_inline = mf.current_comment_inline.take();
                    mf.mapping.entries.insert(
                        key,
                        YamlEntry {
                            value: node,
                            comment_before,
                            comment_inline,
                        },
                    );
                }
            }
            Some(Frame::Sequence(sf)) => {
                let comment_before = sf.current_comment_before.take();
                sf.seq.items.push(YamlItem {
                    value: node,
                    comment_before,
                    comment_inline: None,
                });
            }
        }
    }

    /// Process a single parser event.
    pub fn process_event(&mut self, ev: Event, mark: Marker) {
        match ev {
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}

            Event::DocumentStart | Event::DocumentEnd => {}

            Event::MappingStart(_, _) => {
                let is_seq_parent = matches!(self.stack.last(), Some(Frame::Sequence(_)));
                if is_seq_parent {
                    // Only drain before-comments when our parent is a sequence item;
                    // for mapping/root parents, leave comments in pending_before so the
                    // first key scalar can pick them up.
                    let before = self.take_before(mark.line());
                    if let Some(Frame::Sequence(sf)) = self.stack.last_mut() {
                        sf.current_comment_before = before;
                    }
                }
                self.stack.push(Frame::Mapping(MappingFrame {
                    mapping: YamlMapping::new(),
                    current_key: None,
                    current_comment_before: None,
                    current_comment_inline: None,
                }));
            }

            Event::MappingEnd => {
                if let Some(Frame::Mapping(mf)) = self.stack.pop() {
                    self.push_node(YamlNode::Mapping(mf.mapping));
                }
            }

            Event::SequenceStart(_, _) => {
                let is_seq_parent = matches!(self.stack.last(), Some(Frame::Sequence(_)));
                if is_seq_parent {
                    let before = self.take_before(mark.line());
                    if let Some(Frame::Sequence(sf)) = self.stack.last_mut() {
                        sf.current_comment_before = before;
                    }
                }
                self.stack.push(Frame::Sequence(SequenceFrame {
                    seq: YamlSequence::new(),
                    current_comment_before: None,
                }));
            }

            Event::SequenceEnd => {
                if let Some(Frame::Sequence(sf)) = self.stack.pop() {
                    self.push_node(YamlNode::Sequence(sf.seq));
                }
            }

            Event::Scalar(value, style, _, _) => {
                let typed = match style {
                    TScalarStyle::Plain => ScalarValue::from_str(&value),
                    // Quoted scalars are always strings — even an empty "" or '' is ""
                    // not null (quoting is explicit intent to represent a string value).
                    _ => ScalarValue::Str(value.clone()),
                };

                match self.stack.last_mut() {
                    None => {
                        self.docs
                            .push(YamlNode::Scalar(YamlScalar { value: typed }));
                        self.last_content_line = Some(mark.line());
                    }
                    Some(Frame::Mapping(mf)) => {
                        if mf.current_key.is_none() {
                            // This is a KEY scalar
                            let node_line = mark.line();
                            let comment_before = {
                                let before: Vec<String> = self
                                    .pending_before
                                    .drain(..)
                                    .filter(|(l, _)| *l < node_line)
                                    .map(|(_, t)| t)
                                    .collect();
                                if before.is_empty() {
                                    None
                                } else {
                                    Some(before.join("\n"))
                                }
                            };
                            mf.current_key = Some(value);
                            mf.current_comment_before = comment_before;
                            mf.current_comment_inline = None;
                            self.last_content_line = Some(mark.line());
                        } else {
                            // This is a VALUE scalar
                            let node = YamlNode::Scalar(YamlScalar { value: typed });
                            if let Some(key) = mf.current_key.take() {
                                let comment_before = mf.current_comment_before.take();
                                let comment_inline = mf.current_comment_inline.take();
                                mf.mapping.entries.insert(
                                    key,
                                    YamlEntry {
                                        value: node,
                                        comment_before,
                                        comment_inline,
                                    },
                                );
                            }
                            self.last_content_line = Some(mark.line());
                        }
                    }
                    Some(Frame::Sequence(sf)) => {
                        let node_line = mark.line();
                        let comment_before = {
                            let before: Vec<String> = self
                                .pending_before
                                .drain(..)
                                .filter(|(l, _)| *l < node_line)
                                .map(|(_, t)| t)
                                .collect();
                            if before.is_empty() {
                                None
                            } else {
                                Some(before.join("\n"))
                            }
                        };
                        sf.seq.items.push(YamlItem {
                            value: YamlNode::Scalar(YamlScalar { value: typed }),
                            comment_before,
                            comment_inline: None,
                        });
                        self.last_content_line = Some(mark.line());
                    }
                }
            }

            Event::Alias(_) => {
                self.push_node(YamlNode::Null);
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
pub fn parse_str(input: &str) -> Result<Vec<YamlNode>, String> {
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

    Ok(builder.docs)
}
