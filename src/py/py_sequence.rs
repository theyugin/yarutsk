// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::sync::Arc;

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PySlice};

use super::convert::{
    ChildContainer, LoadCtx, carry_metadata, collect_live_children_from_sequence, deep_clone_live,
    extract_yaml_node, for_each_live_child, live_sequence_to_py_obj, live_sequence_to_python,
    materialise_sequence, node_to_py, py_to_stored_node, read_metadata, resolve_seq_idx,
    seq_child_node, sequence_repr,
};
use super::live::LiveNode;
use super::macros::container_metadata_pymethods;
use super::py_mapping::PyYamlMapping;
use super::py_node::PyYamlNode;
use super::schema::Schema;
use super::sort::py_compare;
use super::style_parse::parse_container_style;
use crate::core::types::{FormatOptions, NodeMeta, YamlNode, YamlSequence};

/// A YAML sequence node. Standalone pyclass implementing the list protocol
/// (`__getitem__`/`__setitem__`/`__iter__`/...).
///
/// Container items are stored as `LiveNode::LivePy(Py<PyYamlMapping|PyYamlSequence>)`,
/// so `s[i]` returns the same Py every time and mutations propagate.
///
/// **Note**: this class does NOT extend `list`. `isinstance(s, list)` is False.
/// Use `s.to_python()` for a plain `list` (recursively).
#[pyclass(name = "YamlSequence", extends = PyYamlNode, from_py_object)]
#[derive(Clone)]
pub struct PyYamlSequence {
    pub(crate) inner: YamlSequence<LiveNode>,
}

#[pymethods]
impl PyYamlSequence {
    #[new]
    #[pyo3(signature = (iterable = None, *, style = "block", tag = None, schema = None))]
    fn new(
        iterable: Option<&Bound<'_, PyAny>>,
        style: &str,
        tag: Option<&str>,
        schema: Option<Py<Schema>>,
    ) -> PyResult<(Self, PyYamlNode)> {
        let _ = (iterable, schema); // populated in __init__
        let mut inner = YamlSequence::new();
        inner.style = parse_container_style(style)?;
        inner.meta.tag = tag.map(str::to_owned);
        Ok((PyYamlSequence { inner }, PyYamlNode::default()))
    }

    #[pyo3(signature = (iterable = None, *, style = "block", tag = None, schema = None))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    fn __init__(
        slf: &Bound<'_, Self>,
        iterable: Option<&Bound<'_, PyAny>>,
        style: &str,
        tag: Option<&str>,
        schema: Option<Py<Schema>>,
    ) -> PyResult<()> {
        let _ = (style, tag); // already applied in __new__
        if let Some(it) = iterable {
            let py = slf.py();
            crate::py::schema::freeze_schema(py, schema.as_ref());
            let sb = schema.as_ref().map(|s| s.bind(py));
            // `extract_yaml_node` (not `py_to_node`) so self-referential
            // lists round-trip via auto-anchor instead of erroring on the
            // cycle guard.
            let node = extract_yaml_node(it, sb.as_ref().copied())?;
            match node {
                YamlNode::Sequence(parsed) => {
                    let mut ctx = LoadCtx::default();
                    let mut live =
                        materialise_sequence(py, parsed, sb.as_ref().copied(), &mut ctx)?;
                    let mut borrow = slf.borrow_mut();
                    let style = borrow.inner.style;
                    let tag = std::mem::take(&mut borrow.inner.meta.tag);
                    live.style = style;
                    live.meta.tag = tag;
                    borrow.inner = live;
                }
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "YamlSequence requires a list or iterable object",
                    ));
                }
            }
        }
        Ok(())
    }

    /// `s[i]` — integer index returns the live Py at that position; slice
    /// returns a plain `list`.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn __getitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if let Ok(idx) = key.extract::<isize>() {
            let real = resolve_seq_idx(idx, slf.borrow().inner.items.len())?;
            let value: LiveNode = slf.borrow().inner.items[real].clone();
            return node_to_py(py, &value, None);
        }
        if let Ok(slice) = key.cast::<PySlice>() {
            let len = slf.borrow().inner.items.len() as isize;
            let indices = slice.indices(len)?;
            let mut out: Vec<Py<PyAny>> = Vec::new();
            let begin = indices.start;
            let end = indices.stop;
            let step = indices.step;
            if step == 0 {
                return Err(PyValueError::new_err("slice step cannot be zero"));
            }
            let mut i = begin;
            while (step > 0 && i < end) || (step < 0 && i > end) {
                if i >= 0 && (i as usize) < slf.borrow().inner.items.len() {
                    let value = slf.borrow().inner.items[i as usize].clone();
                    out.push(node_to_py(py, &value, None)?);
                }
                i += step;
            }
            return Ok(PyList::new(py, out)?.into_any().unbind());
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "sequence indices must be integers or slices",
        ))
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn __setitem__(
        slf: &Bound<'_, Self>,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        if let Ok(idx) = key.extract::<isize>() {
            let real = resolve_seq_idx(idx, slf.borrow().inner.items.len())?;
            let mut node = py_to_stored_node(py, value, None)?;
            // Read old metadata before taking the mut borrow (for Container
            // containers `read_metadata` briefly borrows the wrapped Py).
            let (oi, ob, obl) = read_metadata(&slf.borrow().inner.items[real]);
            let mut borrow = slf.borrow_mut();
            carry_metadata(&mut node, oi, ob, obl);
            borrow.inner.items[real] = node;
            return Ok(());
        }
        if let Ok(slice) = key.cast::<PySlice>() {
            let len = slf.borrow().inner.items.len() as isize;
            let indices = slice.indices(len)?;
            if indices.step != 1 {
                return Err(pyo3::exceptions::PyNotImplementedError::new_err(
                    "yarutsk sequences do not support extended slice assignment (step != 1)",
                ));
            }
            let start = indices.start as usize;
            let stop = indices.stop as usize;
            let mut new_items: Vec<LiveNode> = Vec::new();
            for py_item in value.try_iter()? {
                let py_item = py_item?;
                new_items.push(py_to_stored_node(py, &py_item, None)?);
            }
            let mut borrow = slf.borrow_mut();
            borrow.inner.items.drain(start..stop);
            for (i, item) in new_items.into_iter().enumerate() {
                borrow.inner.items.insert(start + i, item);
            }
            return Ok(());
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "sequence indices must be integers or slices",
        ))
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn __delitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(idx) = key.extract::<isize>() {
            let real = resolve_seq_idx(idx, slf.borrow().inner.items.len())?;
            slf.borrow_mut().inner.items.remove(real);
            return Ok(());
        }
        if let Ok(slice) = key.cast::<PySlice>() {
            let len = slf.borrow().inner.items.len() as isize;
            let indices = slice.indices(len)?;
            if indices.step != 1 {
                return Err(pyo3::exceptions::PyNotImplementedError::new_err(
                    "yarutsk sequences do not support extended slice deletion (step != 1)",
                ));
            }
            let start = indices.start as usize;
            let stop = indices.stop as usize;
            slf.borrow_mut().inner.items.drain(start..stop);
            return Ok(());
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "sequence indices must be integers or slices",
        ))
    }

    fn __len__(&self) -> usize {
        self.inner.items.len()
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Snapshot — same semantics as `list.__iter__` (no live mutation reflection).
        let items: Vec<Py<PyAny>> = self
            .inner
            .items
            .iter()
            .map(|n| node_to_py(py, n, None))
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, items)?.try_iter()?.into_any().unbind())
    }

    fn __contains__(&self, value: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        for item in &self.inner.items {
            let v = node_to_py(py, item, None)?;
            if v.bind(py).eq(value)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let our = slf.borrow().to_python(py)?;
        our.bind(py).eq(other)
    }

    fn __ne__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::__eq__(slf, other).map(|b| !b)
    }

    /// Comparison delegates to the plain-list view so `sort()` (which compares
    /// items lexicographically) works for nested `YamlSequence` items.
    fn __lt__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        self.to_python(py)?.bind(py).lt(other)
    }
    fn __le__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        self.to_python(py)?.bind(py).le(other)
    }
    fn __gt__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        self.to_python(py)?.bind(py).gt(other)
    }
    fn __ge__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        self.to_python(py)?.bind(py).ge(other)
    }

    /// `s + other` — concatenation. Returns a plain `list`.
    fn __add__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let lhs = self.to_python(py)?;
        lhs.bind(py)
            .call_method1("__add__", (other,))
            .map(pyo3::Bound::unbind)
    }

    fn __iadd__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::extend(slf, other)
    }

    fn __mul__(&self, n: isize, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let lhs = self.to_python(py)?;
        lhs.bind(py)
            .call_method1("__mul__", (n,))
            .map(pyo3::Bound::unbind)
    }

    fn __rmul__(&self, n: isize, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.__mul__(n, py)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        sequence_repr(py, &self.inner)
    }

    /// Pickle support: round-trip via `to_python()`. Metadata is *not*
    /// preserved across pickle — for that, use `dumps` / `loads`.
    fn __reduce__(&self, py: Python<'_>) -> PyResult<(Py<PyAny>, (Py<PyAny>,))> {
        let cls = py.get_type::<PyYamlSequence>().into_any().unbind();
        let list_form = self.to_python(py)?;
        Ok((cls, (list_form,)))
    }

    fn append(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let node = py_to_stored_node(py, value, None)?;
        slf.borrow_mut().inner.items.push(node);
        Ok(())
    }

    fn extend(slf: &Bound<'_, Self>, iterable: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        // Fast-path: another PyYamlSequence — clone its items directly so item
        // metadata (comments, blank lines) is preserved.
        if let Ok(other_seq) = iterable.extract::<PyYamlSequence>() {
            let items: Vec<LiveNode> = other_seq.inner.items.clone();
            slf.borrow_mut().inner.items.extend(items);
            return Ok(());
        }
        for py_item in iterable.try_iter()? {
            let py_item = py_item?;
            let node = py_to_stored_node(py, &py_item, None)?;
            slf.borrow_mut().inner.items.push(node);
        }
        Ok(())
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn insert(slf: &Bound<'_, Self>, idx: isize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let node = py_to_stored_node(py, value, None)?;
        let real = {
            let borrow = slf.borrow();
            let len = borrow.inner.items.len() as isize;
            if idx < 0 {
                (len + idx).max(0) as usize
            } else {
                idx.min(len) as usize
            }
        };
        slf.borrow_mut().inner.items.insert(real, node);
        Ok(())
    }

    #[pyo3(signature = (idx=-1))]
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn pop(slf: &Bound<'_, Self>, py: Python<'_>, idx: isize) -> PyResult<Py<PyAny>> {
        let mut borrow = slf.borrow_mut();
        let len = borrow.inner.items.len() as isize;
        if len == 0 {
            return Err(PyIndexError::new_err("pop from empty list"));
        }
        let real = if idx < 0 { len + idx } else { idx };
        if real < 0 || real >= len {
            return Err(PyIndexError::new_err("pop index out of range"));
        }
        let node = borrow.inner.items.remove(real as usize);
        node_to_py(py, &node, None)
    }

    fn remove(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let idx = {
            let borrow = slf.borrow();
            let mut found = None;
            for (i, item) in borrow.inner.items.iter().enumerate() {
                let v = node_to_py(py, item, None)?;
                if v.bind(py).eq(value)? {
                    found = Some(i);
                    break;
                }
            }
            found.ok_or_else(|| PyValueError::new_err("value not in list"))?
        };
        slf.borrow_mut().inner.items.remove(idx);
        Ok(())
    }

    fn clear(&mut self) {
        self.inner.items.clear();
    }

    fn count(&self, value: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<usize> {
        let mut n = 0usize;
        for item in &self.inner.items {
            let v = node_to_py(py, item, None)?;
            if v.bind(py).eq(value)? {
                n += 1;
            }
        }
        Ok(n)
    }

    #[pyo3(signature = (value, start=None, stop=None))]
    fn index(
        &self,
        value: &Bound<'_, PyAny>,
        start: Option<isize>,
        stop: Option<isize>,
        py: Python<'_>,
    ) -> PyResult<usize> {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let len = self.inner.items.len() as isize;
        let s = start.unwrap_or(0).clamp(0, len);
        let e = stop.unwrap_or(len).clamp(0, len);
        for i in s..e {
            #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
            let item = &self.inner.items[i as usize];
            let v = node_to_py(py, item, None)?;
            if v.bind(py).eq(value)? {
                #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
                return Ok(i as usize);
            }
        }
        Err(PyValueError::new_err("value not in list"))
    }

    fn reverse(slf: &Bound<'_, Self>) {
        slf.borrow_mut().inner.items.reverse();
    }

    #[pyo3(signature = (key=None, reverse=false, recursive=false))]
    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 requires Option<Py<T>> by value
    fn sort(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: Option<Py<PyAny>>,
        reverse: bool,
        recursive: bool,
    ) -> PyResult<()> {
        // Build (sort_key, item) pairs from our items.
        let n = slf.borrow().inner.items.len();
        let mut zipped: Vec<(Py<PyAny>, LiveNode)> = {
            let borrow = slf.borrow();
            (0..n)
                .map(|i| {
                    let item = borrow.inner.items[i].clone();
                    let py_obj = node_to_py(py, &item, None)?;
                    let sort_key = if let Some(key_fn) = &key {
                        key_fn
                            .bind(py)
                            .call1((py_obj.bind(py),))
                            .map(pyo3::Bound::unbind)?
                    } else {
                        py_obj
                    };
                    Ok((sort_key, item))
                })
                .collect::<PyResult<_>>()?
        };
        let mut err: Option<PyErr> = None;
        zipped.sort_by(|(ka, _), (kb, _)| {
            if err.is_some() {
                return std::cmp::Ordering::Equal;
            }
            py_compare(ka.bind(py), kb.bind(py), &mut err)
        });
        if let Some(e) = err {
            return Err(e);
        }
        if reverse {
            zipped.reverse();
        }
        slf.borrow_mut().inner.items = zipped.into_iter().map(|(_, item)| item).collect();
        if recursive {
            let children = collect_live_children_from_sequence(&slf.borrow().inner, py);
            for_each_live_child(py, children, |child| match child {
                ChildContainer::Mapping(m) => PyYamlMapping::sort_keys(m, py, None, reverse, true),
                ChildContainer::Sequence(s) => PyYamlSequence::sort(
                    s,
                    py,
                    key.as_ref().map(|k| k.clone_ref(py)),
                    reverse,
                    true,
                ),
                ChildContainer::Scalar(_) => Ok(()),
            })?;
        }
        Ok(())
    }

    #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 receivers are by-value
    fn __deepcopy__(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        _memo: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        deep_copy_sequence(&slf, py)
    }

    fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        live_sequence_to_python(py, &self.inner)
    }

    /// Return the YAML alias name if the item at *idx* is an alias (``*name``), else ``None``.
    /// Raises ``IndexError`` for out-of-range indices.
    fn get_alias(&self, idx: isize) -> PyResult<Option<&str>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        Ok(match &self.inner.items[i] {
            LiveNode::Alias { name, .. } => Some(name.as_str()),
            _ => None,
        })
    }

    /// Mark the item at *idx* as a YAML alias that emits ``*anchor_name``.
    /// The current value is kept as the resolved node so Python reads still work.
    /// Raises ``IndexError`` for out-of-range indices.
    fn set_alias(&mut self, py: Python<'_>, idx: isize, anchor_name: &str) -> PyResult<()> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        let resolved = Arc::new(crate::py::convert::live_to_yamlnode(
            py,
            &self.inner.items[i],
        )?);
        self.inner.items[i] = LiveNode::Alias {
            name: anchor_name.to_owned(),
            resolved,
            materialised: None,
            meta: NodeMeta::default(),
        };
        Ok(())
    }

    /// Return the underlying YAML node for the item at *idx* as a `YamlScalar`,
    /// `YamlMapping`, or `YamlSequence` object, preserving style/tag metadata.
    ///
    /// Mutations on the returned object propagate back into this sequence: for
    /// container items the returned object is the live child (identical to
    /// `s[idx]`), and for scalar items it is a fresh `YamlScalar` whose
    /// setters write through into this sequence's `inner`.
    ///
    /// Raises ``IndexError`` for out-of-range indices.
    fn node(slf: &Bound<'_, Self>, idx: isize) -> PyResult<Py<PyAny>> {
        let i = resolve_seq_idx(idx, slf.borrow().inner.items.len())?;
        seq_child_node(slf, i)
    }

    /// Return a list of underlying YAML nodes for every item in this sequence.
    fn nodes(slf: &Bound<'_, Self>) -> PyResult<Vec<Py<PyAny>>> {
        let n = slf.borrow().inner.items.len();
        (0..n).map(|i| seq_child_node(slf, i)).collect()
    }

    /// Strip cosmetic formatting metadata, resetting to clean YAML defaults.
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
        let children = collect_live_children_from_sequence(&slf.borrow().inner, py);
        for_each_live_child(py, children, |child| match child {
            ChildContainer::Mapping(m) => PyYamlMapping::format(m, styles, comments, blank_lines),
            ChildContainer::Sequence(s) => PyYamlSequence::format(s, styles, comments, blank_lines),
            ChildContainer::Scalar(sc) => {
                sc.borrow_mut().inner.format_with(opts);
                Ok(())
            }
        })
    }
}

container_metadata_pymethods!(PyYamlSequence, live_sequence_to_py_obj);

/// Deep-copy a sequence. Free-function variant of `__deepcopy__` so it's
/// callable from Rust code (pymethods are not).
pub(crate) fn deep_copy_sequence(
    slf: &PyRef<'_, PyYamlSequence>,
    py: Python<'_>,
) -> PyResult<Py<PyAny>> {
    let mut cloned = slf.inner.clone();
    for item in &mut cloned.items {
        deep_clone_live(py, item)?;
    }
    let meta = slf.as_super().doc_metadata().clone();
    live_sequence_to_py_obj(py, cloned, meta)
}
