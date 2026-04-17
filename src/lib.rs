// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

mod core;
mod py;

use core::builder;
use core::emitter::{emit_docs, emit_docs_to};
use core::types::YamlNode;
use py::convert::{
    DocMeta, clear_anchor_state, extract_yaml_node, init_anchor_state, node_to_doc, parse_stream,
    parse_text,
};
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

/// Build a `DocMeta` for document index `i` from a `ParseOutput`.
fn doc_meta(out: &mut builder::ParseOutput, i: usize) -> DocMeta {
    DocMeta {
        explicit_start: out.doc_explicit.get(i).copied().unwrap_or(false),
        explicit_end: out.doc_explicit_end.get(i).copied().unwrap_or(false),
        yaml_version: out.doc_yaml_version.get(i).and_then(|v| *v),
        tag_directives: out.doc_tag_directives.get(i).cloned().unwrap_or_default(),
    }
}

/// Extract a doc-level field from any of the three document types.
macro_rules! doc_field {
    ($name:ident -> $ret:ty : $field:ident, $default:expr) => {
        fn $name(obj: &Bound<'_, PyAny>) -> $ret {
            if let Ok(m) = obj.cast::<PyYamlMapping>() {
                return m.borrow().$field.clone();
            }
            if let Ok(s) = obj.cast::<PyYamlSequence>() {
                return s.borrow().$field.clone();
            }
            if let Ok(sc) = obj.extract::<PyYamlScalar>() {
                return sc.$field.clone();
            }
            $default
        }
    };
}

doc_field!(get_explicit_start_flag -> bool : explicit_start, false);
doc_field!(get_explicit_end_flag   -> bool : explicit_end,   false);
doc_field!(get_yaml_version_flag   -> Option<(u8, u8)> : yaml_version, None);
doc_field!(get_tag_directives_flag -> Vec<(String, String)> : tag_directives, vec![]);

/// Extract a YamlNode from a Python doc object, handling anchor state setup/teardown.
fn extract_doc_node(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    init_anchor_state(doc);
    let result = extract_yaml_node(doc, schema);
    clear_anchor_state();
    result
}

fn emit_doc_to_string(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    indent: usize,
) -> PyResult<String> {
    let node = extract_doc_node(doc, schema)?;
    Ok(emit_docs(
        std::slice::from_ref(&node),
        &[get_explicit_start_flag(doc)],
        &[get_explicit_end_flag(doc)],
        &[get_yaml_version_flag(doc)],
        &[get_tag_directives_flag(doc)],
        indent,
    ))
}

/// Emit a single document directly to a Python IO stream via [`PyStreamWriter`].
fn emit_doc_to_stream(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    stream: &Bound<'_, PyAny>,
    indent: usize,
) -> PyResult<()> {
    let node = extract_doc_node(doc, schema)?;
    let mut writer = PyStreamWriter::new(stream.clone().unbind());
    let _ = emit_docs_to(
        std::slice::from_ref(&node),
        &[get_explicit_start_flag(doc)],
        &[get_explicit_end_flag(doc)],
        &[get_yaml_version_flag(doc)],
        &[get_tag_directives_flag(doc)],
        indent,
        &mut writer,
    );
    if let Some(err) = writer.take_error() {
        return Err(err);
    }
    Ok(())
}

// ─── Module-level functions ───────────────────────────────────────────────────

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
fn load(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let mut out = parse_stream(stream, sb_borrow.as_deref())?;
    if out.docs.is_empty() {
        return Ok(py.None());
    }
    let meta = doc_meta(&mut out, 0);
    node_to_doc(py, out.docs.swap_remove(0), meta, sb)
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
fn loads(py: Python<'_>, text: &str, schema: Option<Py<Schema>>) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let mut out = parse_text(text, sb_borrow.as_deref())?;
    if out.docs.is_empty() {
        return Ok(py.None());
    }
    let meta = doc_meta(&mut out, 0);
    node_to_doc(py, out.docs.swap_remove(0), meta, sb)
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
fn load_all(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let mut out = parse_stream(stream, sb_borrow.as_deref())?;
    let pydocs: Vec<Py<PyAny>> = out
        .docs
        .drain(..)
        .enumerate()
        .map(|(i, d)| {
            let meta = DocMeta {
                explicit_start: out.doc_explicit.get(i).copied().unwrap_or(false),
                explicit_end: out.doc_explicit_end.get(i).copied().unwrap_or(false),
                yaml_version: out.doc_yaml_version.get(i).and_then(|v| *v),
                tag_directives: out.doc_tag_directives.get(i).cloned().unwrap_or_default(),
            };
            node_to_doc(py, d, meta, sb)
        })
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, pydocs)?.into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (text, *, schema=None))]
fn loads_all(py: Python<'_>, text: &str, schema: Option<Py<Schema>>) -> PyResult<Py<PyAny>> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let mut out = parse_text(text, sb_borrow.as_deref())?;
    let pydocs: Vec<Py<PyAny>> = out
        .docs
        .drain(..)
        .enumerate()
        .map(|(i, d)| {
            let meta = DocMeta {
                explicit_start: out.doc_explicit.get(i).copied().unwrap_or(false),
                explicit_end: out.doc_explicit_end.get(i).copied().unwrap_or(false),
                yaml_version: out.doc_yaml_version.get(i).and_then(|v| *v),
                tag_directives: out.doc_tag_directives.get(i).cloned().unwrap_or_default(),
            };
            node_to_doc(py, d, meta, sb)
        })
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, pydocs)?.into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
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
    text: String,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyYamlIter>> {
    use core::builder::Builder;
    use core::parser::Parser;

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
fn dumps(doc: &Bound<'_, PyAny>, schema: Option<Py<Schema>>, indent: usize) -> PyResult<String> {
    let sb = schema.as_ref().map(|s| s.bind(doc.py()));
    emit_doc_to_string(doc, sb, indent)
}

#[pyfunction]
#[pyo3(signature = (docs, stream, *, schema=None, indent=2))]
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
        init_anchor_state(item);
        let node_result = extract_yaml_node(item, sb);
        clear_anchor_state();
        let node = node_result?;
        // Pass `n > 1` via a synthetic explicit_start so that multi-doc streams
        // always emit `---` separators (matching the original emit_docs behaviour).
        let want_start = get_explicit_start_flag(item) || (n > 1 && i == 0) || i > 0;
        let _ = emit_docs_to(
            std::slice::from_ref(&node),
            &[want_start],
            &[get_explicit_end_flag(item)],
            &[get_yaml_version_flag(item)],
            &[get_tag_directives_flag(item)],
            indent,
            &mut writer,
        );
        if let Some(err) = writer.take_error() {
            return Err(err);
        }
    }
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (docs, *, schema=None, indent=2))]
fn dumps_all(
    py: Python<'_>,
    docs: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
    indent: usize,
) -> PyResult<String> {
    let sb = schema.as_ref().map(|s| s.bind(py));
    let items: Vec<Bound<'_, PyAny>> = docs.try_iter()?.collect::<PyResult<_>>()?;
    let nodes: Vec<YamlNode> = items
        .iter()
        .map(|i| {
            init_anchor_state(i);
            let node = extract_yaml_node(i, sb);
            clear_anchor_state();
            node
        })
        .collect::<PyResult<_>>()?;
    let starts: Vec<bool> = items.iter().map(|i| get_explicit_start_flag(i)).collect();
    let ends: Vec<bool> = items.iter().map(|i| get_explicit_end_flag(i)).collect();
    let versions: Vec<Option<(u8, u8)>> = items.iter().map(|i| get_yaml_version_flag(i)).collect();
    let tags: Vec<Vec<(String, String)>> =
        items.iter().map(|i| get_tag_directives_flag(i)).collect();
    Ok(emit_docs(&nodes, &starts, &ends, &versions, &tags, indent))
}

/// The yarutsk module.
#[pymodule]
fn yarutsk(m: &Bound<'_, PyModule>) -> PyResult<()> {
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
