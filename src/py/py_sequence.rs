// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use pyo3::prelude::*;
use pyo3::types::{PyList, PySlice, PyTuple};

use super::convert::{
    DocMeta, OverloadArg, node_to_doc, node_to_py, overload_arg, parse_container_style,
    parse_scalar_style, parse_yaml_version, plain_item, py_compare, py_to_node,
    py_to_node_with_fallback, resolve_seq_idx, sequence_repr, sequence_to_py_obj,
    sequence_to_python,
};
use super::py_mapping::PyYamlMapping;
use super::schema::Schema;
use crate::core::types::{
    ContainerStyle, FormatOptions, YamlItem, YamlMapping, YamlNode, YamlSequence,
};

// ─── PyYamlSequence (Python: YamlSequence extends list) ──────────────────────

/// A YAML sequence node. Subclass of list; the parent list is always kept in
/// sync with `inner` so that standard list operations work transparently.
#[pyclass(name = "YamlSequence", extends = PyList, from_py_object)]
#[derive(Clone)]
pub struct PyYamlSequence {
    pub(crate) inner: YamlSequence,
    /// True when the document this sequence belongs to had an explicit `---` marker.
    #[pyo3(get, set)]
    pub explicit_start: bool,
    /// True when the document this sequence belongs to had an explicit `...` marker.
    #[pyo3(get, set)]
    pub explicit_end: bool,
    /// `%YAML major.minor` directive for this document, if any.
    /// Exposed to Python as a `"major.minor"` string via manual getter/setter.
    pub yaml_version: Option<(u8, u8)>,
    /// `%TAG handle prefix` pairs for this document.
    #[pyo3(get, set)]
    pub tag_directives: Vec<(String, String)>,
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
    ) -> PyResult<Self> {
        let _ = (iterable, schema); // populated in __init__ once the parent list is available
        let mut inner = YamlSequence::new();
        inner.style = parse_container_style(style)?;
        inner.tag = tag.map(str::to_owned);
        Ok(PyYamlSequence {
            inner,
            explicit_start: false,
            explicit_end: false,
            yaml_version: None,
            tag_directives: vec![],
        })
    }

    // Intercept __init__ so that Python does not forward args to list.__init__,
    // which would otherwise try to iterate them. Populate from `iterable` here
    // because the parent list is accessible via slf.as_super() at this point.
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
            if let Some(ref schema_py) = schema {
                let py = slf.py();
                let sb = schema_py.bind(py);
                let node = py_to_node(it, Some(sb))?;
                if let YamlNode::Sequence(parsed) = node {
                    let list_part = slf.as_super();
                    let mut borrow = slf.borrow_mut();
                    let style = borrow.inner.style;
                    let tag = std::mem::take(&mut borrow.inner.tag);
                    borrow.inner = parsed;
                    borrow.inner.style = style;
                    borrow.inner.tag = tag;
                    drop(borrow);
                    let borrow = slf.borrow();
                    for item in &borrow.inner.items {
                        let py_val = node_to_py(py, &item.value, Some(sb))?;
                        list_part.append(py_val.bind(py))?;
                    }
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "YamlSequence requires a list or iterable object",
                    ));
                }
            } else {
                PyYamlSequence::extend(slf, it)?;
            }
        }
        Ok(())
    }

    // ── Mutations (must sync parent list) ────────────────────────────────────

    // Python slice indices are `isize` (`Py_ssize_t`); conversions to/from
    // `usize` here are bounded by `PySlice::indices`, which clamps to
    // `[0, len]`, or by explicit range checks above the cast.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn __setitem__(
        slf: &Bound<'_, Self>,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        if let Ok(idx) = key.extract::<isize>() {
            let real_idx = resolve_seq_idx(idx, slf.borrow().inner.items.len())?;
            let (node, py_val) =
                py_to_node_with_fallback(
                    py,
                    value,
                    None,
                    || YamlNode::Mapping(YamlMapping::new()),
                )?;
            {
                let mut borrow = slf.borrow_mut();
                borrow.inner.items[real_idx as usize].value = node;
            }
            slf.as_super()
                .set_item(real_idx as usize, py_val.bind(py))?;
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
            let mut new_items: Vec<YamlItem> = Vec::new();
            let mut new_py: Vec<Py<PyAny>> = Vec::new();
            for py_item in value.try_iter()? {
                let py_item = py_item?;
                let (node, py_val) = py_to_node_with_fallback(py, &py_item, None, || {
                    YamlNode::Mapping(YamlMapping::new())
                })?;
                new_items.push(plain_item(node));
                new_py.push(py_val);
            }
            {
                let mut borrow = slf.borrow_mut();
                borrow.inner.items.drain(start..stop);
                for (i, item) in new_items.into_iter().enumerate() {
                    borrow.inner.items.insert(start + i, item);
                }
            }
            let new_py_list = PyList::new(py, new_py.iter().map(|v| v.bind(py)))?;
            slf.as_super()
                .set_slice(start, stop, new_py_list.as_any())?;
            return Ok(());
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "sequence indices must be integers or slices",
        ))
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // see __setitem__
    fn __delitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(idx) = key.extract::<isize>() {
            let real_idx = resolve_seq_idx(idx, slf.borrow().inner.items.len())?;
            {
                let mut borrow = slf.borrow_mut();
                borrow.inner.items.remove(real_idx as usize);
            }
            // Use set_slice(i, i+1, []) instead of del_item(): del_item routes through
            // PySequence_DelItem which dispatches via sq_ass_item back to our __delitem__,
            // causing a recursive removal loop. set_slice calls PyList_SetSlice at C level,
            // bypassing the MRO entirely.
            let empty = PyList::empty(slf.py());
            slf.as_super()
                .set_slice(real_idx as usize, real_idx as usize + 1, empty.as_any())?;
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
            {
                let mut borrow = slf.borrow_mut();
                borrow.inner.items.drain(start..stop);
            }
            let empty = PyList::empty(slf.py());
            slf.as_super().set_slice(start, stop, empty.as_any())?;
            return Ok(());
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "sequence indices must be integers or slices",
        ))
    }

    fn append(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let (node, py_val) =
            py_to_node_with_fallback(py, value, None, || YamlNode::Mapping(YamlMapping::new()))?;
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items.push(plain_item(node));
        }
        slf.as_super().append(py_val.bind(py))?;
        Ok(())
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // see __setitem__
    fn insert(slf: &Bound<'_, Self>, idx: isize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let (node, py_val_insert) =
            py_to_node_with_fallback(py, value, None, || YamlNode::Mapping(YamlMapping::new()))?;
        let real_idx = {
            let borrow = slf.borrow();
            let len = borrow.inner.items.len() as isize;
            if idx < 0 {
                (len + idx).max(0) as usize
            } else {
                idx.min(len) as usize
            }
        };
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items.insert(real_idx, plain_item(node));
        }
        slf.as_super().insert(real_idx, py_val_insert.bind(py))?;
        Ok(())
    }

    #[pyo3(signature = (idx=-1))]
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // see __setitem__
    fn pop(slf: &Bound<'_, Self>, py: Python<'_>, idx: isize) -> PyResult<Py<PyAny>> {
        let (real_idx, node) = {
            let mut borrow = slf.borrow_mut();
            let len = borrow.inner.items.len() as isize;
            if len == 0 {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "pop from empty list",
                ));
            }
            let real_idx = if idx < 0 { len + idx } else { idx };
            if real_idx < 0 || real_idx >= len {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "pop index out of range",
                ));
            }
            let item = borrow.inner.items.remove(real_idx as usize);
            (real_idx, item.value)
        };
        // Same C-level slice trick as __delitem__ to avoid re-entering our override.
        let empty = PyList::empty(py);
        slf.as_super()
            .set_slice(real_idx as usize, real_idx as usize + 1, empty.as_any())?;
        node_to_py(py, &node, None)
    }

    fn remove(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let idx = {
            let borrow = slf.borrow();
            let mut found = None;
            for (i, item) in borrow.inner.items.iter().enumerate() {
                let v = node_to_py(py, &item.value, None)?;
                if v.bind(py).eq(value)? {
                    found = Some(i);
                    break;
                }
            }
            found.ok_or_else(|| pyo3::exceptions::PyValueError::new_err("value not in list"))?
        };
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items.remove(idx);
        }
        slf.as_super().del_item(idx)?;
        Ok(())
    }

    fn extend(slf: &Bound<'_, Self>, iterable: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        // Collect (YamlItem, Py<PyAny>) pairs.
        let mut pairs: Vec<(YamlItem, Py<PyAny>)> = Vec::new();
        if let Ok(other_seq) = iterable.cast::<PyYamlSequence>() {
            // Preserve full YamlItem metadata (comments, blank lines) when
            // extending from another YamlSequence, mirroring what update() does
            // for YamlMapping sources.  Read Python values from the parent list
            // to avoid re-creating objects for nested containers.
            let other_list = other_seq.as_super();
            let borrow = other_seq.borrow();
            for (i, item) in borrow.inner.items.iter().enumerate() {
                let py_val = other_list.get_item(i)?;
                pairs.push((item.clone(), py_val.unbind()));
            }
        } else {
            for py_item in iterable.try_iter()? {
                let py_item = py_item?;
                let (node, py_val) = py_to_node_with_fallback(py, &py_item, None, || {
                    YamlNode::Mapping(YamlMapping::new())
                })?;
                pairs.push((plain_item(node), py_val));
            }
        }
        {
            let mut borrow = slf.borrow_mut();
            for (item, _) in &pairs {
                borrow.inner.items.push(item.clone());
            }
        }
        let list_part = slf.as_super();
        for (_, py_val) in pairs {
            list_part.append(py_val.bind(py))?;
        }
        Ok(())
    }

    fn __iadd__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::extend(slf, other)
    }

    fn reverse(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let list_part = slf.as_super();
        let n = list_part.len();
        // Collect existing Python objects in reversed order before clearing.
        // No node_to_py calls needed — values are unchanged, only order changes.
        let reversed: Vec<Py<PyAny>> = (0..n)
            .rev()
            .map(|i| list_part.get_item(i).map(pyo3::Bound::unbind))
            .collect::<PyResult<_>>()?;
        slf.borrow_mut().inner.items.reverse();
        list_part.call_method0("clear")?;
        for v in &reversed {
            list_part.append(v.bind(py))?;
        }
        Ok(())
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
        let list_part = slf.as_super();
        let n = list_part.len();
        // Collect (inner_item, py_obj) pairs — reuse Python objects from parent list
        // so we never call node_to_py here.
        let pairs: Vec<(YamlItem, Py<PyAny>)> = {
            let borrow = slf.borrow();
            (0..n)
                .map(|i| {
                    Ok((
                        borrow.inner.items[i].clone(),
                        list_part.get_item(i)?.unbind(),
                    ))
                })
                .collect::<PyResult<_>>()?
        };
        // Compute sort keys from existing Python objects (apply key fn if given).
        let sort_keys: Vec<Py<PyAny>> = pairs
            .iter()
            .map(|(_, py_obj)| {
                if let Some(key_fn) = &key {
                    key_fn
                        .bind(py)
                        .call1((py_obj.bind(py),))
                        .map(pyo3::Bound::unbind)
                } else {
                    Ok(py_obj.clone_ref(py))
                }
            })
            .collect::<PyResult<_>>()?;
        let mut zipped: Vec<(Py<PyAny>, YamlItem, Py<PyAny>)> = sort_keys
            .into_iter()
            .zip(pairs)
            .map(|(k, (item, obj))| (k, item, obj))
            .collect();
        let mut err: Option<PyErr> = None;
        zipped.sort_by(|(ka, _, _), (kb, _, _)| {
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
        let mut new_items = Vec::with_capacity(zipped.len());
        let mut new_py = Vec::with_capacity(zipped.len());
        for (_, item, py_obj) in zipped {
            new_items.push(item);
            new_py.push(py_obj);
        }
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items = new_items;
        }
        list_part.call_method0("clear")?;
        for py_obj in &new_py {
            list_part.append(py_obj.bind(py))?;
        }
        if recursive {
            let n = list_part.len();
            for i in 0..n {
                let py_item = list_part.get_item(i)?;
                if let Ok(nested) = py_item.cast::<PyYamlMapping>() {
                    // The user's key function operates on sequence items, not mapping
                    // key strings, so always use natural (None) key for sort_keys.
                    PyYamlMapping::sort_keys(nested, py, None, reverse, true)?;
                }
                if let Ok(nested) = py_item.cast::<PyYamlSequence>() {
                    // Nested sequences get the same key/reverse as the outer sort.
                    PyYamlSequence::sort(
                        nested,
                        py,
                        key.as_ref().map(|k| k.clone_ref(py)),
                        reverse,
                        true,
                    )?;
                }
            }
        }
        Ok(())
    }

    // ── Read-only extras ──────────────────────────────────────────────────────

    fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        sequence_to_python(py, &self.inner)
    }

    /// Read or write the inline comment for the item at *idx*.
    /// `comment_inline(idx)` returns the current comment (``str | None``).
    /// `comment_inline(idx, comment)` sets it; pass ``None`` to clear.
    #[pyo3(signature = (idx, *args))]
    fn comment_inline(
        &mut self,
        py: Python<'_>,
        idx: isize,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        match overload_arg(args, "comment_inline")? {
            OverloadArg::Get => Ok(self.inner.items[i]
                .comment_inline
                .as_deref()
                .into_pyobject(py)?
                .into_any()
                .unbind()),
            OverloadArg::Set(v) => {
                self.inner.items[i].comment_inline = v.extract()?;
                Ok(py.None())
            }
        }
    }

    /// Read or write the block comment above the item at *idx*.
    /// `comment_before(idx)` returns the current comment (``str | None``).
    /// `comment_before(idx, comment)` sets it; pass ``None`` to clear.
    #[pyo3(signature = (idx, *args))]
    fn comment_before(
        &mut self,
        py: Python<'_>,
        idx: isize,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        match overload_arg(args, "comment_before")? {
            OverloadArg::Get => Ok(self.inner.items[i]
                .comment_before
                .as_deref()
                .into_pyobject(py)?
                .into_any()
                .unbind()),
            OverloadArg::Set(v) => {
                self.inner.items[i].comment_before = v.extract()?;
                Ok(py.None())
            }
        }
    }

    /// Return the inline comment for the item at *idx*, or ``None`` if unset.
    /// Raises ``IndexError`` for out-of-range indices.
    fn get_comment_inline(&self, idx: isize) -> PyResult<Option<String>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        Ok(self.inner.items[i].comment_inline.clone())
    }

    /// Set the inline comment for the item at *idx*; pass ``None`` to clear.
    /// Raises ``IndexError`` for out-of-range indices.
    fn set_comment_inline(&mut self, idx: isize, comment: Option<&str>) -> PyResult<()> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        self.inner.items[i].comment_inline = comment.map(str::to_owned);
        Ok(())
    }

    /// Return the block comment above the item at *idx*, or ``None`` if unset.
    /// Raises ``IndexError`` for out-of-range indices.
    fn get_comment_before(&self, idx: isize) -> PyResult<Option<String>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        Ok(self.inner.items[i].comment_before.clone())
    }

    /// Set the block comment above the item at *idx*; pass ``None`` to clear.
    /// Raises ``IndexError`` for out-of-range indices.
    fn set_comment_before(&mut self, idx: isize, comment: Option<&str>) -> PyResult<()> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        self.inner.items[i].comment_before = comment.map(str::to_owned);
        Ok(())
    }

    /// Return the YAML alias name if the item at *idx* is an alias (``*name``), else ``None``.
    /// Raises ``IndexError`` for out-of-range indices.
    fn alias_name(&self, idx: isize) -> PyResult<Option<&str>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        Ok(match &self.inner.items[i].value {
            YamlNode::Alias { name, .. } => Some(name.as_str()),
            _ => None,
        })
    }

    /// Mark the item at *idx* as a YAML alias that emits ``*anchor_name``.
    /// The current value is kept as the resolved node so Python reads still work.
    /// Raises ``IndexError`` for out-of-range indices.
    fn set_alias(&mut self, idx: isize, anchor_name: &str) -> PyResult<()> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        let resolved = Box::new(self.inner.items[i].value.clone());
        self.inner.items[i].value = YamlNode::Alias {
            name: anchor_name.to_owned(),
            resolved,
        };
        Ok(())
    }

    /// The YAML tag on this sequence (e.g. ``"!!seq"``), or ``None``.
    #[getter]
    fn get_tag(&self) -> Option<&str> {
        self.inner.tag.as_deref()
    }

    #[setter]
    fn set_tag(&mut self, tag: Option<&str>) {
        self.inner.tag = tag.map(str::to_owned);
    }

    /// The anchor name declared on this sequence (``&name``), or ``None``.
    #[getter]
    fn get_anchor(&self) -> Option<&str> {
        self.inner.anchor.as_deref()
    }

    #[setter]
    fn set_anchor(&mut self, anchor: Option<&str>) {
        self.inner.anchor = anchor.map(str::to_owned);
    }

    /// The container style: ``"block"`` or ``"flow"``.
    #[getter]
    fn get_style(&self) -> &str {
        match self.inner.style {
            ContainerStyle::Block => "block",
            ContainerStyle::Flow => "flow",
        }
    }

    #[setter]
    fn set_style(&mut self, style: &str) -> PyResult<()> {
        self.inner.style = parse_container_style(style)?;
        Ok(())
    }

    /// The number of blank lines emitted after all items in this sequence.
    #[getter]
    fn get_trailing_blank_lines(&self) -> u8 {
        self.inner.trailing_blank_lines
    }

    #[setter]
    fn set_trailing_blank_lines(&mut self, n: u8) {
        self.inner.trailing_blank_lines = n;
    }

    /// The `%YAML` version directive for this document (e.g. ``"1.2"``), or ``None``.
    #[getter]
    fn get_yaml_version(&self) -> Option<String> {
        self.yaml_version.map(|(maj, min)| format!("{maj}.{min}"))
    }

    #[setter]
    fn set_yaml_version(&mut self, version: Option<&str>) -> PyResult<()> {
        self.yaml_version = parse_yaml_version(version)?;
        Ok(())
    }

    /// Return the underlying YAML node for the item at *idx* as a `YamlScalar`,
    /// `YamlMapping`, or `YamlSequence` object, preserving style/tag metadata.
    /// Raises ``IndexError`` for out-of-range indices.
    fn node(&self, py: Python<'_>, idx: isize) -> PyResult<Py<PyAny>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        node_to_doc(py, self.inner.items[i].value.clone(), DocMeta::none(), None)
    }

    /// Set the block/flow style for the container value at *idx*.
    /// *style* must be ``"block"`` or ``"flow"``.
    /// No-op when the item is a scalar or null.
    /// Raises ``IndexError`` for out-of-range indices; ``ValueError`` for unknown styles.
    fn container_style(slf: &Bound<'_, Self>, idx: isize, style: &str) -> PyResult<()> {
        let new_style = parse_container_style(style)?;
        let i = {
            let borrow = slf.borrow();
            resolve_seq_idx(idx, borrow.inner.items.len())?
        };
        {
            let mut borrow = slf.borrow_mut();
            match &mut borrow.inner.items[i].value {
                YamlNode::Mapping(m) => m.style = new_style,
                YamlNode::Sequence(s) => s.style = new_style,
                _ => {} // scalar / null / alias — silently ignored
            }
        }
        // Also sync the Python-side object stored in the parent list so that
        // extract_yaml_node (which reads inner.style from the child object) sees the change.
        let py_val = slf.as_super().get_item(i)?;
        if let Ok(bound_m) = py_val.cast::<PyYamlMapping>() {
            bound_m.borrow_mut().inner.style = new_style;
        } else if let Ok(bound_s) = py_val.cast::<PyYamlSequence>() {
            bound_s.borrow_mut().inner.style = new_style;
        }
        Ok(())
    }

    /// Set the scalar style for the item at *idx*.
    /// *style* must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
    /// Raises ``IndexError`` for out-of-range indices; ``ValueError`` for unknown styles;
    /// ``TypeError`` if the item is not a scalar (use ``container_style()`` instead).
    fn scalar_style(&mut self, idx: isize, style: &str) -> PyResult<()> {
        let new_style = parse_scalar_style(style)?;
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        match &mut self.inner.items[i].value {
            YamlNode::Scalar(s) => {
                s.style = new_style;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "item at index {idx} is not a scalar; use container_style() for mappings and sequences"
            ))),
        }
    }

    /// Read or write the number of blank lines emitted before the item at *idx*.
    /// `blank_lines_before(idx)` returns the current count (``int``).
    /// `blank_lines_before(idx, n)` sets it; values are clamped to 0–255.
    #[pyo3(signature = (idx, *args))]
    fn blank_lines_before(
        &mut self,
        py: Python<'_>,
        idx: isize,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let i = resolve_seq_idx(idx, self.inner.items.len())?;
        match overload_arg(args, "blank_lines_before")? {
            OverloadArg::Get => Ok(u32::from(self.inner.items[i].blank_lines_before)
                .into_pyobject(py)?
                .into_any()
                .unbind()),
            OverloadArg::Set(v) => {
                let n: u32 = v.extract()?;
                self.inner.items[i].blank_lines_before = n.min(255) as u8;
                Ok(py.None())
            }
        }
    }

    /// Strip cosmetic formatting metadata, resetting to clean YAML defaults.
    ///
    /// All three keyword flags default to ``True``:
    ///
    /// - ``styles``: scalar quoting → plain (or ``literal`` for multi-line strings),
    ///   container style → block, scalar ``original`` values cleared.
    /// - ``comments``: ``comment_before`` and ``comment_inline`` cleared on all items.
    /// - ``blank_lines``: ``blank_lines_before`` zeroed on all items;
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
        // Step 1: reset Rust-inner tree (covers scalar style and item metadata)
        slf.borrow_mut().inner.format_with(opts);
        // Step 2: propagate to Python-side child objects (their own .inner must also be updated)
        for item in slf.as_super().iter() {
            if let Ok(child_m) = item.cast::<PyYamlMapping>() {
                PyYamlMapping::format(child_m, styles, comments, blank_lines)?;
            } else if let Ok(child_s) = item.cast::<PyYamlSequence>() {
                PyYamlSequence::format(child_s, styles, comments, blank_lines)?;
            }
        }
        Ok(())
    }

    /// Return a shallow copy of this sequence (comments, style metadata, and
    /// nested structure are all cloned).
    fn __copy__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let meta = DocMeta {
            explicit_start: self.explicit_start,
            explicit_end: self.explicit_end,
            yaml_version: self.yaml_version,
            tag_directives: self.tag_directives.clone(),
        };
        sequence_to_py_obj(py, self.inner.clone(), meta, None)
    }

    /// Return a deep copy of this sequence.
    ///
    /// Because ``YamlSequence`` owns all its data (no ``Rc``/``Arc`` sharing),
    /// the Rust ``Clone`` is already a deep copy. The *memo* dict is accepted
    /// for API compatibility but is not used.
    fn __deepcopy__(&self, py: Python<'_>, _memo: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        self.__copy__(py)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        sequence_repr(py, &self.inner)
    }
}
