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
use pyo3::types::{PyDict, PyList, PyTuple};
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
/// Mapping → PyYamlMapping, Sequence → PyYamlSequence, scalar/null → Python primitive.
fn node_to_py(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
        YamlNode::Mapping(m) => Ok(PyYamlMapping { inner: m.clone() }
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        YamlNode::Sequence(s) => Ok(PyYamlSequence { inner: s.clone() }
            .into_pyobject(py)?
            .into_any()
            .unbind()),
    }
}

/// Convert a Python object to a YamlNode.
fn py_to_node(obj: &Bound<'_, PyAny>) -> PyResult<YamlNode> {
    if obj.is_none() {
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Null,
        }));
    }
    if let Ok(m) = obj.extract::<PyYamlMapping>() {
        return Ok(YamlNode::Mapping(m.inner));
    }
    if let Ok(s) = obj.extract::<PyYamlSequence>() {
        return Ok(YamlNode::Sequence(s.inner));
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
    Err(PyRuntimeError::new_err(format!(
        "Cannot convert {obj} to YAML node"
    )))
}

/// Convert a top-level YamlNode to PyYamlMapping, PyYamlSequence, or PyYamlScalar.
fn node_to_doc(py: Python<'_>, node: YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Mapping(m) => Ok(PyYamlMapping { inner: m }
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        YamlNode::Sequence(s) => Ok(PyYamlSequence { inner: s }
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        other => Ok(PyYamlScalar { inner: other }
            .into_pyobject(py)?
            .into_any()
            .unbind()),
    }
}

/// Extract a YamlNode from a PyYamlMapping, PyYamlSequence, or PyYamlScalar for serialisation.
fn extract_yaml_node(obj: &Bound<'_, PyAny>) -> PyResult<YamlNode> {
    if let Ok(m) = obj.extract::<PyYamlMapping>() {
        return Ok(YamlNode::Mapping(m.inner));
    }
    if let Ok(s) = obj.extract::<PyYamlSequence>() {
        return Ok(YamlNode::Sequence(s.inner));
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    Err(PyRuntimeError::new_err(
        "expected YamlMapping, YamlSequence, or YamlScalar",
    ))
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

fn sort_sequence(
    py: Python<'_>,
    s: &mut YamlSequence,
    key: Option<&Py<PyAny>>,
    reverse: bool,
) -> PyResult<()> {
    let items = std::mem::take(&mut s.items);

    let computed: Vec<Py<PyAny>> = items
        .iter()
        .map(|item| {
            let v = node_to_py(py, &item.value)?;
            if let Some(key_fn) = key {
                key_fn.bind(py).call1((v,)).map(|r| r.unbind())
            } else {
                Ok(v)
            }
        })
        .collect::<PyResult<_>>()?;

    let mut zipped: Vec<(Py<PyAny>, YamlItem)> = computed.into_iter().zip(items).collect();

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
    s.items = zipped.into_iter().map(|(_, item)| item).collect();
    Ok(())
}

// ─── Shared mapping helpers ───────────────────────────────────────────────────

/// Merge all entries from `other` into `dest`, overwriting existing keys.
/// Accepts PyYamlMapping, any dict-like object with a `keys()` method, or an
/// iterable of (key, value) pairs.
fn mapping_update(dest: &mut YamlMapping, other: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Ok(m) = other.extract::<PyYamlMapping>() {
        for (k, e) in m.inner.entries {
            dest.entries.insert(k, e);
        }
        return Ok(());
    }
    // Duck-typing: if it has a keys() method, treat as a mapping
    if other.hasattr("keys")? {
        let keys = other.call_method0("keys")?;
        for key in keys.try_iter()? {
            let key = key?;
            let val = other.get_item(&key)?;
            let k: String = key.extract()?;
            let node = py_to_node(&val)?;
            dest.entries.insert(
                k,
                YamlEntry {
                    value: node,
                    comment_before: None,
                    comment_inline: None,
                },
            );
        }
        return Ok(());
    }
    // Fallback: iterable of (key, value) pairs
    for item in other.try_iter()? {
        let item = item?;
        let (key, val): (String, Bound<'_, PyAny>) = item.extract()?;
        let node = py_to_node(&val)?;
        dest.entries.insert(
            key,
            YamlEntry {
                value: node,
                comment_before: None,
                comment_inline: None,
            },
        );
    }
    Ok(())
}

/// Build a PyList of (key, value) tuples for a mapping.
fn mapping_items(py: Python<'_>, m: &YamlMapping) -> PyResult<Py<PyAny>> {
    let mut pairs: Vec<Py<PyAny>> = Vec::with_capacity(m.entries.len());
    for (k, e) in &m.entries {
        let v = node_to_py(py, &e.value)?;
        let k_py: Py<PyAny> = k.as_str().into_pyobject(py)?.into_any().unbind();
        let t = PyTuple::new(py, [k_py, v])?.into_any().unbind();
        pairs.push(t);
    }
    Ok(PyList::new(py, pairs)?.into_any().unbind())
}

/// Build a PyList of values for a mapping.
fn mapping_values(py: Python<'_>, m: &YamlMapping) -> PyResult<Py<PyAny>> {
    let vals: Vec<Py<PyAny>> = m
        .entries
        .values()
        .map(|e| node_to_py(py, &e.value))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, vals)?.into_any().unbind())
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

// ─── PyYamlMapping (Python: YamlMapping) ──────────────────────────────────────

/// A YAML mapping node.
#[pyclass(name = "YamlMapping", from_py_object)]
#[derive(Clone)]
pub struct PyYamlMapping {
    inner: types::YamlMapping,
}

#[pymethods]
impl PyYamlMapping {
    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match self.inner.entries.get(key) {
            Some(entry) => node_to_py(py, &entry.value),
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
        }
    }

    fn __setitem__(&mut self, key: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = py_to_node(value)?;
        if let Some(entry) = self.inner.entries.get_mut(key) {
            entry.value = node;
        } else {
            self.inner.entries.insert(
                key.to_owned(),
                YamlEntry {
                    value: node,
                    comment_before: None,
                    comment_inline: None,
                },
            );
        }
        Ok(())
    }

    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        if self.inner.entries.shift_remove(key).is_none() {
            return Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()));
        }
        Ok(())
    }

    fn __contains__(&self, key: &str) -> bool {
        self.inner.entries.contains_key(key)
    }

    fn __len__(&self) -> usize {
        self.inner.entries.len()
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys: Vec<&str> = self.inner.entries.keys().map(|k| k.as_str()).collect();
        let list = PyList::new(py, keys)?;
        Ok(list.call_method0("__iter__")?.unbind())
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let self_dict = mapping_to_dict(py, &self.inner)?;
        self_dict.bind(py).eq(other)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        mapping_repr(py, &self.inner)
    }

    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys: Vec<&str> = self.inner.entries.keys().map(|k| k.as_str()).collect();
        Ok(PyList::new(py, keys)?.into_any().unbind())
    }

    fn values(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        mapping_values(py, &self.inner)
    }

    fn items(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        mapping_items(py, &self.inner)
    }

    #[pyo3(signature = (key, default=None))]
    fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match self.inner.entries.get(key) {
            Some(entry) => node_to_py(py, &entry.value),
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    #[pyo3(signature = (key, default=None))]
    fn pop(
        &mut self,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match self.inner.entries.shift_remove(key) {
            Some(entry) => node_to_py(py, &entry.value),
            None => match default {
                Some(d) => Ok(d),
                None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
            },
        }
    }

    fn update(&mut self, _py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        mapping_update(&mut self.inner, other)
    }

    #[pyo3(signature = (key, default=None))]
    fn setdefault(
        &mut self,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        if !self.inner.entries.contains_key(key) {
            let default_val = default.unwrap_or_else(|| py.None());
            let node = py_to_node(default_val.bind(py))?;
            self.inner.entries.insert(
                key.to_owned(),
                YamlEntry {
                    value: node,
                    comment_before: None,
                    comment_inline: None,
                },
            );
        }
        node_to_py(py, &self.inner.entries[key].value)
    }

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

    fn set_comment_inline(&mut self, key: &str, comment: &str) -> PyResult<()> {
        if let Some(entry) = self.inner.entries.get_mut(key) {
            entry.comment_inline = Some(comment.to_owned());
            Ok(())
        } else {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
        }
    }

    fn set_comment_before(&mut self, key: &str, comment: &str) -> PyResult<()> {
        if let Some(entry) = self.inner.entries.get_mut(key) {
            entry.comment_before = Some(comment.to_owned());
            Ok(())
        } else {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
        }
    }

    #[pyo3(signature = (key=None, reverse=false, recursive=false))]
    fn sort_keys(
        &mut self,
        py: Python<'_>,
        key: Option<Py<PyAny>>,
        reverse: bool,
        recursive: bool,
    ) -> PyResult<()> {
        sort_mapping(py, &mut self.inner, key.as_ref(), reverse, recursive)
    }
}

// ─── PyYamlSequence (Python: YamlSequence) ────────────────────────────────────

/// A YAML sequence node.
#[pyclass(name = "YamlSequence", from_py_object)]
#[derive(Clone)]
pub struct PyYamlSequence {
    inner: types::YamlSequence,
}

#[pymethods]
impl PyYamlSequence {
    fn __getitem__(&self, py: Python<'_>, key: isize) -> PyResult<Py<PyAny>> {
        let len = self.inner.items.len() as isize;
        let real_idx = if key < 0 { len + key } else { key };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "index out of range",
            ));
        }
        node_to_py(py, &self.inner.items[real_idx as usize].value)
    }

    fn __setitem__(&mut self, key: isize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = py_to_node(value)?;
        let len = self.inner.items.len() as isize;
        let real_idx = if key < 0 { len + key } else { key };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "index out of range",
            ));
        }
        self.inner.items[real_idx as usize].value = node;
        Ok(())
    }

    fn __delitem__(&mut self, key: isize) -> PyResult<()> {
        let len = self.inner.items.len() as isize;
        let real_idx = if key < 0 { len + key } else { key };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "index out of range",
            ));
        }
        self.inner.items.remove(real_idx as usize);
        Ok(())
    }

    fn __contains__(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        for item in &self.inner.items {
            let v = node_to_py(py, &item.value)?;
            if v.bind(py).eq(value)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn __len__(&self) -> usize {
        self.inner.items.len()
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let items: Vec<Py<PyAny>> = self
            .inner
            .items
            .iter()
            .map(|i| node_to_py(py, &i.value))
            .collect::<PyResult<_>>()?;
        let list = PyList::new(py, items)?;
        Ok(list.call_method0("__iter__")?.unbind())
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let self_list = sequence_to_dict(py, &self.inner)?;
        self_list.bind(py).eq(other)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        sequence_repr(py, &self.inner)
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        sequence_to_dict(py, &self.inner)
    }

    fn append(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.items.push(YamlItem {
            value: py_to_node(value)?,
            comment_before: None,
            comment_inline: None,
        });
        Ok(())
    }

    fn insert(&mut self, idx: isize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 {
            (len + idx).max(0) as usize
        } else {
            idx.min(len) as usize
        };
        self.inner.items.insert(
            real_idx,
            YamlItem {
                value: py_to_node(value)?,
                comment_before: None,
                comment_inline: None,
            },
        );
        Ok(())
    }

    #[pyo3(signature = (idx=-1))]
    fn pop(&mut self, py: Python<'_>, idx: isize) -> PyResult<Py<PyAny>> {
        let len = self.inner.items.len() as isize;
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
        let item = self.inner.items.remove(real_idx as usize);
        node_to_py(py, &item.value)
    }

    fn remove(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        for (i, item) in self.inner.items.iter().enumerate() {
            let v = node_to_py(py, &item.value)?;
            if v.bind(py).eq(value)? {
                self.inner.items.remove(i);
                return Ok(());
            }
        }
        Err(pyo3::exceptions::PyValueError::new_err("value not in list"))
    }

    fn extend(&mut self, _py: Python<'_>, iterable: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(other) = iterable.extract::<PyYamlSequence>() {
            self.inner.items.extend(other.inner.items);
            return Ok(());
        }
        for item in iterable.try_iter()? {
            let item = item?;
            self.inner.items.push(YamlItem {
                value: py_to_node(&item)?,
                comment_before: None,
                comment_inline: None,
            });
        }
        Ok(())
    }

    #[pyo3(signature = (value, start=0, stop=None))]
    fn index(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        start: isize,
        stop: Option<isize>,
    ) -> PyResult<usize> {
        let len = self.inner.items.len() as isize;
        let start = if start < 0 { (len + start).max(0) } else { start };
        let end = stop.unwrap_or(len);
        let end = if end < 0 { (len + end).max(0) } else { end.min(len) };
        for i in (start as usize)..(end as usize) {
            let v = node_to_py(py, &self.inner.items[i].value)?;
            if v.bind(py).eq(value)? {
                return Ok(i);
            }
        }
        Err(pyo3::exceptions::PyValueError::new_err(
            "value is not in list",
        ))
    }

    fn count(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<usize> {
        let mut n = 0usize;
        for item in &self.inner.items {
            let v = node_to_py(py, &item.value)?;
            if v.bind(py).eq(value)? {
                n += 1;
            }
        }
        Ok(n)
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

    fn set_comment_inline(&mut self, idx: isize, comment: &str) -> PyResult<()> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 { len + idx } else { idx };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        self.inner.items[real_idx as usize].comment_inline = Some(comment.to_owned());
        Ok(())
    }

    fn set_comment_before(&mut self, idx: isize, comment: &str) -> PyResult<()> {
        let len = self.inner.items.len() as isize;
        let real_idx = if idx < 0 { len + idx } else { idx };
        if real_idx < 0 || real_idx >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("index out of range"));
        }
        self.inner.items[real_idx as usize].comment_before = Some(comment.to_owned());
        Ok(())
    }

    fn reverse(&mut self) {
        self.inner.items.reverse();
    }

    #[pyo3(signature = (key=None, reverse=false))]
    fn sort(&mut self, py: Python<'_>, key: Option<Py<PyAny>>, reverse: bool) -> PyResult<()> {
        sort_sequence(py, &mut self.inner, key.as_ref(), reverse)
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
