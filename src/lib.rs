// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

mod core;
mod py;

use core::builder;
use core::emitter::emit_docs;
use core::types::YamlNode;
use py::convert::{
    DocMeta, extract_yaml_node, node_to_doc, parse_text, read_stream, write_to_stream,
};
use py::py_mapping::PyYamlMapping;
use py::py_scalar::PyYamlScalar;
use py::py_sequence::PyYamlSequence;
use py::schema::Schema;
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

/// Return true if the Python doc object has `explicit_start = True`.
fn get_explicit_start_flag(obj: &Bound<'_, PyAny>) -> bool {
    if let Ok(m) = obj.cast::<PyYamlMapping>() {
        return m.borrow().explicit_start;
    }
    if let Ok(s) = obj.cast::<PyYamlSequence>() {
        return s.borrow().explicit_start;
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return sc.explicit_start;
    }
    false
}

/// Return true if the Python doc object has `explicit_end = True`.
fn get_explicit_end_flag(obj: &Bound<'_, PyAny>) -> bool {
    if let Ok(m) = obj.cast::<PyYamlMapping>() {
        return m.borrow().explicit_end;
    }
    if let Ok(s) = obj.cast::<PyYamlSequence>() {
        return s.borrow().explicit_end;
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return sc.explicit_end;
    }
    false
}

fn get_yaml_version_flag(obj: &Bound<'_, PyAny>) -> Option<(u8, u8)> {
    if let Ok(m) = obj.cast::<PyYamlMapping>() {
        return m.borrow().yaml_version;
    }
    if let Ok(s) = obj.cast::<PyYamlSequence>() {
        return s.borrow().yaml_version;
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return sc.yaml_version;
    }
    None
}

fn get_tag_directives_flag(obj: &Bound<'_, PyAny>) -> Vec<(String, String)> {
    if let Ok(m) = obj.cast::<PyYamlMapping>() {
        return m.borrow().tag_directives.clone();
    }
    if let Ok(s) = obj.cast::<PyYamlSequence>() {
        return s.borrow().tag_directives.clone();
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return sc.tag_directives.clone();
    }
    vec![]
}

fn emit_doc_to_string(
    doc: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    indent: usize,
) -> PyResult<String> {
    let node = extract_yaml_node(doc, schema)?;
    Ok(emit_docs(
        std::slice::from_ref(&node),
        &[get_explicit_start_flag(doc)],
        &[get_explicit_end_flag(doc)],
        &[get_yaml_version_flag(doc)],
        &[get_tag_directives_flag(doc)],
        indent,
    ))
}

// ─── Module-level functions ───────────────────────────────────────────────────

#[pyfunction]
#[pyo3(signature = (stream, *, schema=None))]
fn load(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
) -> PyResult<Py<PyAny>> {
    let text = read_stream(stream)?;
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let mut out = parse_text(&text, sb_borrow.as_deref())?;
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
    let text = read_stream(stream)?;
    let sb = schema.as_ref().map(|s| s.bind(py));
    let sb_borrow = sb.map(|s| s.borrow());
    let mut out = parse_text(&text, sb_borrow.as_deref())?;
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
#[pyo3(signature = (doc, stream, *, schema=None, indent=2))]
fn dump(
    doc: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
    schema: Option<Py<Schema>>,
    indent: usize,
) -> PyResult<()> {
    let sb = schema.as_ref().map(|s| s.bind(doc.py()));
    write_to_stream(stream, &emit_doc_to_string(doc, sb, indent)?)
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
    let nodes: Vec<YamlNode> = items
        .iter()
        .map(|i| extract_yaml_node(i, sb))
        .collect::<PyResult<_>>()?;
    let starts: Vec<bool> = items.iter().map(|i| get_explicit_start_flag(i)).collect();
    let ends: Vec<bool> = items.iter().map(|i| get_explicit_end_flag(i)).collect();
    let versions: Vec<Option<(u8, u8)>> = items.iter().map(|i| get_yaml_version_flag(i)).collect();
    let tags: Vec<Vec<(String, String)>> =
        items.iter().map(|i| get_tag_directives_flag(i)).collect();
    write_to_stream(
        stream,
        &emit_docs(&nodes, &starts, &ends, &versions, &tags, indent),
    )
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
        .map(|i| extract_yaml_node(i, sb))
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
