// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::collections::{HashMap, HashSet};

use pyo3::prelude::*;

use crate::core::builder::TagPolicy;

// ─── Schema ───────────────────────────────────────────────────────────────────

/// Tags for which the builder skips coercion when a loader is registered.
/// These are the tags whose ScalarValue is determined by the builder, so a
/// loader for them needs the raw string rather than a pre-converted value.
pub(crate) const COERCION_TAGS: &[&str] = &[
    "!!null",
    "tag:yaml.org,2002:null",
    "!!bool",
    "tag:yaml.org,2002:bool",
    "!!int",
    "tag:yaml.org,2002:int",
    "!!float",
    "tag:yaml.org,2002:float",
    "!!str",
    "tag:yaml.org,2002:str",
];

/// Expand the `!!` shorthand to the full YAML 1.1 secondary tag URI.
/// `"!!int"` → `"tag:yaml.org,2002:int"`.  Any other tag is returned unchanged.
/// This mirrors what the parser does internally, so user-registered `"!!int"`
/// tags match the `"tag:yaml.org,2002:int"` that the parser stores on scalars.
pub(crate) fn normalize_tag(tag: &str) -> String {
    if let Some(suffix) = tag.strip_prefix("!!") {
        format!("tag:yaml.org,2002:{suffix}")
    } else {
        tag.to_owned()
    }
}

/// Per-call registry of custom YAML ↔ Python type handlers.
///
/// Pass as `schema=` to any load/dump function. Has no global state — each
/// `Schema` is independent and can be reused across calls.
#[pyclass]
#[derive(Default)]
pub struct Schema {
    /// tag → callable(value) → Py<PyAny>  (load side)
    pub(crate) loaders: HashMap<String, Py<PyAny>>,
    /// ordered list of (type, callable(obj) → (tag, data))  (dump side)
    pub(crate) dumpers: Vec<(Py<PyAny>, Py<PyAny>)>,
    /// Tags for which the builder must skip ScalarValue coercion.
    pub(crate) raw_tags: HashSet<String>,
}

#[pymethods]
impl Schema {
    #[new]
    pub fn new() -> Self {
        Schema::default()
    }

    /// Register a loader for a YAML tag.
    ///
    /// *func* is called with the default-converted Python value during load:
    /// - For scalar nodes: a Python `str` (raw YAML text) when the tag is a
    ///   standard coercion tag (``!!int``, ``!!float``, ``!!bool``, ``!!null``,
    ///   ``!!str``); otherwise whatever type inference produced.
    /// - For mapping nodes: the ``YamlMapping`` (dict subclass).
    /// - For sequence nodes: the ``YamlSequence`` (list subclass).
    ///
    /// The return value of *func* is used as the loaded Python object.
    fn add_loader(&mut self, tag: String, func: Py<PyAny>) {
        let normalized = normalize_tag(&tag);
        if COERCION_TAGS.contains(&normalized.as_str()) {
            self.raw_tags.insert(normalized.clone());
        }
        self.loaders.insert(normalized, func);
    }

    /// Register a dumper for a Python type.
    ///
    /// *func* is called with the Python object and must return a 2-tuple
    /// ``(tag: str, data)`` where *data* is any YAML-serialisable value
    /// (``str``, ``int``, ``float``, ``bool``, ``None``, ``dict``, or ``list``).
    ///
    /// Dumpers are checked in registration order; the first ``isinstance`` match
    /// wins, so register more specific types before base types.
    fn add_dumper(&mut self, py_type: Py<PyAny>, func: Py<PyAny>) {
        self.dumpers.push((py_type, func));
    }
}

impl Schema {
    /// Derive a ``TagPolicy`` from the registered loaders.
    pub(crate) fn tag_policy(&self) -> Option<TagPolicy> {
        if self.raw_tags.is_empty() {
            None
        } else {
            Some(TagPolicy {
                raw_tags: self.raw_tags.clone(),
            })
        }
    }
}
