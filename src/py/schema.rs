// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::collections::{HashMap, HashSet};

use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::core::builder::TagPolicy;

/// Tags for which the builder skips coercion when a loader is registered.
/// These are the tags whose `ScalarValue` is determined by the builder, so a
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
/// `Schema` is independent and can be reused across calls. Schemas are
/// frozen the first time they are bound to a load/dump call; afterwards
/// `add_loader`/`add_dumper` raise `RuntimeError`. Construct a fresh
/// `Schema` (or pass everything through the constructor kwargs) if you
/// need different registrations for a later call.
#[pyclass]
#[derive(Default)]
pub struct Schema {
    /// tag → callable(value) → Py<PyAny>  (load side)
    pub(crate) loaders: HashMap<String, Py<PyAny>>,
    /// ordered list of (type, callable(obj) → (tag, data))  (dump side)
    pub(crate) dumpers: Vec<(Py<PyAny>, Py<PyAny>)>,
    /// Tags for which the builder must skip `ScalarValue` coercion.
    pub(crate) raw_tags: HashSet<String>,
    /// Once set, further `add_loader` / `add_dumper` calls raise. Set on the
    /// first load/dump that binds the schema; concurrent loads sharing the
    /// same schema briefly contend on the pyclass mut-borrow during this
    /// one-time flip, which is fine in practice.
    pub(crate) frozen: bool,
}

#[pymethods]
impl Schema {
    /// Construct a `Schema`, optionally pre-populated with loaders and dumpers.
    ///
    /// *loaders* is a mapping `{tag: callable}`; *dumpers* is an iterable of
    /// `(type, callable)` pairs (insertion order is preserved, matching the
    /// `isinstance` dispatch order at dump time). After construction you can
    /// still call `add_loader`/`add_dumper` until the schema is bound to a
    /// load/dump call, after which it is frozen.
    #[new]
    #[pyo3(signature = (*, loaders = None, dumpers = None))]
    pub fn new(
        loaders: Option<&Bound<'_, PyAny>>,
        dumpers: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut schema = Schema::default();
        if let Some(map) = loaders {
            for item in map.call_method0("items")?.try_iter()? {
                let (tag, func): (String, Py<PyAny>) = item?.extract()?;
                schema.register_loader(&tag, func);
            }
        }
        if let Some(iter) = dumpers {
            for item in iter.try_iter()? {
                let (ty, func): (Py<PyAny>, Py<PyAny>) = item?.extract()?;
                schema.dumpers.push((ty, func));
            }
        }
        Ok(schema)
    }

    /// Register a loader for a YAML tag.
    ///
    /// *func* is called with the default-converted Python value during load:
    /// - For scalar nodes: a Python `str` (raw YAML text) when the tag is a
    ///   standard coercion tag (``!!int``, ``!!float``, ``!!bool``, ``!!null``,
    ///   ``!!str``); otherwise whatever type inference produced.
    /// - For mapping nodes: the ``YamlMapping``.
    /// - For sequence nodes: the ``YamlSequence``.
    ///
    /// The return value of *func* is used as the loaded Python object.
    /// Raises `RuntimeError` if the schema has already been used in a load
    /// or dump call.
    fn add_loader(&mut self, tag: &str, func: Py<PyAny>) -> PyResult<()> {
        self.check_unfrozen("add_loader")?;
        self.register_loader(tag, func);
        Ok(())
    }

    /// Register a dumper for a Python type.
    ///
    /// *func* is called with the Python object and must return a 2-tuple
    /// ``(tag: str, data)`` where *data* is any YAML-serialisable value
    /// (``str``, ``int``, ``float``, ``bool``, ``None``, ``dict``, or ``list``).
    ///
    /// Dumpers are checked in registration order; the first ``isinstance`` match
    /// wins, so register more specific types before base types. Raises
    /// `RuntimeError` if the schema has already been used in a load or dump
    /// call.
    fn add_dumper(&mut self, py_type: Bound<'_, PyType>, func: Py<PyAny>) -> PyResult<()> {
        self.check_unfrozen("add_dumper")?;
        self.dumpers.push((py_type.into_any().unbind(), func));
        Ok(())
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

    fn register_loader(&mut self, tag: &str, func: Py<PyAny>) {
        let normalized = normalize_tag(tag);
        if COERCION_TAGS.contains(&normalized.as_str()) {
            self.raw_tags.insert(normalized.clone());
        }
        self.loaders.insert(normalized, func);
    }

    fn check_unfrozen(&self, op: &str) -> PyResult<()> {
        if self.frozen {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Schema.{op}() called after the schema was used in a load/dump call; \
                 schemas are immutable once bound. Construct a fresh Schema \
                 (or pass loaders=/dumpers= to its constructor) for new registrations."
            )));
        }
        Ok(())
    }
}

/// Mark *schema* frozen so its loader/dumper sets cannot be mutated for the
/// remainder of the load/dump call. No-op if `schema` is `None` or already
/// frozen — the read-then-mut-borrow pattern avoids contending on the
/// pyclass mut-borrow when concurrent loads share a schema (the common case
/// is "all threads see frozen=true and skip the mut path").
pub(crate) fn freeze_schema(py: Python<'_>, schema: Option<&Py<Schema>>) {
    if let Some(s) = schema {
        let bound = s.bind(py);
        if bound.borrow().frozen {
            return;
        }
        bound.borrow_mut().frozen = true;
    }
}
