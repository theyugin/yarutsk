// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::sync::Arc;

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::convert::{
    ChildContainer, LoadCtx, carry_metadata, collect_opaque_children_from_mapping,
    collect_opaque_children_from_sequence, deep_clone_opaque, extract_yaml_node,
    for_each_opaque_child, map_child_node, mapping_repr, mapping_to_py_obj, mapping_to_python,
    materialise_node, node_to_py, plain_entry, py_to_stored_node, read_metadata,
};
use super::macros::container_metadata_pymethods;
use super::py_node::PyYamlNode;
use super::py_sequence::PyYamlSequence;
use super::schema::Schema;
use super::sort::sort_mapping;
use super::style_parse::parse_container_style;
use crate::core::types::{FormatOptions, MapKey, NodeMeta, YamlMapping, YamlNode};

/// A YAML mapping node. Standalone pyclass implementing the dict protocol
/// (`__getitem__`/`__setitem__`/`__iter__`/...).
///
/// Container children are stored as `YamlNode::Container(Py<PyYamlMapping>)` /
/// `Container(Py<PyYamlSequence>)` so `doc['a']` returns the same Py every time,
/// mutations propagate, and aliases share identity. Scalars convert to
/// Python primitives on each read.
///
/// **Note**: this class does NOT extend `dict`. `isinstance(m, dict)` is False.
/// Use `m.to_python()` for a plain `dict` (recursively).
#[pyclass(name = "YamlMapping", extends = PyYamlNode, from_py_object)]
#[derive(Clone)]
pub struct PyYamlMapping {
    pub(crate) inner: YamlMapping,
}

#[pymethods]
impl PyYamlMapping {
    #[new]
    #[pyo3(signature = (mapping = None, *, style = "block", tag = None, schema = None))]
    fn new(
        mapping: Option<&Bound<'_, PyAny>>,
        style: &str,
        tag: Option<&str>,
        schema: Option<Py<Schema>>,
    ) -> PyResult<(Self, PyYamlNode)> {
        let _ = (mapping, schema); // populated in __init__ once `slf` is available
        let mut inner = YamlMapping::new();
        inner.style = parse_container_style(style)?;
        inner.meta.tag = tag.map(str::to_owned);
        Ok((PyYamlMapping { inner }, PyYamlNode::default()))
    }

    /// Populate from `mapping` once the pyclass exists. Splitting `__new__`
    /// (which builds an empty shell) from `__init__` (which fills it) lets the
    /// schema-aware path return the materialised tree from `py_to_stored_node`
    /// straight into `inner`.
    #[pyo3(signature = (mapping = None, *, style = "block", tag = None, schema = None))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    fn __init__(
        slf: &Bound<'_, Self>,
        mapping: Option<&Bound<'_, PyAny>>,
        style: &str,
        tag: Option<&str>,
        schema: Option<Py<Schema>>,
    ) -> PyResult<()> {
        let _ = (style, tag); // already applied in __new__
        if let Some(m) = mapping {
            let py = slf.py();
            crate::py::schema::freeze_schema(py, schema.as_ref());
            let sb = schema.as_ref().map(|s| s.bind(py));
            // `extract_yaml_node` (not `py_to_node`) so self-referential
            // dicts round-trip via auto-anchor instead of erroring on the
            // cycle guard.
            let node = extract_yaml_node(m, sb.as_ref().copied())?;
            match node {
                YamlNode::Mapping(mut parsed) => {
                    // Materialise children — Mapping/Sequence become Container,
                    // Aliases (from auto-anchor) keep their structure for
                    // round-trip.
                    let mut ctx = LoadCtx::default();
                    for entry in parsed.entries.values_mut() {
                        materialise_node(py, &mut entry.value, sb.as_ref().copied(), &mut ctx)?;
                    }
                    let mut borrow = slf.borrow_mut();
                    let style = borrow.inner.style;
                    let tag = std::mem::take(&mut borrow.inner.meta.tag);
                    borrow.inner = parsed;
                    borrow.inner.style = style;
                    borrow.inner.meta.tag = tag;
                }
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "YamlMapping requires a dict or mapping-like object",
                    ));
                }
            }
        }
        Ok(())
    }

    fn __getitem__(slf: &Bound<'_, Self>, key: &str) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let mk = MapKey::scalar(key);
        let value: YamlNode = {
            let borrow = slf.borrow();
            match borrow.inner.entries.get(&mk) {
                Some(entry) => entry.value.clone(),
                None => return Err(PyKeyError::new_err(key.to_owned())),
            }
        };
        node_to_py(py, &value, None)
    }

    fn __setitem__(slf: &Bound<'_, Self>, key: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let mut node = py_to_stored_node(py, value, None)?;
        let mk = MapKey::scalar(key);
        // Read old metadata BEFORE taking the mut borrow so `read_metadata`
        // can briefly borrow the live child Py for `Container` slots.
        let old_meta = {
            let borrow = slf.borrow();
            borrow
                .inner
                .entries
                .get(&mk)
                .map(|e| read_metadata(&e.value))
        };
        let mut borrow = slf.borrow_mut();
        if let Some(entry) = borrow.inner.entries.get_mut(&mk) {
            if let Some((oi, ob, obl)) = old_meta {
                carry_metadata(&mut node, oi, ob, obl);
            }
            entry.value = node;
        } else {
            borrow.inner.entries.insert(mk, plain_entry(node));
        }
        Ok(())
    }

    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        match self.inner.entries.shift_remove(&MapKey::scalar(key)) {
            Some(_) => Ok(()),
            None => Err(PyKeyError::new_err(key.to_owned())),
        }
    }

    /// `key in m` — string keys probe `inner.entries`; non-string keys
    /// always return `False` (matches `dict` for incompatible key types).
    fn __contains__(&self, key: &Bound<'_, PyAny>) -> bool {
        match key.extract::<String>() {
            Ok(k) => self.inner.entries.contains_key(&MapKey::scalar(k)),
            Err(_) => false,
        }
    }

    fn __len__(&self) -> usize {
        self.inner.entries.len()
    }

    /// Iterate over keys (matches `dict.__iter__` semantics).
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys: Vec<String> = self.inner.entries.keys().map(MapKey::python_key).collect();
        Ok(PyList::new(py, keys)?.try_iter()?.into_any().unbind())
    }

    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let our = slf.borrow().to_python(py)?;
        our.bind(py).eq(other)
    }

    fn __ne__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::__eq__(slf, other).map(|b| !b)
    }

    /// `m | other` — return a new `dict` with our entries overlaid by *other*'s
    /// (PEP 584). Returns a plain `dict`. Accepts a yarutsk mapping or plain
    /// dict on the right.
    fn __or__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let lhs = self.to_python(py)?;
        let rhs: Py<PyAny> = if let Ok(m) = other.extract::<PyYamlMapping>() {
            m.to_python(py)?
        } else if other.is_instance_of::<PyDict>() {
            other.clone().unbind()
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "unsupported operand for |: expected a mapping",
            ));
        };
        lhs.bind(py)
            .call_method1("__or__", (rhs.bind(py),))
            .map(pyo3::Bound::unbind)
    }

    /// `m |= other` — in-place update (PEP 584).
    fn __ior__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::update(slf, other)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        mapping_repr(py, &self.inner)
    }

    /// Pickle support: round-trip via `to_python()`. Metadata (style, tag,
    /// comments) is *not* preserved across pickle — pickling is for value
    /// shape only. For round-trip with metadata, use `dumps` / `loads`.
    fn __reduce__(&self, py: Python<'_>) -> PyResult<(Py<PyAny>, (Py<PyAny>,))> {
        let cls = py.get_type::<PyYamlMapping>().into_any().unbind();
        let dict_form = self.to_python(py)?;
        Ok((cls, (dict_form,)))
    }

    /// Return a list of keys (snapshot — does not reflect later mutations).
    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys: Vec<String> = self.inner.entries.keys().map(MapKey::python_key).collect();
        Ok(PyList::new(py, keys)?.into_any().unbind())
    }

    /// Return a list of values (snapshot).
    fn values(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let vals: Vec<Py<PyAny>> = {
            let borrow = slf.borrow();
            borrow
                .inner
                .entries
                .values()
                .map(|e| node_to_py(py, &e.value, None))
                .collect::<PyResult<_>>()?
        };
        Ok(PyList::new(py, vals)?.into_any().unbind())
    }

    /// Return a list of (key, value) tuples (snapshot).
    fn items(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let pairs: Vec<(String, Py<PyAny>)> = {
            let borrow = slf.borrow();
            borrow
                .inner
                .entries
                .iter()
                .map(|(k, e)| Ok((k.python_key(), node_to_py(py, &e.value, None)?)))
                .collect::<PyResult<_>>()?
        };
        Ok(PyList::new(py, pairs)?.into_any().unbind())
    }

    #[pyo3(signature = (key, default=None))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    fn get(slf: &Bound<'_, Self>, key: &str, default: Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let value: Option<YamlNode> = {
            let borrow = slf.borrow();
            borrow
                .inner
                .entries
                .get(&MapKey::scalar(key))
                .map(|e| e.value.clone())
        };
        match value {
            Some(v) => node_to_py(py, &v, None),
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    #[pyo3(signature = (key, default=None))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    fn pop(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let entry = slf
            .borrow_mut()
            .inner
            .entries
            .shift_remove(&MapKey::scalar(key));
        match entry {
            Some(e) => node_to_py(py, &e.value, None),
            None => match default {
                Some(d) => Ok(d),
                None => Err(PyKeyError::new_err(key.to_owned())),
            },
        }
    }

    fn popitem(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
        let last = slf
            .borrow_mut()
            .inner
            .entries
            .pop()
            .ok_or_else(|| PyKeyError::new_err("dictionary is empty"))?;
        let (k, e) = last;
        let py_val = node_to_py(py, &e.value, None)?;
        Ok((k.python_key(), py_val))
    }

    fn clear(&mut self) {
        self.inner.entries.clear();
    }

    /// Update from another mapping or iterable of `(key, value)` pairs.
    fn update(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        // Fast-path: another PyYamlMapping — clone its entries directly.
        if let Ok(m) = other.extract::<PyYamlMapping>() {
            let mut borrow = slf.borrow_mut();
            for (k, e) in &m.inner.entries {
                borrow.inner.entries.insert(k.clone(), e.clone());
            }
            return Ok(());
        }
        // Mapping protocol: keys() + __getitem__.
        if other.hasattr("keys")? {
            let keys = other.call_method0("keys")?;
            for key in keys.try_iter()? {
                let key = key?;
                let val = other.get_item(&key)?;
                let k: String = key.extract()?;
                let node = py_to_stored_node(py, &val, None)?;
                slf.borrow_mut()
                    .inner
                    .entries
                    .insert(MapKey::Scalar(k), plain_entry(node));
            }
            return Ok(());
        }
        // Iterable of (key, value) pairs.
        for item in other.try_iter()? {
            let item = item?;
            let (k, val): (String, Bound<'_, PyAny>) = item.extract()?;
            let node = py_to_stored_node(py, &val, None)?;
            slf.borrow_mut()
                .inner
                .entries
                .insert(MapKey::Scalar(k), plain_entry(node));
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    fn setdefault(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let mk = MapKey::scalar(key);
        let existing: Option<YamlNode> = {
            let borrow = slf.borrow();
            borrow.inner.entries.get(&mk).map(|e| e.value.clone())
        };
        if let Some(value) = existing {
            return node_to_py(py, &value, None);
        }
        let default_val = default.unwrap_or_else(|| py.None());
        let dv = default_val.bind(py);
        let node = py_to_stored_node(py, dv, None)?;
        slf.borrow_mut()
            .inner
            .entries
            .insert(mk, plain_entry(node.clone()));
        node_to_py(py, &node, None)
    }

    /// Sort mapping keys in-place.
    ///
    /// When *recursive* is ``True``, every nested ``YamlMapping`` (including
    /// those reached through nested ``YamlSequence`` items) has its keys
    /// sorted with the same *key* / *reverse* arguments. Sequence item order
    /// is **not** changed — ``sort_keys`` only sorts mapping keys.
    #[pyo3(signature = (key=None, reverse=false, recursive=false))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    pub fn sort_keys(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: Option<Py<PyAny>>,
        reverse: bool,
        recursive: bool,
    ) -> PyResult<()> {
        sort_mapping(
            py,
            &mut slf.borrow_mut().inner,
            key.as_ref(),
            reverse,
            recursive,
        )?;
        // `sort_mapping` walks the shared `inner` tree but doesn't touch the
        // live `PyYamlMapping`/`PyYamlSequence` Pys stored in `Container` slots —
        // descend into them so their own keys get sorted too.
        if recursive {
            let children = collect_opaque_children_from_mapping(&slf.borrow().inner, py);
            for_each_opaque_child(py, children, |child| match child {
                ChildContainer::Mapping(m) => PyYamlMapping::sort_keys(
                    m,
                    py,
                    key.as_ref().map(|k| k.clone_ref(py)),
                    reverse,
                    true,
                ),
                ChildContainer::Sequence(s) => {
                    descend_seq_for_sort_keys(s, py, key.as_ref(), reverse)
                }
            })?;
        }
        Ok(())
    }

    fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        mapping_to_python(py, &self.inner)
    }

    /// Return the YAML alias name if the value at *key* is an alias (``*name``), else ``None``.
    /// Raises ``KeyError`` if *key* is absent.
    fn get_alias(&self, key: &str) -> PyResult<Option<&str>> {
        match self.inner.entries.get(&MapKey::scalar(key)) {
            Some(entry) => Ok(match &entry.value {
                YamlNode::Alias { name, .. } => Some(name.as_str()),
                _ => None,
            }),
            None => Err(PyKeyError::new_err(key.to_owned())),
        }
    }

    /// Mark the value at *key* as a YAML alias that emits ``*anchor_name``.
    /// The current value is kept as the resolved node so Python reads still work.
    /// Raises ``KeyError`` if *key* is absent.
    fn set_alias(&mut self, key: &str, anchor_name: &str) -> PyResult<()> {
        let entry = self
            .inner
            .entries
            .get_mut(&MapKey::scalar(key))
            .ok_or_else(|| PyKeyError::new_err(key.to_owned()))?;
        let resolved = Arc::new(entry.value.clone());
        entry.value = YamlNode::Alias {
            name: anchor_name.to_owned(),
            resolved,
            meta: NodeMeta::default(),
            materialised: None,
        };
        Ok(())
    }

    /// Return the underlying YAML node for a key as a `YamlScalar`, `YamlMapping`,
    /// or `YamlSequence` object, preserving style/tag metadata.
    ///
    /// Mutations on the returned object propagate back into this mapping: for
    /// container children the returned object is the live child (identical to
    /// `m[key]`), and for scalar children it is a fresh `YamlScalar` whose
    /// setters write through into this mapping's `inner`.
    ///
    /// Raises `KeyError` if the key is absent.
    fn node(slf: &Bound<'_, Self>, key: &str) -> PyResult<Py<PyAny>> {
        map_child_node(slf, key)
    }

    /// Strip cosmetic formatting metadata, resetting to clean YAML defaults.
    ///
    /// All three keyword flags default to ``True``:
    ///
    /// - ``styles``: scalar quoting → plain (or ``literal`` for multi-line strings),
    ///   container style → block, scalar ``original`` values cleared.
    /// - ``comments``: ``comment_before`` and ``comment_inline`` cleared on all entries.
    /// - ``blank_lines``: ``blank_lines_before`` zeroed on all entries;
    ///   ``trailing_blank_lines`` zeroed on this container.
    ///
    /// Tags, anchors, and document-level markers are always preserved.
    /// Recurses into all nested containers.
    #[pyo3(signature = (*, styles=true, comments=true, blank_lines=true))]
    pub fn format(
        slf: &Bound<'_, Self>,
        styles: bool,
        comments: bool,
        blank_lines: bool,
    ) -> PyResult<()> {
        let opts = FormatOptions {
            styles,
            comments,
            blank_lines,
        };
        slf.borrow_mut().inner.format_with(opts);
        let py = slf.py();
        let children = collect_opaque_children_from_mapping(&slf.borrow().inner, py);
        for_each_opaque_child(py, children, |child| match child {
            ChildContainer::Mapping(m) => PyYamlMapping::format(m, styles, comments, blank_lines),
            ChildContainer::Sequence(s) => PyYamlSequence::format(s, styles, comments, blank_lines),
        })
    }

    /// Return a list of ``(key, node)`` pairs for all entries in this mapping.
    ///
    /// Each node is a ``YamlMapping``, ``YamlSequence``, or ``YamlScalar``,
    /// preserving style/tag metadata. Mutations on the returned nodes propagate
    /// back into this mapping — same semantics as ``node(key)``.
    fn nodes(slf: &Bound<'_, Self>) -> PyResult<Vec<(String, Py<PyAny>)>> {
        let keys: Vec<String> = slf
            .borrow()
            .inner
            .entries
            .keys()
            .map(MapKey::python_key)
            .collect();
        keys.into_iter()
            .map(|k| {
                let node = map_child_node(slf, &k)?;
                Ok((k, node))
            })
            .collect()
    }

    /// Return a shallow copy of this mapping (style metadata cloned; container
    /// children share Py identity with the original — same semantics as
    /// `dict.copy()`).
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 receivers are by-value
    fn copy(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Self::__copy__(slf, py)
    }

    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 receivers are by-value
    fn __copy__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // `Container(Py<…>)` slots clone via Py refcount, so child containers are
        // shared with the source — same as `dict.copy()`.
        let meta = slf.as_super().doc_metadata().clone();
        mapping_to_py_obj(py, slf.inner.clone(), meta, None)
    }

    /// Deep copy: recursively reconstructs every nested container as a fresh
    /// independent Py. Mutations on the deep copy don't affect the original.
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 receivers are by-value
    fn __deepcopy__(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        _memo: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        deep_copy_mapping(&slf, py)
    }
}

container_metadata_pymethods!(PyYamlMapping);

/// Deep-copy a mapping. Free-function variant of `__deepcopy__` so it's
/// callable from Rust code (pymethods are not).
pub(crate) fn deep_copy_mapping(
    slf: &PyRef<'_, PyYamlMapping>,
    py: Python<'_>,
) -> PyResult<Py<PyAny>> {
    let mut cloned = slf.inner.clone();
    for entry in cloned.entries.values_mut() {
        deep_clone_opaque(py, &mut entry.value)?;
    }
    let meta = slf.as_super().doc_metadata().clone();
    mapping_to_py_obj(py, cloned, meta, None)
}

/// Walk a `YamlSequence`'s items, syncing the order of every nested
/// `PyYamlMapping`. The sequence itself is **not** reordered — `sort_keys`
/// only touches mapping keys.
pub(crate) fn descend_seq_for_sort_keys(
    seq: &Bound<'_, PyYamlSequence>,
    py: Python<'_>,
    key: Option<&Py<PyAny>>,
    reverse: bool,
) -> PyResult<()> {
    let children = collect_opaque_children_from_sequence(&seq.borrow().inner, py);
    for_each_opaque_child(py, children, |child| match child {
        ChildContainer::Mapping(m) => {
            PyYamlMapping::sort_keys(m, py, key.map(|k| k.clone_ref(py)), reverse, true)
        }
        ChildContainer::Sequence(s) => descend_seq_for_sort_keys(s, py, key, reverse),
    })
}
