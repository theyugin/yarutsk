// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

#[doc(hidden)]
pub mod core;
mod py;

use std::sync::{Arc, Mutex};

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
use py::streaming::{CharsSource, PyIoCharsIter, StringCharsIter};
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

// ─── Unified load / dump cores ────────────────────────────────────────────────

/// Source for `do_load` — either a Python stream-like object or owned text.
enum LoadSource<'py> {
    Stream(Bound<'py, PyAny>),
    Text(String),
}

/// Parse + convert one or all docs. Single place that binds the schema borrow,
/// dispatches the parser, and runs the Rust→Python conversion.
#[allow(clippy::needless_pass_by_value)] // matches caller signatures (PyO3 Option<Py<T>>)
fn do_load(
    py: Python<'_>,
    src: LoadSource<'_>,
    schema: Option<Py<Schema>>,
    all: bool,
) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let out = match src {
        LoadSource::Stream(s) => parse_stream(&s, sb_borrow.as_deref())?,
        LoadSource::Text(t) => parse_text(&t, sb_borrow.as_deref())?,
    };
    if out.docs.is_empty() && !all {
        return Ok(py.None());
    }
    if all {
        let builder::ParseOutput { docs, docs_meta } = out;
        let pydocs: Vec<Py<PyAny>> = docs
            .into_iter()
            .enumerate()
            .map(|(i, d)| node_to_doc(py, d, doc_meta(&docs_meta, i), sb))
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, pydocs)?.into_any().unbind())
    } else {
        let mut out = out;
        let meta = doc_meta(&out.docs_meta, 0);
        node_to_doc(py, out.docs.swap_remove(0), meta, sb)
    }
}

/// Build a streaming iterator from a `CharsSource` and schema.
fn make_iter(
    py: Python<'_>,
    src: CharsSource,
    schema: Option<Py<Schema>>,
    error_slot: Option<Arc<Mutex<Option<PyErr>>>>,
) -> PyResult<Py<PyYamlIter>> {
    use core::builder::Builder;
    use core::parser::Parser;

    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let policy = sb_borrow.as_deref().and_then(Schema::tag_policy);
    let inner = YamlIterInner {
        parser: Parser::new(src),
        builder: Builder::new(),
        policy,
        done: false,
        error_slot,
    };
    Py::new(py, PyYamlIter::new(inner, schema))
}

/// Sink for `do_dump_all` — stream out doc-by-doc, or accumulate into a string.
enum EmitSink<'py> {
    Stream(Bound<'py, PyAny>),
    String,
}

/// Emit a single doc to either sink. Returns `Some(string)` for `EmitSink::String`,
/// `None` for stream emission.
fn do_dump(
    doc: &Bound<'_, PyAny>,
    sink: EmitSink<'_>,
    schema: Option<&Bound<'_, Schema>>,
    indent: usize,
) -> PyResult<Option<String>> {
    let (node, meta) = extract_doc_and_meta(doc, schema)?;
    match sink {
        EmitSink::String => Ok(Some(emit_docs(
            std::slice::from_ref(&node),
            &[meta],
            indent,
        ))),
        EmitSink::Stream(stream) => {
            let mut writer = PyStreamWriter::new(stream.unbind());
            let _ = emit_docs_to(std::slice::from_ref(&node), &[meta], indent, &mut writer);
            if let Some(err) = writer.take_error() {
                return Err(err);
            }
            Ok(None)
        }
    }
}

/// Emit a sequence of docs to either sink. For `Stream`, emits doc-by-doc and
/// synthesises `---` separators. For `String`, batches via `emit_docs`.
fn do_dump_all(
    docs: &Bound<'_, PyAny>,
    sink: EmitSink<'_>,
    schema: Option<&Bound<'_, Schema>>,
    indent: usize,
) -> PyResult<Option<String>> {
    let items: Vec<Bound<'_, PyAny>> = docs.try_iter()?.collect::<PyResult<_>>()?;
    match sink {
        EmitSink::String => {
            let (nodes, meta): (Vec<YamlNode>, Vec<builder::DocMetadata>) = items
                .iter()
                .map(|i| extract_doc_and_meta(i, schema))
                .collect::<PyResult<Vec<_>>>()?
                .into_iter()
                .unzip();
            Ok(Some(emit_docs(&nodes, &meta, indent)))
        }
        EmitSink::Stream(stream) => {
            let n = items.len();
            let mut writer = PyStreamWriter::new(stream.unbind());
            for (i, item) in items.iter().enumerate() {
                let (node, mut meta) = extract_doc_and_meta(item, schema)?;
                // Synthetic explicit_start so multi-doc streams always emit `---`
                // separators (matches the emit_docs behaviour for batched emit).
                meta.explicit_start |= (n > 1 && i == 0) || i > 0;
                let _ = emit_docs_to(std::slice::from_ref(&node), &[meta], indent, &mut writer);
                if let Some(err) = writer.take_error() {
                    return Err(err);
                }
            }
            Ok(None)
        }
    }
}

// ─── Module-level functions ───────────────────────────────────────────────────

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn load(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    do_load(py, LoadSource::Stream(stream.clone()), schema, false)
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn loads(
    py: Python<'_>,
    text: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    do_load(py, LoadSource::Text(coerce_text(text)?), schema, false)
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn load_all(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    do_load(py, LoadSource::Stream(stream.clone()), schema, true)
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn loads_all(
    py: Python<'_>,
    text: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    do_load(py, LoadSource::Text(coerce_text(text)?), schema, true)
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn iter_load_all(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyYamlIter>> {
    let error_slot: Arc<Mutex<Option<PyErr>>> = Arc::new(Mutex::new(None));
    let src = CharsSource::PyIo(PyIoCharsIter::new(
        stream.clone().unbind(),
        error_slot.clone(),
    ));
    make_iter(py, src, schema, Some(error_slot))
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn iter_loads_all(
    py: Python<'_>,
    text: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyYamlIter>> {
    let text = coerce_text(text)?;
    let src = CharsSource::Str(StringCharsIter::new(text));
    make_iter(py, src, schema, None)
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
    do_dump(doc, EmitSink::Stream(stream.clone()), sb, indent)?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (doc, *, schema=None, indent=2))]
#[allow(clippy::needless_pass_by_value)] // pyfunction: PyO3 requires Option<Py<T>> by value
fn dumps(doc: &Bound<'_, PyAny>, schema: Option<Py<Schema>>, indent: usize) -> PyResult<String> {
    let sb = schema.as_ref().map(|s| s.bind(doc.py()));
    Ok(do_dump(doc, EmitSink::String, sb, indent)?.unwrap_or_default())
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
    do_dump_all(docs, EmitSink::Stream(stream.clone()), sb, indent)?;
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
    Ok(do_dump_all(docs, EmitSink::String, sb, indent)?.unwrap_or_default())
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
