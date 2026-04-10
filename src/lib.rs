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

/// A single YAML document or sub-node.
///
/// Wraps a YamlNode and exposes a Python-friendly interface for reading,
/// writing, and comment manipulation.
#[pyclass(name = "PyYamlDocument", from_py_object)]
#[derive(Clone)]
pub struct PyYamlDocument {
    inner: YamlNode,
}

impl PyYamlDocument {
    fn from_node(node: YamlNode) -> Self {
        PyYamlDocument { inner: node }
    }

    /// Convert a YamlNode to a Python object (primitive or PyYamlDocument).
    fn node_to_py(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
        match node {
            YamlNode::Null => Ok(py.None()),
            YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
            YamlNode::Mapping(_) | YamlNode::Sequence(_) => {
                Ok(PyYamlDocument::from_node(node.clone())
                    .into_pyobject(py)?
                    .into_any()
                    .unbind())
            }
        }
    }

    /// Convert a Python object to a YamlNode.
    fn py_to_node(obj: &Bound<'_, PyAny>) -> PyResult<YamlNode> {
        if obj.is_none() {
            return Ok(YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Null,
            }));
        }
        if let Ok(doc) = obj.extract::<PyYamlDocument>() {
            return Ok(doc.inner);
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

    fn emit_to_string(&self) -> String {
        let mut out = String::new();
        emit_node(&self.inner, 0, &mut out);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

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

#[pymethods]
impl PyYamlDocument {
    /// Read the value at the given key (for mappings) or index (for sequences).
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            YamlNode::Mapping(m) => {
                let k: String = key.extract()?;
                match m.entries.get(&k) {
                    Some(entry) => Self::node_to_py(py, &entry.value),
                    None => Err(pyo3::exceptions::PyKeyError::new_err(k)),
                }
            }
            YamlNode::Sequence(s) => {
                let idx: isize = key.extract()?;
                let len = s.items.len() as isize;
                let real_idx = if idx < 0 { len + idx } else { idx };
                if real_idx < 0 || real_idx >= len {
                    return Err(pyo3::exceptions::PyIndexError::new_err(
                        "index out of range",
                    ));
                }
                Self::node_to_py(py, &s.items[real_idx as usize].value)
            }
            _ => Err(PyRuntimeError::new_err("not a mapping or sequence")),
        }
    }

    /// Set the value at the given key (for mappings) or index (for sequences).
    fn __setitem__(&mut self, key: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let node = Self::py_to_node(value)?;
        match &mut self.inner {
            YamlNode::Mapping(m) => {
                let k: String = key.extract()?;
                if let Some(entry) = m.entries.get_mut(&k) {
                    entry.value = node;
                } else {
                    m.entries.insert(
                        k,
                        YamlEntry {
                            value: node,
                            comment_before: None,
                            comment_inline: None,
                        },
                    );
                }
            }
            YamlNode::Sequence(s) => {
                let idx: isize = key.extract()?;
                let len = s.items.len() as isize;
                let real_idx = if idx < 0 { len + idx } else { idx };
                if real_idx < 0 || real_idx >= len {
                    return Err(pyo3::exceptions::PyIndexError::new_err(
                        "index out of range",
                    ));
                }
                s.items[real_idx as usize].value = node;
            }
            _ => return Err(PyRuntimeError::new_err("not a mapping or sequence")),
        }
        Ok(())
    }

    /// Check if key exists (for mappings).
    fn __contains__(&self, key: &str) -> bool {
        match &self.inner {
            YamlNode::Mapping(m) => m.entries.contains_key(key),
            _ => false,
        }
    }

    /// Return number of entries (for sequences).
    fn __len__(&self) -> usize {
        match &self.inner {
            YamlNode::Mapping(m) => m.entries.len(),
            YamlNode::Sequence(s) => s.items.len(),
            _ => 0,
        }
    }

    /// Return the list of keys in insertion order (for mappings).
    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            YamlNode::Mapping(m) => {
                let keys: Vec<&str> = m.entries.keys().map(|k| k.as_str()).collect();
                Ok(PyList::new(py, keys)?.into_any().unbind())
            }
            _ => Err(PyRuntimeError::new_err("not a mapping")),
        }
    }

    /// Deep conversion to a plain Python dict/list/primitive.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        node_to_dict(py, &self.inner)
    }

    /// Serialize to a stream (StringIO or BytesIO).
    fn dump(&self, stream: &Bound<'_, PyAny>) -> PyResult<()> {
        let text = self.emit_to_string();
        write_to_stream(stream, &text)
    }

    /// Get the inline comment for a key (without the leading `#`).
    fn get_comment_inline(&self, key: &str) -> Option<String> {
        match &self.inner {
            YamlNode::Mapping(m) => m.entries.get(key).and_then(|e| e.comment_inline.clone()),
            _ => None,
        }
    }

    /// Get the before-key comment for a key (without the leading `#`).
    fn get_comment_before(&self, key: &str) -> Option<String> {
        match &self.inner {
            YamlNode::Mapping(m) => m.entries.get(key).and_then(|e| e.comment_before.clone()),
            _ => None,
        }
    }

    /// Set the inline comment for a key.
    fn set_comment_inline(&mut self, key: &str, comment: &str) -> PyResult<()> {
        match &mut self.inner {
            YamlNode::Mapping(m) => {
                if let Some(entry) = m.entries.get_mut(key) {
                    entry.comment_inline = Some(comment.to_owned());
                    Ok(())
                } else {
                    Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
                }
            }
            _ => Err(PyRuntimeError::new_err("not a mapping")),
        }
    }

    /// Set the before-key comment for a key.
    fn set_comment_before(&mut self, key: &str, comment: &str) -> PyResult<()> {
        match &mut self.inner {
            YamlNode::Mapping(m) => {
                if let Some(entry) = m.entries.get_mut(key) {
                    entry.comment_before = Some(comment.to_owned());
                    Ok(())
                } else {
                    Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
                }
            }
            _ => Err(PyRuntimeError::new_err("not a mapping")),
        }
    }

    /// Sort mapping keys in-place.
    ///
    /// key: optional callable(key_str) -> sort_key; defaults to alphabetical
    /// reverse: if True, descending order
    /// recursive: if True, also sort nested mappings
    #[pyo3(signature = (key=None, reverse=false, recursive=false))]
    fn sort_keys(
        &mut self,
        py: Python<'_>,
        key: Option<Py<PyAny>>,
        reverse: bool,
        recursive: bool,
    ) -> PyResult<()> {
        match &mut self.inner {
            YamlNode::Mapping(m) => sort_mapping(py, m, key.as_ref(), reverse, recursive),
            _ => Err(PyRuntimeError::new_err("sort_keys requires a mapping")),
        }
    }

    /// Sort sequence items in-place.
    ///
    /// key: optional callable(item_value) -> sort_key; defaults to natural ordering
    /// reverse: if True, descending order
    #[pyo3(signature = (key=None, reverse=false))]
    fn sort(&mut self, py: Python<'_>, key: Option<Py<PyAny>>, reverse: bool) -> PyResult<()> {
        match &mut self.inner {
            YamlNode::Sequence(s) => sort_sequence(py, s, key.as_ref(), reverse),
            _ => Err(PyRuntimeError::new_err("sort requires a sequence")),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        node_repr(py, &self.inner)
    }
}

// ─── Sorting helpers ─────────────────────────────────────────────────────────

/// Three-way comparison using Python's `<` and `>` operators.
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
            let v = PyYamlDocument::node_to_py(py, &item.value)?;
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

fn node_repr(py: Python<'_>, node: &YamlNode) -> String {
    match node {
        YamlNode::Mapping(m) => {
            let pairs: Vec<String> = m
                .entries
                .iter()
                .map(|(k, e)| {
                    format!(
                        "{}: {}",
                        PyYamlDocument::from_node(YamlNode::Scalar(YamlScalar {
                            value: ScalarValue::Str(k.clone())
                        }))
                        .__repr__(py),
                        node_repr(py, &e.value)
                    )
                })
                .collect();
            format!("YamlMapping({{{}}})", pairs.join(", "))
        }
        YamlNode::Sequence(s) => {
            let items: Vec<String> = s.items.iter().map(|i| node_repr(py, &i.value)).collect();
            format!("YamlSequence([{}])", items.join(", "))
        }
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

fn node_to_dict(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
        YamlNode::Mapping(m) => {
            let d = PyDict::new(py);
            for (k, e) in &m.entries {
                let v = node_to_dict(py, &e.value)?;
                d.set_item(k, v)?;
            }
            Ok(d.into_any().unbind())
        }
        YamlNode::Sequence(s) => {
            let items: Vec<Py<PyAny>> = s
                .items
                .iter()
                .map(|i| node_to_dict(py, &i.value))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, items)?.into_any().unbind())
        }
    }
}

/// Read text from a stream (StringIO or BytesIO).
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

/// Write text to a stream (StringIO or BytesIO).
fn write_to_stream(stream: &Bound<'_, PyAny>, text: &str) -> PyResult<()> {
    // Try writing as str first (StringIO)
    if stream.call_method1("write", (text,)).is_ok() {
        return Ok(());
    }
    // Fall back to bytes (BytesIO)
    stream
        .call_method1("write", (text.as_bytes(),))
        .map(|_| ())
        .map_err(|e| PyRuntimeError::new_err(format!("Write error: {e}")))
}

// ─── Module-level functions ───────────────────────────────────────────────────

/// Parse a single YAML document from a stream.
#[pyfunction]
fn load(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let text = read_stream(stream)?;
    let docs =
        parse_str(&text).map_err(|e| PyRuntimeError::new_err(format!("Parse error: {e}")))?;
    if docs.is_empty() {
        return Ok(py.None());
    }
    let doc = PyYamlDocument::from_node(docs.into_iter().next().unwrap());
    Ok(doc.into_pyobject(py)?.into_any().unbind())
}

/// Parse all YAML documents from a stream, returning a list.
#[pyfunction]
fn load_all(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let text = read_stream(stream)?;
    let docs =
        parse_str(&text).map_err(|e| PyRuntimeError::new_err(format!("Parse error: {e}")))?;
    let pydocs: Vec<Py<PyAny>> = docs
        .into_iter()
        .map(|d| {
            PyYamlDocument::from_node(d)
                .into_pyobject(py)
                .map(|o| o.into_any().unbind())
        })
        .collect::<Result<_, _>>()?;
    Ok(PyList::new(py, pydocs)?.into_any().unbind())
}

/// Serialize a document to a stream.
#[pyfunction]
fn dump(doc: &PyYamlDocument, stream: &Bound<'_, PyAny>) -> PyResult<()> {
    doc.dump(stream)
}

/// Serialize multiple documents to a stream.
#[pyfunction]
fn dump_all(_py: Python<'_>, docs: &Bound<'_, PyAny>, stream: &Bound<'_, PyAny>) -> PyResult<()> {
    let doc_list = docs.extract::<Vec<PyYamlDocument>>()?;
    let nodes: Vec<YamlNode> = doc_list.into_iter().map(|d| d.inner).collect();
    let text = emit_docs(&nodes);
    write_to_stream(stream, &text)
}

/// The yarutsk module.
#[pymodule]
fn yarutsk(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyYamlDocument>()?;
    m.add_function(wrap_pyfunction!(load, m)?)?;
    m.add_function(wrap_pyfunction!(load_all, m)?)?;
    m.add_function(wrap_pyfunction!(dump, m)?)?;
    m.add_function(wrap_pyfunction!(dump_all, m)?)?;
    Ok(())
}
