//! Flat, stack-safe document representation used by the emission pipeline.
//!
//! Child containers and complex keys are referenced by [`NodeId`] rather than
//! owned recursively. This makes traversal and destruction independent of YAML
//! nesting depth. During the migration the parser-facing [`YamlNode`] model is
//! adapted into this arena iteratively; the builder will eventually allocate
//! these records directly.

use crate::core::builder::DocMetadata;
use crate::core::types::{ContainerStyle, MapKey, NodeMeta, ScalarStyle, YamlNode, YamlScalar};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone)]
pub struct ArenaEntry {
    pub key: MapKey,
    pub value: NodeId,
    pub key_style: ScalarStyle,
    pub key_anchor: Option<String>,
    pub key_alias: Option<String>,
    pub key_tag: Option<String>,
    pub key_node: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub enum ArenaNode {
    Mapping {
        entries: Vec<ArenaEntry>,
        style: ContainerStyle,
        trailing_blank_lines: u8,
        meta: NodeMeta,
    },
    Sequence {
        items: Vec<NodeId>,
        style: ContainerStyle,
        trailing_blank_lines: u8,
        meta: NodeMeta,
    },
    Scalar(YamlScalar),
    Alias {
        name: String,
        meta: NodeMeta,
    },
}

#[derive(Debug, Clone)]
pub struct ArenaDocument {
    pub nodes: Vec<ArenaNode>,
    pub root: NodeId,
    pub meta: DocMetadata,
}

impl ArenaDocument {
    #[must_use]
    pub fn node(&self, id: NodeId) -> &ArenaNode {
        &self.nodes[id.0]
    }

    /// Lower an existing recursive tree without using the call stack.
    #[must_use]
    #[allow(clippy::too_many_lines)] // Explicit-stack traversal keeps all frame transitions together.
    pub fn from_yaml(root: &YamlNode, meta: DocMetadata) -> Self {
        struct EntryTemplate {
            key: MapKey,
            key_style: ScalarStyle,
            key_anchor: Option<String>,
            key_alias: Option<String>,
            key_tag: Option<String>,
            has_key_node: bool,
        }

        enum Task<'a> {
            Visit(&'a YamlNode),
            FinishMapping {
                templates: Vec<EntryTemplate>,
                style: ContainerStyle,
                trailing_blank_lines: u8,
                meta: NodeMeta,
                child_count: usize,
            },
            FinishSequence {
                len: usize,
                style: ContainerStyle,
                trailing_blank_lines: u8,
                meta: NodeMeta,
            },
        }

        let mut tasks = vec![Task::Visit(root)];
        let mut completed = Vec::<NodeId>::new();
        let mut nodes = Vec::<ArenaNode>::new();

        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(YamlNode::Scalar(scalar)) => {
                    let id = NodeId(nodes.len());
                    nodes.push(ArenaNode::Scalar(scalar.clone()));
                    completed.push(id);
                }
                Task::Visit(YamlNode::Alias { name, meta, .. }) => {
                    let id = NodeId(nodes.len());
                    nodes.push(ArenaNode::Alias {
                        name: name.clone(),
                        meta: meta.clone(),
                    });
                    completed.push(id);
                }
                Task::Visit(YamlNode::Sequence(sequence)) => {
                    tasks.push(Task::FinishSequence {
                        len: sequence.items.len(),
                        style: sequence.style,
                        trailing_blank_lines: sequence.trailing_blank_lines,
                        meta: sequence.meta.clone(),
                    });
                    for item in sequence.items.iter().rev() {
                        tasks.push(Task::Visit(item));
                    }
                }
                Task::Visit(YamlNode::Mapping(mapping)) => {
                    let mut templates = Vec::with_capacity(mapping.entries.len());
                    let mut child_count = 0;
                    for (key, entry) in &mapping.entries {
                        templates.push(EntryTemplate {
                            key: key.clone(),
                            key_style: entry.key_style,
                            key_anchor: entry.key_anchor.clone(),
                            key_alias: entry.key_alias.clone(),
                            key_tag: entry.key_tag.clone(),
                            has_key_node: entry.key_node.is_some(),
                        });
                        child_count += 1 + usize::from(entry.key_node.is_some());
                    }
                    tasks.push(Task::FinishMapping {
                        templates,
                        style: mapping.style,
                        trailing_blank_lines: mapping.trailing_blank_lines,
                        meta: mapping.meta.clone(),
                        child_count,
                    });
                    for entry in mapping.entries.values().rev() {
                        tasks.push(Task::Visit(&entry.value));
                        if let Some(key_node) = entry.key_node.as_deref() {
                            tasks.push(Task::Visit(key_node));
                        }
                    }
                }
                Task::FinishSequence {
                    len,
                    style,
                    trailing_blank_lines,
                    meta,
                } => {
                    let start = completed.len() - len;
                    let items = completed.drain(start..).collect();
                    let id = NodeId(nodes.len());
                    nodes.push(ArenaNode::Sequence {
                        items,
                        style,
                        trailing_blank_lines,
                        meta,
                    });
                    completed.push(id);
                }
                Task::FinishMapping {
                    templates,
                    style,
                    trailing_blank_lines,
                    meta,
                    child_count,
                } => {
                    let start = completed.len() - child_count;
                    let drained: Vec<NodeId> = completed.drain(start..).collect();
                    let mut children = drained.into_iter();
                    let mut entries = Vec::with_capacity(templates.len());
                    for template in templates {
                        let key_node = template.has_key_node.then(|| {
                            children
                                .next()
                                .expect("complex key child must have been lowered")
                        });
                        let value = children
                            .next()
                            .expect("mapping value child must have been lowered");
                        entries.push(ArenaEntry {
                            key: template.key,
                            value,
                            key_style: template.key_style,
                            key_anchor: template.key_anchor,
                            key_alias: template.key_alias,
                            key_tag: template.key_tag,
                            key_node,
                        });
                    }
                    debug_assert!(children.next().is_none());
                    let id = NodeId(nodes.len());
                    nodes.push(ArenaNode::Mapping {
                        entries,
                        style,
                        trailing_blank_lines,
                        meta,
                    });
                    completed.push(id);
                }
            }
        }

        let root = completed
            .pop()
            .expect("every YAML document has one lowered root");
        debug_assert!(completed.is_empty());
        ArenaDocument { nodes, root, meta }
    }
}

impl ArenaNode {
    #[must_use]
    pub fn meta(&self) -> &NodeMeta {
        match self {
            ArenaNode::Mapping { meta, .. }
            | ArenaNode::Sequence { meta, .. }
            | ArenaNode::Alias { meta, .. } => meta,
            ArenaNode::Scalar(scalar) => &scalar.meta,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::builder::parse_str;
    use crate::core::emitter::{emit_arena_docs, emit_docs};

    #[test]
    fn lowers_nested_document_iteratively() {
        let parsed = parse_str("a:\n  - b: 1\n", None).unwrap();
        let arena = ArenaDocument::from_yaml(&parsed.docs[0], parsed.docs_meta[0].clone());
        let ArenaNode::Mapping { entries, .. } = arena.node(arena.root) else {
            panic!("expected mapping root");
        };
        let ArenaNode::Sequence { items, .. } = arena.node(entries[0].value) else {
            panic!("expected nested sequence");
        };
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn arena_emitter_matches_tree_emitter() {
        let cases = [
            "a: 1\nb:\n  - two\n  - three\n",
            "flow: {a: [1, 2], b: true}\n",
            "# before\na: value  # inline\n\nb: |\n  line one\n  line two\n",
            "root:\n  child:\n    grandchild: null\n",
            "anchor: &shared\n  value: 1\nalias: *shared\n",
            "- a: 1\n  b: 2\n- [x, y]\n",
        ];
        for source in cases {
            let parsed = parse_str(source, None).unwrap();
            let arenas: Vec<ArenaDocument> = parsed
                .docs
                .iter()
                .enumerate()
                .map(|(i, node)| {
                    ArenaDocument::from_yaml(
                        node,
                        parsed.docs_meta.get(i).cloned().unwrap_or_default(),
                    )
                })
                .collect();
            assert_eq!(
                emit_arena_docs(&arenas, 2),
                emit_docs(&parsed.docs, &parsed.docs_meta, 2),
                "arena mismatch for:\n{source}"
            );
        }
    }
}
