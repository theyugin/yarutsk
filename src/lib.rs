// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

mod builder;
mod char_traits;
mod emitter;
mod parser;
mod scanner;
mod types;

use builder::parse_str;
use emitter::{emit_docs, emit_node};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use types::*;

// ─── Scalar conversion ────────────────────────────────────────────────────────

fn scalar_to_py(py: Python<'_>, v: &ScalarValue) -> PyResult<Py<PyAny>> {
    match v {
        ScalarValue::Null => Ok(py.None()),
        ScalarValue::Bool(b) => {
            use pyo3::types::PyBool;
            use std::ops::Deref;
            Ok(PyBool::new(py, *b).deref().clone().unbind().into_any())
        }
        ScalarValue::Int(n) => Ok(n.into_pyobject(py)?.into_any().unbind()),
        ScalarValue::Float(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
        ScalarValue::Str(s) => Ok(s.clone().into_pyobject(py)?.into_any().unbind()),
    }
}

// ─── Node ↔ Python conversion ─────────────────────────────────────────────────

/// Convert a YamlNode to its Python representation.
/// Mapping → PyYamlMapping (dict subclass), Sequence → PyYamlSequence (list subclass),
/// scalar/null → Python primitive.
fn node_to_py(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
        YamlNode::Mapping(m) => mapping_to_py_obj(py, m.clone()),
        YamlNode::Sequence(s) => sequence_to_py_obj(py, s.clone()),
    }
}

/// Convert a Python object to a YamlNode.
fn py_to_node(obj: &Bound<'_, PyAny>) -> PyResult<YamlNode> {
    if obj.is_none() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Null,
        }));
    }
    // Check our custom types before primitives (PyYamlMapping extends PyDict,
    // so the PyDict check below would also match — order matters here).
    if let Ok(m) = obj.extract::<PyYamlMapping>() {
        return Ok(YamlNode::Mapping(m.inner));
    }
    if let Ok(s) = obj.extract::<PyYamlSequence>() {
        return Ok(YamlNode::Sequence(s.inner));
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    // Primitives — bool must come before i64 (bool is a subtype of int in Python)
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Bool(b),
        }));
    }
    if let Ok(n) = obj.extract::<i64>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Int(n),
        }));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Float(f),
        }));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Str(s),
        }));
    }
    // Plain dict/list fallback (for users passing native Python dicts/lists).
    // Note: PyYamlMapping extends PyDict so it would match cast::<PyDict>() too,
    // but we already handled it with extract::<PyYamlMapping>() above.
    if let Ok(d) = obj.cast::<PyDict>() {
        let mut mapping = YamlMapping::new();
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            let node = py_to_node(&v)?;
            mapping.entries.insert(
                key,
                YamlEntry {
                    value: node,
                    comment_before: None,
                    comment_inline: None,
                },
            );
        }
        return Ok(YamlNode::Mapping(mapping));
    }
    if let Ok(l) = obj.cast::<PyList>() {
        let mut seq = YamlSequence::new();
        for item in l.iter() {
            seq.items.push(YamlItem {
                value: py_to_node(&item)?,
                comment_before: None,
                comment_inline: None,
            });
        }
        return Ok(YamlNode::Sequence(seq));
    }
    Err(PyRuntimeError::new_err(format!(
        "Cannot convert {obj} to YAML node"
    )))
}

/// Convert a top-level YamlNode to PyYamlMapping, PyYamlSequence, or PyYamlScalar.
fn node_to_doc(py: Python<'_>, node: YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Mapping(m) => mapping_to_py_obj(py, m),
        YamlNode::Sequence(s) => sequence_to_py_obj(py, s),
        other => Ok(PyYamlScalar { inner: other }
            .into_pyobject(py)?
            .into_any()
            .unbind()),
    }
}

/// Extract a YamlNode from a PyYamlMapping, PyYamlSequence, or PyYamlScalar for serialisation.
///
/// For mappings and sequences, current values come from the parent dict/list (so that
/// mutations made to nested objects after they were returned from __getitem__ are visible),
/// while key ordering and comment metadata come from `inner`.
fn extract_yaml_node(obj: &Bound<'_, PyAny>) -> PyResult<YamlNode> {
    if let Ok(bound_m) = obj.cast::<PyYamlMapping>() {
        let borrow = bound_m.borrow();
        let dict_part = bound_m.as_super();
        let mut mapping = YamlMapping::new();
        // Walk inner.entries for key order and comment data; read values from parent dict.
        for (k, e) in &borrow.inner.entries {
            let py_val = match dict_part.get_item(k)? {
                Some(v) => v,
                None => continue, // key was removed; skip
            };
            let node = extract_yaml_node(&py_val)?;
            mapping.entries.insert(
                k.clone(),
                YamlEntry {
                    value: node,
                    comment_before: e.comment_before.clone(),
                    comment_inline: e.comment_inline.clone(),
                },
            );
        }
        return Ok(YamlNode::Mapping(mapping));
    }
    if let Ok(bound_s) = obj.cast::<PyYamlSequence>() {
        let borrow = bound_s.borrow();
        let list_part = bound_s.as_super();
        let inner_len = borrow.inner.items.len();
        let mut seq = YamlSequence::new();
        for i in 0..inner_len {
            let py_val = list_part.get_item(i)?;
            let node = extract_yaml_node(&py_val)?;
            seq.items.push(YamlItem {
                value: node,
                comment_before: borrow.inner.items[i].comment_before.clone(),
                comment_inline: borrow.inner.items[i].comment_inline.clone(),
            });
        }
        return Ok(YamlNode::Sequence(seq));
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    // Scalars passed directly (int, str, etc.)
    if obj.is_none() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Null,
        }));
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Bool(b),
        }));
    }
    if let Ok(n) = obj.extract::<i64>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Int(n),
        }));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Float(f),
        }));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Str(s),
        }));
    }
    Err(PyRuntimeError::new_err(
        "expected YamlMapping, YamlSequence, or YamlScalar",
    ))
}

// ─── Python object creation helpers ──────────────────────────────────────────

/// Create a PyYamlMapping (dict subclass) from a Rust YamlMapping.
/// The parent dict is populated with the mapping's entries.
fn mapping_to_py_obj(py: Python<'_>, m: types::YamlMapping) -> PyResult<Py<PyAny>> {
    // Build Python values before moving m into the struct.
    let py_pairs: Vec<(String, Py<PyAny>)> = m
        .entries
        .iter()
        .map(|(k, e)| {
            let v = node_to_py(py, &e.value)?;
            Ok((k.clone(), v))
        })
        .collect::<PyResult<_>>()?;

    let obj: Py<PyYamlMapping> = Py::new(py, PyYamlMapping { inner: m })?;

    // Populate the underlying dict with Python-visible values.
    {
        let bound = obj.bind(py);
        let dict_part = bound.as_super();
        for (k, v) in &py_pairs {
            dict_part.set_item(k.as_str(), v.bind(py))?;
        }
    }

    Ok(obj.into_any())
}

/// Create a PyYamlSequence (list subclass) from a Rust YamlSequence.
/// The parent list is populated with the sequence's items.
fn sequence_to_py_obj(py: Python<'_>, s: types::YamlSequence) -> PyResult<Py<PyAny>> {
    // Build Python values before moving s into the struct.
    let py_items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|item| node_to_py(py, &item.value))
        .collect::<PyResult<_>>()?;

    let obj: Py<PyYamlSequence> = Py::new(py, PyYamlSequence { inner: s })?;

    // Populate the underlying list with Python-visible values.
    {
        let bound = obj.bind(py);
        let list_part = bound.as_super();
        for v in &py_items {
            list_part.append(v.bind(py))?;
        }
    }

    Ok(obj.into_any())
}

// ─── Repr helpers ─────────────────────────────────────────────────────────────

fn node_repr(py: Python<'_>, node: &YamlNode) -> String {
    match node {
        YamlNode::Mapping(m) => mapping_repr(py, m),
        YamlNode::Sequence(s) => sequence_repr(py, s),
        YamlNode::Scalar(s) => match &s.value {
            ScalarValue::Null => "None".to_string(),
            ScalarValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            ScalarValue::Int(n) => n.to_string(),
            ScalarValue::Float(f) => format!("{f}"),
            ScalarValue::Str(s) => format!("{s:?}"),
        },
        YamlNode::Null => "None".to_string(),
    }
}

fn mapping_repr(py: Python<'_>, m: &YamlMapping) -> String {
    let pairs: Vec<String> = m
        .entries
        .iter()
        .map(|(k, e)| format!("{k:?}: {}", node_repr(py, &e.value)))
        .collect();
    format!("YamlMapping({{{}}})", pairs.join(", "))
}

fn sequence_repr(py: Python<'_>, s: &YamlSequence) -> String {
    let items: Vec<String> = s.items.iter().map(|i| node_repr(py, &i.value)).collect();
    format!("YamlSequence([{}])", items.join(", "))
}

// ─── Dict conversion helpers ──────────────────────────────────────────────────

fn node_to_dict(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
        YamlNode::Mapping(m) => mapping_to_dict(py, m),
        YamlNode::Sequence(s) => sequence_to_dict(py, s),
    }
}

fn mapping_to_dict(py: Python<'_>, m: &YamlMapping) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    for (k, e) in &m.entries {
        let v = node_to_dict(py, &e.value)?;
        d.set_item(k, v)?;
    }
    Ok(d.into_any().unbind())
}

fn sequence_to_dict(py: Python<'_>, s: &YamlSequence) -> PyResult<Py<PyAny>> {
    let items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|i| node_to_dict(py, &i.value))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, items)?.into_any().unbind())
}

// ─── Stream helpers ───────────────────────────────────────────────────────────

fn read_stream(stream: &Bound<'_, PyAny>) -> PyResult<String> {
    let content = stream.call_method0("read")?;
    if let Ok(s) = content.extract::<String>() {
        return Ok(s);
    }
    if let Ok(b) = content.extract::<Vec<u8>>() {
        return String::from_utf8(b)
            .map_err(|e| PyRuntimeError::new_err(format!("UTF-8 decode error: {e}")));
    }
    Err(PyRuntimeError::new_err(
        "stream.read() must return str or bytes",
    ))
}

fn write_to_stream(stream: &Bound<'_, PyAny>, text: &str) -> PyResult<()> {
    if stream.call_method1("write", (text,)).is_ok() {
        return Ok(());
    }
    stream
        .call_method1("write", (text.as_bytes(),))
        .map(|_| ())
        .map_err(|e| PyRuntimeError::new_err(format!("Write error: {e}")))
}

// ─── Parse / emit helpers ─────────────────────────────────────────────────────

fn parse_text(text: &str) -> PyResult<Vec<YamlNode>> {
    parse_str(text).map_err(|e| PyRuntimeError::new_err(format!("Parse error: {e}")))
}

fn emit_node_to_string(node: &YamlNode) -> String {
    let mut out = String::new();
    emit_node(node, 0, &mut out);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

// ─── Sort helpers ─────────────────────────────────────────────────────────────

fn py_compare<'py>(
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
    err: &mut Option<PyErr>,
) -> std::cmp::Ordering {
    match a.lt(b) {
        Err(e) => {
            *err = Some(e);
            std::cmp::Ordering::Equal
        }
        Ok(true) => std::cmp::Ordering::Less,
        Ok(false) => match a.gt(b) {
            Err(e) => {
                *err = Some(e);
                std::cmp::Ordering::Equal
            }
            Ok(true) => std::cmp::Ordering::Greater,
            Ok(false) => std::cmp::Ordering::Equal,
        },
    }
}

fn sort_mapping(
    py: Python<'_>,
    m: &mut YamlMapping,
    key: Option<&Py<PyAny>>,
    reverse: bool,
    recursive: bool,
) -> PyResult<()> {
    if recursive {
        for (_, entry) in m.entries.iter_mut() {
            if let YamlNode::Mapping(nested) = &mut entry.value {
                sort_mapping(py, nested, key, reverse, recursive)?;
            }
        }
    }

    let mut entries: Vec<(String, YamlEntry)> = m.entries.drain(..).collect();

    if let Some(key_fn) = key {
        let computed: Vec<Py<PyAny>> = entries
            .iter()
            .map(|(k, _)| key_fn.bind(py).call1((k.as_str(),)).map(|r| r.unbind()))
            .collect::<PyResult<_>>()?;

        let mut zipped: Vec<(Py<PyAny>, (String, YamlEntry))> =
            computed.into_iter().zip(entries).collect();

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
        entries = zipped.into_iter().map(|(_, e)| e).collect();
    } else {
        entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
    }

    if reverse {
        entries.reverse();
    }
    for (k, v) in entries {
        m.entries.insert(k, v);
    }
    Ok(())
}

// ─── PyYamlScalar (Python: YamlScalar) ───────────────────────────────────────

/// A YAML scalar document node (int, float, bool, str, or null).
#[pyclass(name = "YamlScalar", from_py_object)]
#[derive(Clone)]
pub struct PyYamlScalar {
    inner: YamlNode, // YamlNode::Scalar or YamlNode::Null
}

#[pymethods]
impl PyYamlScalar {
    /// The Python primitive value of this scalar.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
            _ => Ok(py.None()),
        }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.value(py)
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let v = self.value(py)?;
        if let Ok(other_s) = other.extract::<PyYamlScalar>() {
            let ov = other_s.value(py)?;
            v.bind(py).eq(ov.bind(py))
        } else {
            v.bind(py).eq(other)
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let v = self.value(py)?;
        Ok(format!("YamlScalar({})", v.bind(py).repr()?))
    }
}

// ─── PyYamlMapping (Python: YamlMapping extends dict) ─────────────────────────

/// A YAML mapping node. Subclass of dict; the parent dict is always kept in
/// sync with `inner` so that standard dict operations work transparently.
#[pyclass(name = "YamlMapping", extends = PyDict, from_py_object)]
#[derive(Clone)]
pub struct PyYamlMapping {
    inner: types::YamlMapping,
}

#[pymethods]
impl PyYamlMapping {
    #[new]
    fn new() -> Self {
        PyYamlMapping {
            inner: types::YamlMapping::new(),
        }
    }

    // ── Mutations (must sync parent dict) ────────────────────────────────────

    fn __setitem__(slf: &Bound<'_, Self>, key: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = py_to_node(value)?;
        let py = slf.py();
        {
            let mut borrow = slf.borrow_mut();
            if let Some(entry) = borrow.inner.entries.get_mut(key) {
                entry.value = node.clone();
            } else {
                borrow.inner.entries.insert(
                    key.to_owned(),
                    YamlEntry {
                        value: node.clone(),
                        comment_before: None,
                        comment_inline: None,
                    },
                );
            }
        }
        let py_val = node_to_py(py, &node)?;
        slf.as_super().set_item(key, py_val.bind(py))?;
        Ok(())
    }

    fn __delitem__(slf: &Bound<'_, Self>, key: &str) -> PyResult<()> {
        let removed = {
            let mut borrow = slf.borrow_mut();
            borrow.inner.entries.shift_remove(key).is_some()
        };
        if !removed {
            return Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()));
        }
        slf.as_super().del_item(key)?;
        Ok(())
    }

    #[pyo3(signature = (key, default=None))]
    fn pop(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let entry = {
            let mut borrow = slf.borrow_mut();
            borrow.inner.entries.shift_remove(key)
        };
        match entry {
            Some(e) => {
                slf.as_super().del_item(key)?;
                node_to_py(py, &e.value)
            }
            None => match default {
                Some(d) => Ok(d),
                None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
            },
        }
    }

    fn update(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        // Update inner AND parent dict together for only the keys in `other`,
        // leaving unchanged keys untouched in both. This avoids an O(n) full rebuild
        // and the unnecessary node_to_py calls for keys not present in `other`.
        let py = slf.py();
        let dict_part = slf.as_super();
        if let Ok(m) = other.extract::<PyYamlMapping>() {
            // Read existing Python values from `other`'s parent dict to avoid
            // re-creating Python objects for nested mappings/sequences.
            let other_bound = other.cast::<PyYamlMapping>()?;
            let other_dict = other_bound.as_super();
            {
                let mut borrow = slf.borrow_mut();
                for (k, e) in &m.inner.entries {
                    borrow.inner.entries.insert(k.clone(), e.clone());
                }
            }
            for k in m.inner.entries.keys() {
                if let Some(py_val) = other_dict.get_item(k)? {
                    dict_part.set_item(k, &py_val)?;
                }
            }
            return Ok(());
        }
        // Duck-typing: keys() + __getitem__, or iterable of pairs.
        // Update inner and parent dict key-by-key without full rebuild.
        if other.hasattr("keys")? {
            let keys = other.call_method0("keys")?;
            for key in keys.try_iter()? {
                let key = key?;
                let val = other.get_item(&key)?;
                let k: String = key.extract()?;
                let node = py_to_node(&val)?;
                let py_val = node_to_py(py, &node)?;
                {
                    let mut borrow = slf.borrow_mut();
                    borrow.inner.entries.insert(
                        k.clone(),
                        YamlEntry {
                            value: node,
                            comment_before: None,
                            comment_inline: None,
                        },
                    );
                }
                dict_part.set_item(k.as_str(), py_val.bind(py))?;
            }
            return Ok(());
        }
        for item in other.try_iter()? {
            let item = item?;
            let (k, val): (String, Bound<'_, PyAny>) = item.extract()?;
            let node = py_to_node(&val)?;
            let py_val = node_to_py(py, &node)?;
            {
                let mut borrow = slf.borrow_mut();
                borrow.inner.entries.insert(
                    k.clone(),
                    YamlEntry {
                        value: node,
                        comment_before: None,
                        comment_inline: None,
                    },
                );
            }
            dict_part.set_item(k.as_str(), py_val.bind(py))?;
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None))]
    fn setdefault(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let contains = slf.borrow().inner.entries.contains_key(key);
        if !contains {
            let default_val = default.unwrap_or_else(|| py.None());
            let node = py_to_node(default_val.bind(py))?;
            let py_val = node_to_py(py, &node)?;
            {
                let mut borrow = slf.borrow_mut();
                borrow.inner.entries.insert(
                    key.to_owned(),
                    YamlEntry {
                        value: node,
                        comment_before: None,
                        comment_inline: None,
                    },
                );
            }
            slf.as_super().set_item(key, py_val.bind(py))?;
        }
        let borrow = slf.borrow();
        node_to_py(py, &borrow.inner.entries[key].value)
    }

    #[pyo3(signature = (key=None, reverse=false, recursive=false))]
    fn sort_keys(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: Option<Py<PyAny>>,
        reverse: bool,
        recursive: bool,
    ) -> PyResult<()> {
        {
            let mut borrow = slf.borrow_mut();
            sort_mapping(py, &mut borrow.inner, key.as_ref(), reverse, recursive)?;
        }
        let dict_part = slf.as_super();
        if recursive {
            // Recursive sort may have reordered nested objects' inner.entries too.
            // Recreate Python objects from inner (simpler than chasing nested refs).
            dict_part.clear();
            let borrow = slf.borrow();
            for (k, e) in &borrow.inner.entries {
                let py_val = node_to_py(py, &e.value)?;
                dict_part.set_item(k, py_val.bind(py))?;
            }
        } else {
            // Non-recursive: only key order changed; Python objects are unchanged.
            // Read them back from parent dict in the new sorted order and reinsert —
            // no node_to_py calls needed.
            let sorted_keys: Vec<String> = slf.borrow().inner.entries.keys().cloned().collect();
            let py_vals: Vec<Py<PyAny>> = sorted_keys
                .iter()
                .filter_map(|k| dict_part.get_item(k).ok()?.map(|v| v.unbind()))
                .collect();
            dict_part.clear();
            for (k, v) in sorted_keys.iter().zip(py_vals.iter()) {
                dict_part.set_item(k.as_str(), v.bind(py))?;
            }
        }
        Ok(())
    }

    // ── Read-only extras ──────────────────────────────────────────────────────

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        mapping_to_dict(py, &self.inner)
    }

    fn get_comment_inline(&self, key: &str) -> Option<String> {
        self.inner
            .entries
            .get(key)
            .and_then(|e| e.comment_inline.clone())
    }

    fn get_comment_before(&self, key: &str) -> Option<String> {
        self.inner
            .entries
            .get(key)
            .and_then(|e| e.comment_before.clone())
    }

    fn set_comment_inline(&mut self, key: &str, comment: Option<&str>) -> PyResult<()> {
        if let Some(entry) = self.inner.entries.get_mut(key) {
            entry.comment_inline = comment.map(str::to_owned);
            Ok(())
        } else {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
        }
    }

    fn set_comment_before(&mut self, key: &str, comment: Option<&str>) -> PyResult<()> {
        if let Some(entry) = self.inner.entries.get_mut(key) {
            entry.comment_before = comment.map(str::to_owned);
            Ok(())
        } else {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
        }
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        mapping_repr(py, &self.inner)
    }
}

// ─── PyYamlSequence (Python: YamlSequence extends list) ──────────────────────

/// A YAML sequence node. Subclass of list; the parent list is always kept in
/// sync with `inner` so that standard list operations work transparently.
#[pyclass(name = "YamlSequence", extends = PyList, from_py_object)]
#[derive(Clone)]
pub struct PyYamlSequence {
    inner: types::YamlSequence,
}

#[pymethods]
impl PyYamlSequence {
    #[new]
    fn new() -> Self {
        PyYamlSequence {
            inner: types::YamlSequence::new(),
        }
    }

    // ── Mutations (must sync parent list) ────────────────────────────────────

    fn __setitem__(slf: &Bound<'_, Self>, key: isize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = py_to_node(value)?;
        let py = slf.py();
        let len = slf.borrow().inner.items.len() as isize;
        let real_idx = if key < 0 { len + key } else { key };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items[real_idx as usize].value = node.clone();
        }
        let py_val = node_to_py(py, &node)?;
        slf.as_super().set_item(real_idx as usize, py_val.bind(py))?;
        Ok(())
    }

    fn __delitem__(slf: &Bound<'_, Self>, key: isize) -> PyResult<()> {
        let len = slf.borrow().inner.items.len() as isize;
        let real_idx = if key < 0 { len + key } else { key };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
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
        Ok(())
    }

    fn append(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = py_to_node(value)?;
        let py = slf.py();
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items.push(YamlItem {
                value: node.clone(),
                comment_before: None,
                comment_inline: None,
            });
        }
        let py_val = node_to_py(py, &node)?;
        slf.as_super().append(py_val.bind(py))?;
        Ok(())
    }

    fn insert(slf: &Bound<'_, Self>, idx: isize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = py_to_node(value)?;
        let py = slf.py();
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
            borrow.inner.items.insert(
                real_idx,
                YamlItem {
                    value: node.clone(),
                    comment_before: None,
                    comment_inline: None,
                },
            );
        }
        let py_val = node_to_py(py, &node)?;
        slf.as_super().insert(real_idx, py_val.bind(py))?;
        Ok(())
    }

    #[pyo3(signature = (idx=-1))]
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
        node_to_py(py, &node)
    }

    fn remove(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let idx = {
            let borrow = slf.borrow();
            let mut found = None;
            for (i, item) in borrow.inner.items.iter().enumerate() {
                let v = node_to_py(py, &item.value)?;
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
        if let Ok(other) = iterable.extract::<PyYamlSequence>() {
            for item in &other.inner.items {
                let py_val = node_to_py(py, &item.value)?;
                pairs.push((
                    YamlItem {
                        value: item.value.clone(),
                        comment_before: None,
                        comment_inline: None,
                    },
                    py_val,
                ));
            }
        } else {
            for py_item in iterable.try_iter()? {
                let py_item = py_item?;
                let node = py_to_node(&py_item)?;
                let py_val = node_to_py(py, &node)?;
                pairs.push((
                    YamlItem {
                        value: node,
                        comment_before: None,
                        comment_inline: None,
                    },
                    py_val,
                ));
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

    fn reverse(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let list_part = slf.as_super();
        let n = list_part.len();
        // Collect existing Python objects in reversed order before clearing.
        // No node_to_py calls needed — values are unchanged, only order changes.
        let reversed: Vec<Py<PyAny>> = (0..n)
            .rev()
            .map(|i| list_part.get_item(i).map(|v| v.unbind()))
            .collect::<PyResult<_>>()?;
        slf.borrow_mut().inner.items.reverse();
        list_part.call_method0("clear")?;
        for v in &reversed {
            list_part.append(v.bind(py))?;
        }
        Ok(())
    }

    #[pyo3(signature = (key=None, reverse=false))]
    fn sort(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: Option<Py<PyAny>>,
        reverse: bool,
    ) -> PyResult<()> {
        let list_part = slf.as_super();
        let n = list_part.len();
        // Collect (inner_item, py_obj) pairs — reuse Python objects from parent list
        // so we never call node_to_py here.
        let pairs: Vec<(YamlItem, Py<PyAny>)> = {
            let borrow = slf.borrow();
            (0..n)
                .map(|i| Ok((borrow.inner.items[i].clone(), list_part.get_item(i)?.unbind())))
                .collect::<PyResult<_>>()?
        };
        // Compute sort keys from existing Python objects (apply key fn if given).
        let sort_keys: Vec<Py<PyAny>> = pairs
            .iter()
            .map(|(_, py_obj)| {
                if let Some(key_fn) = &key {
                    key_fn.bind(py).call1((py_obj.bind(py),)).map(|r| r.unbind())
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
        {
            let mut borrow = slf.borrow_mut();
            borrow.inner.items = zipped.iter().map(|(_, item, _)| item.clone()).collect();
        }
        list_part.call_method0("clear")?;
        for (_, _, py_obj) in &zipped {
            list_part.append(py_obj.bind(py))?;
        }
        Ok(())
    }

    // ── Read-only extras ──────────────────────────────────────────────────────

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        sequence_to_dict(py, &self.inner)
    }

    fn get_comment_inline(&self, idx: isize) -> PyResult<Option<String>> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 { len + idx } else { idx };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        Ok(self.inner.items[real_idx as usize].comment_inline.clone())
    }

    fn get_comment_before(&self, idx: isize) -> PyResult<Option<String>> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 { len + idx } else { idx };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        Ok(self.inner.items[real_idx as usize].comment_before.clone())
    }

    fn set_comment_inline(&mut self, idx: isize, comment: Option<&str>) -> PyResult<()> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 { len + idx } else { idx };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        self.inner.items[real_idx as usize].comment_inline = comment.map(str::to_owned);
        Ok(())
    }

    fn set_comment_before(&mut self, idx: isize, comment: Option<&str>) -> PyResult<()> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 { len + idx } else { idx };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        self.inner.items[real_idx as usize].comment_before = comment.map(str::to_owned);
        Ok(())
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        sequence_repr(py, &self.inner)
    }
}

// ─── Module-level functions ───────────────────────────────────────────────────

#[pyfunction]
fn load(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let text = read_stream(stream)?;
    let mut docs = parse_text(&text)?;
    if docs.is_empty() {
        return Ok(py.None());
    }
    node_to_doc(py, docs.swap_remove(0))
}

#[pyfunction]
fn loads(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
    let mut docs = parse_text(text)?;
    if docs.is_empty() {
        return Ok(py.None());
    }
    node_to_doc(py, docs.swap_remove(0))
}

#[pyfunction]
fn load_all(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let text = read_stream(stream)?;
    let docs = parse_text(&text)?;
    let pydocs: Vec<Py<PyAny>> = docs
        .into_iter()
        .map(|d| node_to_doc(py, d))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, pydocs)?.into_any().unbind())
}

#[pyfunction]
fn loads_all(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
    let docs = parse_text(text)?;
    let pydocs: Vec<Py<PyAny>> = docs
        .into_iter()
        .map(|d| node_to_doc(py, d))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, pydocs)?.into_any().unbind())
}

#[pyfunction]
fn dump(doc: &Bound<'_, PyAny>, stream: &Bound<'_, PyAny>) -> PyResult<()> {
    let node = extract_yaml_node(doc)?;
    write_to_stream(stream, &emit_node_to_string(&node))
}

#[pyfunction]
fn dumps(doc: &Bound<'_, PyAny>) -> PyResult<String> {
    let node = extract_yaml_node(doc)?;
    Ok(emit_node_to_string(&node))
}

#[pyfunction]
fn dump_all(_py: Python<'_>, docs: &Bound<'_, PyAny>, stream: &Bound<'_, PyAny>) -> PyResult<()> {
    let nodes: Vec<YamlNode> = docs
        .try_iter()?
        .map(|item| extract_yaml_node(&item?))
        .collect::<PyResult<_>>()?;
    write_to_stream(stream, &emit_docs(&nodes))
}

#[pyfunction]
fn dumps_all(_py: Python<'_>, docs: &Bound<'_, PyAny>) -> PyResult<String> {
    let nodes: Vec<YamlNode> = docs
        .try_iter()?
        .map(|item| extract_yaml_node(&item?))
        .collect::<PyResult<_>>()?;
    Ok(emit_docs(&nodes))
}

/// The yarutsk module.
#[pymodule]
fn yarutsk(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyYamlScalar>()?;
    m.add_class::<PyYamlMapping>()?;
    m.add_class::<PyYamlSequence>()?;
    m.add_function(wrap_pyfunction!(load, m)?)?;
    m.add_function(wrap_pyfunction!(loads, m)?)?;
    m.add_function(wrap_pyfunction!(load_all, m)?)?;
    m.add_function(wrap_pyfunction!(loads_all, m)?)?;
    m.add_function(wrap_pyfunction!(dump, m)?)?;
    m.add_function(wrap_pyfunction!(dumps, m)?)?;
    m.add_function(wrap_pyfunction!(dump_all, m)?)?;
    m.add_function(wrap_pyfunction!(dumps_all, m)?)?;
    Ok(())
}
