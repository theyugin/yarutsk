// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! `LiveNode` — the bridge slot type used inside pyclasses.
//!
//! [`crate::core::types::YamlNode`] is the pure data model that the
//! parser/builder produces and the emitter consumes. To preserve `is`-identity
//! across reads on the Python side, the pyclasses store *live* trees whose
//! entry/item slots can additionally hold a `Py` handle. Those Py-bearing
//! slots are this module's responsibility — keeping the pure core layer free
//! of any `pyo3` references.
//!
//! Pyclasses store their inner data as `YamlMapping<LiveNode>` /
//! `YamlSequence<LiveNode>`; the parser produces `YamlMapping<YamlNode>` and
//! the emitter consumes `YamlMapping<YamlNode>`. Conversion between the two
//! happens in `convert.rs` (`materialise_*` for load, `extract_*` for dump).

use std::sync::Arc;

use pyo3::prelude::*;

use crate::core::types::{FormatOptions, Node, NodeMeta, YamlNode, YamlScalar, meta_format_with};

/// Slot type for entries/items inside a live (pyclass-owned) tree.
///
/// `Scalar` and `Alias` are inline — held as data, no wrapping Py. `LivePy`
/// holds *any* Python value: a typed `PyYamlMapping` / `PyYamlSequence` /
/// `PyYamlScalar` (which preserves `is`-identity across reads), or an
/// arbitrary user-supplied object that the schema dumper will convert at
/// emit time. The distinction is made at access sites by a downcast.
#[derive(Debug)]
pub enum LiveNode {
    Scalar(YamlScalar),
    /// An alias node (`*name`). `materialised` caches the resolved Py so
    /// `doc[alias_key] is doc[anchor_key]` holds across separate accesses
    /// (`LoadCtx` is per-call, so it cannot bridge identity past load).
    /// The cache is populated at materialise-time from the anchor's already-
    /// realised Py and never goes stale: mutations on the anchored container
    /// happen through the same `Py<…>` the cache holds.
    Alias {
        name: String,
        resolved: Arc<YamlNode>,
        materialised: Option<Py<PyAny>>,
        meta: NodeMeta,
    },
    /// Any Python-side child (typed yarutsk pyclass or opaque user value).
    /// Whether it is a yarutsk pyclass (typed mapping/sequence/scalar with
    /// preserved identity) or an opaque value (extracted via schema/`py_to_node`
    /// at dump time) is decided by downcast at access sites.
    LivePy(Py<PyAny>),
}

impl Clone for LiveNode {
    fn clone(&self) -> Self {
        match self {
            LiveNode::Scalar(s) => LiveNode::Scalar(s.clone()),
            LiveNode::Alias {
                name,
                resolved,
                materialised,
                meta,
            } => LiveNode::Alias {
                name: name.clone(),
                resolved: resolved.clone(),
                materialised: materialised
                    .as_ref()
                    .map(|p| Python::attach(|py| p.clone_ref(py))),
                meta: meta.clone(),
            },
            LiveNode::LivePy(p) => LiveNode::LivePy(Python::attach(|py| p.clone_ref(py))),
        }
    }
}

impl LiveNode {
    /// `true` iff this slot is a scalar leaf that needs lazy promotion into
    /// `LivePy(Py<PyYamlScalar>)` on first `node()` access.
    #[must_use]
    pub fn is_scalar_child(&self) -> bool {
        match self {
            LiveNode::Scalar(_) => true,
            LiveNode::Alias { resolved, .. } => matches!(resolved.as_ref(), YamlNode::Scalar(_)),
            LiveNode::LivePy(_) => false,
        }
    }
}

impl Node for LiveNode {
    fn comment_inline(&self) -> Option<&str> {
        match self {
            LiveNode::Scalar(s) => s.meta.comment_inline.as_deref(),
            LiveNode::Alias { meta, .. } => meta.comment_inline.as_deref(),
            LiveNode::LivePy(_) => None,
        }
    }
    fn set_comment_inline(&mut self, value: Option<String>) {
        match self {
            LiveNode::Scalar(s) => s.meta.comment_inline = value,
            LiveNode::Alias { meta, .. } => meta.comment_inline = value,
            LiveNode::LivePy(_) => {}
        }
    }
    fn comment_before(&self) -> Option<&str> {
        match self {
            LiveNode::Scalar(s) => s.meta.comment_before.as_deref(),
            LiveNode::Alias { meta, .. } => meta.comment_before.as_deref(),
            LiveNode::LivePy(_) => None,
        }
    }
    fn set_comment_before(&mut self, value: Option<String>) {
        match self {
            LiveNode::Scalar(s) => s.meta.comment_before = value,
            LiveNode::Alias { meta, .. } => meta.comment_before = value,
            LiveNode::LivePy(_) => {}
        }
    }
    fn blank_lines_before(&self) -> u8 {
        match self {
            LiveNode::Scalar(s) => s.meta.blank_lines_before,
            LiveNode::Alias { meta, .. } => meta.blank_lines_before,
            LiveNode::LivePy(_) => 0,
        }
    }
    fn set_blank_lines_before(&mut self, value: u8) {
        match self {
            LiveNode::Scalar(s) => s.meta.blank_lines_before = value,
            LiveNode::Alias { meta, .. } => meta.blank_lines_before = value,
            LiveNode::LivePy(_) => {}
        }
    }
    fn format_with(&mut self, opts: FormatOptions) {
        match self {
            LiveNode::Scalar(s) => s.format_with(opts),
            LiveNode::Alias { meta, .. } => meta_format_with(meta, opts),
            // Pyclass children own their own state — recursion is driven by
            // the pyclass `format()` method walking children.
            LiveNode::LivePy(_) => {}
        }
    }
}
