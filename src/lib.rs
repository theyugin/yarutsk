// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

#[doc(hidden)]
pub mod core;
mod py;

use core::builder;
use core::emitter::{emit_docs, emit_docs_to};
use core::types::YamlNode;
use py::convert::{AnchorGuard, DocMeta, extract_yaml_node, node_to_doc, parse_stream, parse_text};
use py::py_iter::{PyYamlIter, YamlIterInner};
use py::py_mapping::PyYamlMapping;
use py::py_scalar::PyYamlScalar;
use py::py_sequence::PyYamlSequence;
use py::schema::Schema;
use py::streaming::PyStreamWriter;
use py::streaming::{CharsSource, StringCharsIter};
use pyo3::prelude::*;
use pyo3::types::PyList;

pyo3::create_exception!(yarutsk, YarutskError, pyo3::exceptions::PyException);
pyo3::create_exception!(yarutsk, ParseError, YarutskError);
pyo3::create_exception!(yarutsk, LoaderError, YarutskError);
pyo3::create_exception!(yarutsk, DumperError, YarutskError);

// ─── Module-level helpers ─────────────────────────────────────────────────────

/// Build a `DocMeta` for document index `i` from a slice of `DocMetadata`.
fn doc_meta(meta: &[builder::DocMetadata], i: usize) -> DocMeta {
    let m = meta.get(i).cloned().unwrap_or_default();
    DocMeta {
        explicit_start: m.explicit_start,
        explicit_end: m.explicit_end,
        yaml_version: m.yaml_version,
        tag_directives: m.tag_directives,
    }
}

/// Build a single-doc `DocMetadata` from a Python doc object's flags. Probes
/// each of the three doc-carrying classes once; falls back to defaults if the
/// object isn't one of them.
fn doc_meta_from_py(doc: &Bound<'_, PyAny>) -> builder::DocMetadata {
    if let Ok(m) = doc.cast::<PyYamlMapping>() {
        let m = m.borrow();
        return builder::DocMetadata {
            explicit_start: m.explicit_start,
            explicit_end: m.explicit_end,
            yaml_version: m.yaml_version,
            tag_directives: m.tag_directives.clone(),
        };
    }
    if let Ok(s) = doc.cast::<PyYamlSequence>() {
        let s = s.borrow();
        return builder::DocMetadata {
            explicit_start: s.explicit_start,
            explicit_end: s.explicit_end,
            yaml_version: s.yaml_version,
            tag_directives: s.tag_directives.clone(),
        };
    }
    if let Ok(sc) = doc.extract::<PyYamlScalar>() {
        return builder::DocMetadata {
            explicit_start: sc.explicit_start,
            explicit_end: sc.explicit_end,
            yaml_version: sc.yaml_version,
            tag_directives: sc.tag_directives,
        };
    }
    builder::DocMetadata::default()
}

/// Extract a `YamlNode` plus its per-doc metadata from a Python doc object.
/// Manages anchor state via [`AnchorGuard`].
fn extract_doc_and_meta(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<(YamlNode, builder::DocMetadata)> {
    let _guard = AnchorGuard::new(doc);
    let node = extract_yaml_node(doc, schema)?;
    Ok((node, doc_meta_from_py(doc)))
}

fn emit_doc_to_string(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    indent: usize,
) -> PyResult<String> {
    let (node, meta) = extract_doc_and_meta(doc, schema)?;
    Ok(emit_docs(std::slice::from_ref(&node), &[meta], indent))
}

/// Emit a single document directly to a Python IO stream via [`PyStreamWriter`].
fn emit_doc_to_stream(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    stream: &Bound<'_, PyAny>,
    indent: usize,
) -> PyResult<()> {
    let (node, meta) = extract_doc_and_meta(doc, schema)?;
    let mut writer = PyStreamWriter::new(stream.clone().unbind());
    let _ = emit_docs_to(std::slice::from_ref(&node), &[meta], indent, &mut writer);
    if let Some(err) = writer.take_error() {
        return Err(err);
    }
    Ok(())
}

/// Accept either `str` or `bytes`/`bytearray`; returns an owned `String`.
/// Bytes input must be UTF-8 — invalid sequences raise `UnicodeDecodeError`.
fn coerce_text(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    if let Ok(b) = obj.extract::<Vec<u8>>() {
        let py = obj.py();
        return match String::from_utf8(b) {
            Ok(s) => Ok(s),
            Err(e) => {
                let bytes = e.as_bytes().to_vec();
                let utf8_err = e.utf8_error();
                match pyo3::exceptions::PyUnicodeDecodeError::new_utf8(py, &bytes, utf8_err) {
                    Ok(err) => Err(PyErr::from_value(err.into_any())),
                    Err(pyerr) => Err(pyerr),
                }
            }
        };
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "expected str, bytes, or bytearray",
    ))
}

// ─── Module-level functions ───────────────────────────────────────────────────

/// Convert the first parsed doc (or `None`) to a Python doc object.
fn convert_first_doc(
    py: Python<'_>,
    mut out: builder::ParseOutput,
    sb: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    if out.docs.is_empty() {
        return Ok(py.None());
    }
    let meta = doc_meta(&out.docs_meta, 0);
    node_to_doc(py, out.docs.swap_remove(0), meta, sb)
}

/// Convert all parsed docs to a Python list of doc objects.
fn convert_all_docs(
    py: Python<'_>,
    out: builder::ParseOutput,
    sb: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    let builder::ParseOutput { docs, docs_meta } = out;
    let pydocs: Vec<Py<PyAny>> = docs
        .into_iter()
        .enumerate()
        .map(|(i, d)| node_to_doc(py, d, doc_meta(&docs_meta, i), sb))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, pydocs)?.into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn load(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let out = parse_stream(stream, sb_borrow.as_deref())?;
    convert_first_doc(py, out, sb)
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn loads(
    py: Python<'_>,
    text: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let text = coerce_text(text)?;
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let out = parse_text(&text, sb_borrow.as_deref())?;
    convert_first_doc(py, out, sb)
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn load_all(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let out = parse_stream(stream, sb_borrow.as_deref())?;
    convert_all_docs(py, out, sb)
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn loads_all(
    py: Python<'_>,
    text: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let text = coerce_text(text)?;
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let out = parse_text(&text, sb_borrow.as_deref())?;
    convert_all_docs(py, out, sb)
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn iter_load_all(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyYamlIter>> {
    use core::builder::Builder;
    use core::parser::Parser;
    use py::streaming::PyIoCharsIter;

    use pyo3::PyErr;
    use std::sync::{Arc, Mutex};

    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let policy = sb_borrow.as_deref().and_then(Schema::tag_policy);
    let error_slot: Arc<Mutex<Option<PyErr>>> = Arc::new(Mutex::new(None));
    let src = CharsSource::PyIo(PyIoCharsIter::new(
        stream.clone().unbind(),
        error_slot.clone(),
    ));
    let inner = YamlIterInner {
        parser: Parser::new(src),
        builder: Builder::new(),
        policy,
        done: false,
        error_slot: Some(error_slot),
    };
    Py::new(py, PyYamlIter::new(inner, schema))
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
fn iter_loads_all(
    py: Python<'_>,
    text: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyYamlIter>> {
    use core::builder::Builder;
    use core::parser::Parser;

    let text = coerce_text(text)?;
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let policy = sb_borrow.as_deref().and_then(Schema::tag_policy);
    let src = CharsSource::Str(StringCharsIter::new(text));
    let inner = YamlIterInner {
        parser: Parser::new(src),
        builder: Builder::new(),
        policy,
        done: false,
        error_slot: None,
    };
    Py::new(py, PyYamlIter::new(inner, schema))
}

#[pyfunction]
#[pyo3(signature = (doc, stream, *, schema=None, indent=2))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn dump(
    doc: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
    indent: usize,
) -> PyResult<()> {
    let sb = schema.as_ref().map(|s| s.bind(doc.py()));
    emit_doc_to_stream(doc, sb, stream, indent)
}

#[pyfunction]
#[pyo3(signature = (doc, *, schema=None, indent=2))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn dumps(doc: &Bound<'_, PyAny>, schema: Option<Py<Schema>>, indent: usize) -> PyResult<String> {
    let sb = schema.as_ref().map(|s| s.bind(doc.py()));
    emit_doc_to_string(doc, sb, indent)
}

#[pyfunction]
#[pyo3(signature = (docs, stream, *, schema=None, indent=2))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn dump_all(
    py: Python<'_>,
    docs: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
    indent: usize,
) -> PyResult<()> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let items: Vec<Bound<'_, PyAny>> = docs.try_iter()?.collect::<PyResult<_>>()?;
    let n = items.len();
    let mut writer = PyStreamWriter::new(stream.clone().unbind());
    for (i, item) in items.iter().enumerate() {
        let (node, mut meta) = extract_doc_and_meta(item, sb)?;
        // Synthetic explicit_start so that multi-doc streams always emit `---`
        // separators (matches the emit_docs behaviour for batched emit).
        meta.explicit_start |= (n > 1 && i == 0) || i > 0;
        let _ = emit_docs_to(std::slice::from_ref(&node), &[meta], indent, &mut writer);
        if let Some(err) = writer.take_error() {
            return Err(err);
        }
    }
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (docs, *, schema=None, indent=2))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn dumps_all(
    py: Python<'_>,
    docs: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
    indent: usize,
) -> PyResult<String> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let items: Vec<Bound<'_, PyAny>> = docs.try_iter()?.collect::<PyResult<_>>()?;
    let (nodes, meta): (Vec<YamlNode>, Vec<builder::DocMetadata>) = items
        .iter()
        .map(|i| extract_doc_and_meta(i, sb))
        .collect::<PyResult<Vec<_>>>()?
        .into_iter()
        .unzip();
    Ok(emit_docs(&nodes, &meta, indent))
}

/// The yarutsk module (private implementation, re-exported via `yarutsk/__init__.py`).
#[pymodule]
fn _yarutsk(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("YarutskError", m.py().get_type::<YarutskError>())?;
    m.add("ParseError", m.py().get_type::<ParseError>())?;
    m.add("LoaderError", m.py().get_type::<LoaderError>())?;
    m.add("DumperError", m.py().get_type::<DumperError>())?;
    m.add_class::<Schema>()?;
    m.add_class::<PyYamlScalar>()?;
    m.add_class::<PyYamlMapping>()?;
    m.add_class::<PyYamlSequence>()?;
    m.add_class::<PyYamlIter>()?;
    m.add_function(wrap_pyfunction!(load, m)?)?;
    m.add_function(wrap_pyfunction!(loads, m)?)?;
    m.add_function(wrap_pyfunction!(load_all, m)?)?;
    m.add_function(wrap_pyfunction!(loads_all, m)?)?;
    m.add_function(wrap_pyfunction!(iter_load_all, m)?)?;
    m.add_function(wrap_pyfunction!(iter_loads_all, m)?)?;
    m.add_function(wrap_pyfunction!(dump, m)?)?;
    m.add_function(wrap_pyfunction!(dumps, m)?)?;
    m.add_function(wrap_pyfunction!(dump_all, m)?)?;
    m.add_function(wrap_pyfunction!(dumps_all, m)?)?;
    Ok(())
}
