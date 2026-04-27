// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Python ↔ `YamlNode` boundary.
//!
//! Two halves:
//! - **Load** (Rust → Python): `parse_text`/`parse_stream` drive the builder
//!   to produce pure `YamlNode` trees, then `node_to_doc` / `materialise_node`
//!   wraps each container subtree in the matching pyclass
//!   (`PyYamlMapping`/`PyYamlSequence`/`PyYamlScalar`) and stores the result
//!   as a `LiveNode` slot. Schema loaders fire on tagged scalars.
//! - **Dump** (Python → Rust): `extract_yaml_node` walks Python objects
//!   (typed pyclasses, plain dicts/lists, schema-dumper output, …), tracks
//!   anchor identity, and produces a pure `YamlNode` ready for the emitter.
//!
//! Identity model — every typed child surfaced via `node()` lives in the
//! parent's tree as `LiveNode::LivePy(Py<…>)`, so subsequent reads return
//! the same Py and setters land directly on the borrowed pyclass. Scalar
//! children are *lazily promoted* into that variant on first `node()`
//! access (see `map_child_node` / `seq_child_node`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyList, PyTuple, PyType};

use super::live::LiveNode;
use super::py_mapping::PyYamlMapping;
use super::py_scalar::PyYamlScalar;
use super::py_sequence::PyYamlSequence;
use super::schema::Schema;
use crate::core::builder::{DocMetadata, ParseOutput, parse_iter, parse_str};
use crate::core::types::{
    MapKey, Node, NodeMeta, ScalarStyle, ScalarValue, YamlEntry, YamlMapping, YamlNode, YamlScalar,
    YamlSequence,
};
use crate::{DumperError, LoaderError, ParseError};

static DATETIME_TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static DATE_TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

pub(crate) fn datetime_type(py: Python<'_>) -> PyResult<&Bound<'_, PyType>> {
    DATETIME_TYPE.import(py, "datetime", "datetime")
}

pub(crate) fn date_type(py: Python<'_>) -> PyResult<&Bound<'_, PyType>> {
    DATE_TYPE.import(py, "datetime", "date")
}

struct AnchorEmitState {
    anchors: HashMap<usize, Option<String>>,
    counter: usize,
}

impl AnchorEmitState {
    fn next_name(&mut self) -> String {
        self.counter += 1;
        format!("id{:03}", self.counter)
    }
}

/// Per-call load state: caches the Python object built for each anchor name so
/// that aliases can be returned as the *same* `Py<PyAny>`.
#[derive(Default)]
pub(crate) struct LoadCtx {
    anchors: HashMap<String, Py<PyAny>>,
}

impl LoadCtx {
    fn register(&mut self, name: String, py_obj: &Py<PyAny>, py: Python<'_>) {
        self.anchors.insert(name, py_obj.clone_ref(py));
    }

    fn lookup(&self, name: &str, py: Python<'_>) -> Option<Py<PyAny>> {
        self.anchors.get(name).map(|p| p.clone_ref(py))
    }
}

/// Per-call extraction state — one instance per top-level `extract_yaml_node`
/// or `py_to_node` call, threaded through every recursive callee.
#[derive(Default)]
pub(crate) struct EmitCtx {
    cycle_set: HashSet<usize>,
    anchors: Option<AnchorEmitState>,
}

impl EmitCtx {
    fn with_cycle<T>(
        &mut self,
        ptr: usize,
        cycle_err: impl FnOnce() -> PyErr,
        f: impl FnOnce(&mut Self) -> PyResult<T>,
    ) -> PyResult<T> {
        if !self.cycle_set.insert(ptr) {
            return Err(cycle_err());
        }
        let result = f(self);
        self.cycle_set.remove(&ptr);
        result
    }

    fn init_anchors(&mut self, doc: &Bound<'_, PyAny>) {
        let mut ref_count: HashMap<usize, usize> = HashMap::new();
        prepass(doc, &mut ref_count);
        let anchors = ref_count
            .into_iter()
            .filter(|(_, n)| *n > 1)
            .map(|(ptr, _)| (ptr, None))
            .collect();
        self.anchors = Some(AnchorEmitState {
            anchors,
            counter: 0,
        });
    }

    fn check_anchor(
        &mut self,
        ptr: usize,
        explicit_anchor: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        let Some(st) = self.anchors.as_mut() else {
            return (None, None);
        };
        if !st.anchors.contains_key(&ptr) {
            return (None, None);
        }
        let current = st.anchors.get(&ptr).and_then(std::clone::Clone::clone);
        if let Some(name) = current {
            return (Some(name), None);
        }
        let name = explicit_anchor.map_or_else(|| st.next_name(), str::to_owned);
        st.anchors.insert(ptr, Some(name.clone()));
        (None, Some(name))
    }
}

/// Count object-identity occurrences across plain Python containers and
/// our `PyYamlMapping` / `PyYamlSequence` trees. Objects seen more than
/// once — including self-cycles — get an anchor on emit.
fn prepass(obj: &Bound<'_, PyAny>, ref_count: &mut HashMap<usize, usize>) {
    let py = obj.py();
    if let Ok(m_bound) = obj.cast::<PyYamlMapping>() {
        let ptr = obj.as_ptr() as usize;
        if *ref_count.entry(ptr).or_insert(0) > 0 {
            return;
        }
        *ref_count.entry(ptr).or_insert(0) = 1;
        let m = m_bound.borrow();
        for entry in m.inner.entries.values() {
            if let LiveNode::LivePy(p) = &entry.value {
                prepass(p.bind(py), ref_count);
            }
        }
        return;
    }
    if let Ok(s_bound) = obj.cast::<PyYamlSequence>() {
        let ptr = obj.as_ptr() as usize;
        if *ref_count.entry(ptr).or_insert(0) > 0 {
            return;
        }
        *ref_count.entry(ptr).or_insert(0) = 1;
        let s = s_bound.borrow();
        for item in &s.inner.items {
            if let LiveNode::LivePy(p) = item {
                prepass(p.bind(py), ref_count);
            }
        }
        return;
    }
    let is_dict = obj.cast::<PyDict>().is_ok();
    let is_list = !is_dict && obj.cast::<PyList>().is_ok();
    let is_tuple = !is_dict && !is_list && obj.cast::<PyTuple>().is_ok();
    if !is_dict && !is_list && !is_tuple {
        return;
    }
    let ptr = obj.as_ptr() as usize;
    let count = ref_count.entry(ptr).or_insert(0);
    *count += 1;
    if *count > 1 {
        return;
    }
    if is_dict {
        if let Ok(d) = obj.cast::<PyDict>() {
            for (_, v) in d.iter() {
                prepass(&v, ref_count);
            }
        }
    } else if is_list {
        if let Ok(l) = obj.cast::<PyList>() {
            for item in l.iter() {
                prepass(&item, ref_count);
            }
        }
    } else if let Ok(t) = obj.cast::<PyTuple>() {
        for item in t.iter() {
            prepass(&item, ref_count);
        }
    }
}

pub(crate) fn scalar_to_py(py: Python<'_>, v: &ScalarValue) -> PyResult<Py<PyAny>> {
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

fn lookup_loader(
    py: Python<'_>,
    schema: Option<&Bound<'_, Schema>>,
    tag: Option<&str>,
) -> Option<Py<PyAny>> {
    let schema_bound = schema?;
    let t = tag?;
    let sr = schema_bound.borrow();
    sr.loaders.get(t).map(|f| f.clone_ref(py))
}

fn call_loader(
    py: Python<'_>,
    loader_fn: &Py<PyAny>,
    tag: &str,
    arg: Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    loader_fn
        .bind(py)
        .call1((arg,))
        .map(pyo3::Bound::unbind)
        .map_err(|e| LoaderError::new_err(format!("Schema loader for tag '{tag}' raised: {e}")))
}

/// Convert a `YamlScalar` to Python, applying tag-specific conversions first.
pub(crate) fn scalar_to_py_with_tag(
    py: Python<'_>,
    s: &YamlScalar,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    if let Some(loader_fn) = lookup_loader(py, schema, s.meta.tag.as_deref()) {
        let tag_name = s.meta.tag.as_deref().unwrap_or("?");
        let default_val = scalar_to_py(py, s.value())?;
        return call_loader(py, &loader_fn, tag_name, default_val);
    }
    match s.meta.tag.as_deref() {
        Some("!!binary" | "tag:yaml.org,2002:binary") => {
            use base64::{Engine, engine::general_purpose::STANDARD};
            use pyo3::types::PyBytes;
            let raw = s
                .original()
                .or(if let ScalarValue::Str(st) = s.value() {
                    Some(st.as_str())
                } else {
                    None
                })
                .unwrap_or("");
            let stripped: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = STANDARD
                .decode(stripped.as_bytes())
                .map_err(|e| PyRuntimeError::new_err(format!("!!binary decode error: {e}")))?;
            Ok(PyBytes::new(py, &bytes).into_any().unbind())
        }
        Some("!!timestamp" | "tag:yaml.org,2002:timestamp") => {
            let raw = s
                .original()
                .or(if let ScalarValue::Str(st) = s.value() {
                    Some(st.as_str())
                } else {
                    None
                })
                .unwrap_or("");
            let normalized = raw.replacen(' ', "T", 1);
            if !normalized.contains('T') && normalized.len() > 5 && !normalized[5..].contains(':') {
                let date = date_type(py)?.call_method1("fromisoformat", (&*normalized,))?;
                Ok(date.into_any().unbind())
            } else {
                let dt = datetime_type(py)?.call_method1("fromisoformat", (&*normalized,))?;
                Ok(dt.into_any().unbind())
            }
        }
        _ => scalar_to_py(py, s.value()),
    }
}

/// Construct a plain (unquoted, untagged) `YamlScalar` from a typed value.
#[must_use]
pub(crate) fn plain_yaml_scalar(value: ScalarValue) -> YamlScalar {
    YamlScalar {
        value,
        source: None,
        style: ScalarStyle::Plain,
        chomping: None,
        meta: NodeMeta::default(),
    }
}

/// A `YamlEntry` with plain key style.
pub(crate) fn plain_entry<N: Node>(value: N) -> YamlEntry<N> {
    YamlEntry {
        value,
        key_style: ScalarStyle::Plain,
        key_anchor: None,
        key_alias: None,
        key_tag: None,
        key_node: None,
    }
}

/// Read `(comment_inline, comment_before, blank_lines_before)` off a stored
/// live slot. For `Container`, the metadata lives on the wrapped Py.
pub(crate) fn read_metadata(node: &LiveNode) -> (Option<String>, Option<String>, u8) {
    if let LiveNode::LivePy(p) = node {
        return Python::attach(|py| {
            let bound = p.bind(py);
            if let Ok(m) = bound.cast::<PyYamlMapping>() {
                let m = m.borrow();
                return (
                    m.inner.meta.comment_inline.clone(),
                    m.inner.meta.comment_before.clone(),
                    m.inner.meta.blank_lines_before,
                );
            }
            if let Ok(s) = bound.cast::<PyYamlSequence>() {
                let s = s.borrow();
                return (
                    s.inner.meta.comment_inline.clone(),
                    s.inner.meta.comment_before.clone(),
                    s.inner.meta.blank_lines_before,
                );
            }
            if let Ok(sc) = bound.cast::<PyYamlScalar>() {
                let sc = sc.borrow();
                return (
                    sc.inner.comment_inline().map(str::to_owned),
                    sc.inner.comment_before().map(str::to_owned),
                    sc.inner.blank_lines_before(),
                );
            }
            (None, None, 0)
        });
    }
    (
        node.comment_inline().map(str::to_owned),
        node.comment_before().map(str::to_owned),
        node.blank_lines_before(),
    )
}

/// Carry inline/before-comment and `blank_lines_before` from a previous slot
/// onto a freshly-converted slot (only when the new slot didn't bring its own).
pub(crate) fn carry_metadata(
    node: &mut LiveNode,
    old_inline: Option<String>,
    old_before: Option<String>,
    old_blanks: u8,
) {
    if let LiveNode::LivePy(p) = node {
        Python::attach(|py| {
            let bound = p.bind(py);
            if let Ok(m) = bound.cast::<PyYamlMapping>() {
                carry_into_meta(
                    &mut m.borrow_mut().inner.meta,
                    old_inline,
                    old_before,
                    old_blanks,
                );
            } else if let Ok(s) = bound.cast::<PyYamlSequence>() {
                carry_into_meta(
                    &mut s.borrow_mut().inner.meta,
                    old_inline,
                    old_before,
                    old_blanks,
                );
            } else if let Ok(sc) = bound.cast::<PyYamlScalar>() {
                carry_into_node(
                    &mut sc.borrow_mut().inner,
                    old_inline,
                    old_before,
                    old_blanks,
                );
            }
        });
        return;
    }
    carry_into_node(node, old_inline, old_before, old_blanks);
}

/// Apply the carry-rule (only fill missing slots) to a `NodeMeta`.
fn carry_into_meta(
    meta: &mut NodeMeta,
    inline: Option<String>,
    before: Option<String>,
    blanks: u8,
) {
    if meta.comment_inline.is_none() {
        meta.comment_inline = inline;
    }
    if meta.comment_before.is_none() {
        meta.comment_before = before;
    }
    if meta.blank_lines_before == 0 {
        meta.blank_lines_before = blanks;
    }
}

/// Same carry-rule, applied through the `Node` trait so it works uniformly
/// for `LiveNode`, `YamlNode`, and any other `Node` impl.
fn carry_into_node<N: Node>(
    node: &mut N,
    inline: Option<String>,
    before: Option<String>,
    blanks: u8,
) {
    if node.comment_inline().is_none() {
        node.set_comment_inline(inline);
    }
    if node.comment_before().is_none() {
        node.set_comment_before(before);
    }
    if node.blank_lines_before() == 0 {
        node.set_blank_lines_before(blanks);
    }
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
pub(crate) fn resolve_seq_idx(idx: isize, len: usize) -> PyResult<usize> {
    let len_i = len as isize;
    let real = if idx < 0 { len_i + idx } else { idx };
    if real < 0 || real >= len_i {
        return Err(pyo3::exceptions::PyIndexError::new_err(format!(
            "index {idx} is out of range for sequence of length {len}"
        )));
    }
    Ok(real as usize)
}

/// Convert a Python value into a `LiveNode` for storage in a pyclass entry/item.
/// Containers are materialised into `LivePy(Py<PyYamlMapping|PyYamlSequence>)`;
/// anything `py_to_node` can't convert lands as `LivePy(value)` (typed-vs-opaque
/// is decided by downcast at access sites).
pub(crate) fn py_to_stored_node(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<LiveNode> {
    let Ok(node) = py_to_node(value, schema) else {
        return Ok(LiveNode::LivePy(value.clone().unbind()));
    };
    let mut ctx = LoadCtx::default();
    materialise_node(py, node, schema, &mut ctx)
}

/// Convert a `LiveNode` to its Python representation for `m['k']` access.
pub(crate) fn node_to_py(
    py: Python<'_>,
    node: &LiveNode,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    let mut ctx = LoadCtx::default();
    node_to_py_inner(py, node, schema, &mut ctx)
}

pub(crate) fn node_to_py_inner(
    py: Python<'_>,
    node: &LiveNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    match node {
        LiveNode::Scalar(s) => scalar_to_py_with_tag(py, s, schema),
        LiveNode::Alias {
            name,
            resolved,
            materialised,
            ..
        } => {
            if let Some(p) = materialised.as_ref() {
                return Ok(p.clone_ref(py));
            }
            if let Some(cached) = ctx.lookup(name, py) {
                return Ok(cached);
            }
            yamlnode_to_py_inner(py, resolved, schema, ctx)
        }
        LiveNode::LivePy(p) => {
            let bound = p.bind(py);
            if let Ok(sc) = bound.cast::<PyYamlScalar>() {
                let borrow = sc.borrow();
                return pyyamlscalar_to_py(py, &borrow.inner, schema);
            }
            Ok(p.clone_ref(py))
        }
    }
}

/// Resolve a `PyYamlScalar.inner` (which is `YamlNode::Scalar` or
/// `YamlNode::Alias`) to its Python value.
fn pyyamlscalar_to_py(
    py: Python<'_>,
    n: &YamlNode,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    match n {
        YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, schema),
        YamlNode::Alias { resolved, .. } => match resolved.as_ref() {
            YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, schema),
            _ => Ok(py.None()),
        },
        _ => Ok(py.None()),
    }
}

/// Convert a pure `YamlNode` to Python. Used by `node_to_doc` for top-level
/// docs and by alias materialisation. Builds container subtrees as new Pys.
fn yamlnode_to_py_inner(
    py: Python<'_>,
    node: &YamlNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, schema),
        YamlNode::Mapping(m) => {
            let tag = m.meta.tag.clone();
            let anchor = m.meta.anchor.clone();
            let py_obj =
                mapping_to_py_obj_inner(py, m.clone(), DocMetadata::default(), schema, ctx)?;
            let final_obj = apply_loader(py, schema, tag.as_deref(), py_obj)?;
            if let Some(name) = anchor {
                ctx.register(name, &final_obj, py);
            }
            Ok(final_obj)
        }
        YamlNode::Sequence(s) => {
            let tag = s.meta.tag.clone();
            let anchor = s.meta.anchor.clone();
            let py_obj =
                sequence_to_py_obj_inner(py, s.clone(), DocMetadata::default(), schema, ctx)?;
            let final_obj = apply_loader(py, schema, tag.as_deref(), py_obj)?;
            if let Some(name) = anchor {
                ctx.register(name, &final_obj, py);
            }
            Ok(final_obj)
        }
        YamlNode::Alias { name, resolved, .. } => {
            if let Some(cached) = ctx.lookup(name, py) {
                return Ok(cached);
            }
            yamlnode_to_py_inner(py, resolved, schema, ctx)
        }
    }
}

/// If *schema* has a loader for *tag*, call it with *`py_obj`* and return the result.
pub(crate) fn apply_loader(
    py: Python<'_>,
    schema: Option<&Bound<'_, Schema>>,
    tag: Option<&str>,
    py_obj: Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    if let Some(loader_fn) = lookup_loader(py, schema, tag) {
        return call_loader(py, &loader_fn, tag.unwrap_or("?"), py_obj);
    }
    Ok(py_obj)
}

/// Convert a Python primitive (None/bool/int/float/str) to a `YamlScalar`.
/// Returns `None` if *obj* is not a recognised primitive type.
pub(crate) fn py_primitive_to_scalar(obj: &Bound<'_, PyAny>) -> Option<YamlScalar> {
    if obj.is_none() {
        return Some(plain_yaml_scalar(ScalarValue::Null));
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Some(plain_yaml_scalar(ScalarValue::Bool(b)));
    }
    if let Ok(n) = obj.extract::<i64>() {
        return Some(plain_yaml_scalar(ScalarValue::Int(n)));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Some(plain_yaml_scalar(ScalarValue::Float(f)));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Some(plain_yaml_scalar(ScalarValue::Str(s)));
    }
    None
}

/// Convert a Python object to a pure `YamlNode`.
pub(crate) fn py_to_node(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    let mut ctx = EmitCtx::default();
    py_to_node_inner(obj, schema, &mut ctx)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn py_to_node_inner(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    if let Some(schema_bound) = schema {
        let match_result: Option<(Py<PyAny>, Py<PyAny>)> = {
            let sr = schema_bound.borrow();
            sr.dumpers.iter().find_map(|(py_type, fn_obj)| {
                if obj.is_instance(py_type.bind(obj.py())).unwrap_or(false) {
                    Some((py_type.clone_ref(obj.py()), fn_obj.clone_ref(obj.py())))
                } else {
                    None
                }
            })
        };
        if let Some((_, dumper_fn)) = match_result {
            let type_name = obj
                .get_type()
                .qualname()
                .map_or_else(|_| "?".to_string(), |n| n.to_string());
            let call_result = dumper_fn.bind(obj.py()).call1((obj,)).map_err(|e| {
                DumperError::new_err(format!("Schema dumper for {type_name} raised: {e}"))
            })?;
            let (tag, data): (String, Bound<'_, PyAny>) = call_result.extract().map_err(|e| {
                DumperError::new_err(format!(
                    "Schema dumper for {type_name} must return (tag, data) tuple: {e}"
                ))
            })?;
            let mut node = py_to_node_inner(&data, schema, ctx)?;
            match &mut node {
                YamlNode::Scalar(s) => s.meta.tag = Some(tag),
                YamlNode::Mapping(m) => m.meta.tag = Some(tag),
                YamlNode::Sequence(s) => s.meta.tag = Some(tag),
                YamlNode::Alias { .. } => {}
            }
            return Ok(node);
        }
    }

    // Pyclass values: extract the Rust-side data so styles/comments survive.
    if let Ok(m) = obj.cast::<PyYamlMapping>() {
        return livenode_mapping_to_yamlnode(m, schema, ctx);
    }
    if let Ok(s) = obj.cast::<PyYamlSequence>() {
        return livenode_sequence_to_yamlnode(s, schema, ctx);
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    if let Some(scalar) = py_primitive_to_scalar(obj) {
        return Ok(YamlNode::Scalar(scalar));
    }
    if let Ok(t) = obj.cast::<PyTuple>() {
        let ptr = obj.as_ptr() as usize;
        return ctx.with_cycle(
            ptr,
            || {
                pyo3::exceptions::PyValueError::new_err(
                    "cannot serialize a recursive structure: self-referential tuple detected",
                )
            },
            |ctx| {
                let mut seq = YamlSequence::<YamlNode>::new();
                for item in t.iter() {
                    seq.items.push(py_to_node_inner(&item, schema, ctx)?);
                }
                Ok(YamlNode::Sequence(seq))
            },
        );
    }
    if let Ok(l) = obj.cast::<PyList>() {
        let ptr = obj.as_ptr() as usize;
        return ctx.with_cycle(
            ptr,
            || {
                pyo3::exceptions::PyValueError::new_err(
                    "cannot serialize a recursive structure: self-referential list detected",
                )
            },
            |ctx| {
                let mut seq = YamlSequence::<YamlNode>::new();
                for item in l.iter() {
                    seq.items.push(py_to_node_inner(&item, schema, ctx)?);
                }
                Ok(YamlNode::Sequence(seq))
            },
        );
    }
    if (obj.is_instance_of::<pyo3::types::PyBytes>()
        || obj.is_instance_of::<pyo3::types::PyByteArray>())
        && let Ok(b) = obj.extract::<Vec<u8>>()
    {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let encoded = STANDARD.encode(&b);
        return Ok(YamlNode::Scalar(YamlScalar {
            value: ScalarValue::Str(encoded),
            source: None,
            style: ScalarStyle::Plain,
            chomping: None,
            meta: NodeMeta {
                tag: Some("!!binary".to_owned()),
                ..NodeMeta::default()
            },
        }));
    }
    {
        let py = obj.py();
        if obj.is_instance(datetime_type(py)?)? || obj.is_instance(date_type(py)?)? {
            let iso: String = obj.call_method0("isoformat")?.extract()?;
            return Ok(YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str(iso),
                source: None,
                style: ScalarStyle::Plain,
                chomping: None,
                meta: NodeMeta {
                    tag: Some("!!timestamp".to_owned()),
                    ..NodeMeta::default()
                },
            }));
        }
    }
    if let Ok(d) = obj.cast::<PyDict>() {
        let ptr = obj.as_ptr() as usize;
        return ctx.with_cycle(
            ptr,
            || {
                pyo3::exceptions::PyValueError::new_err(
                    "cannot serialize a recursive structure: self-referential dict detected",
                )
            },
            |ctx| {
                let mut mapping = YamlMapping::<YamlNode>::new();
                for (k, v) in d.iter() {
                    let key: String = k.extract()?;
                    mapping.entries.insert(
                        MapKey::Scalar(key),
                        plain_entry(py_to_node_inner(&v, schema, ctx)?),
                    );
                }
                Ok(YamlNode::Mapping(mapping))
            },
        );
    }
    {
        let abc = obj.py().import("collections.abc")?;
        let mapping_type = abc.getattr("Mapping")?;
        if obj.is_instance(&mapping_type)? {
            let items = obj.call_method0("items")?;
            let mut mapping = YamlMapping::<YamlNode>::new();
            for pair in items.try_iter()? {
                let pair = pair?;
                let key: String = pair.get_item(0)?.extract()?;
                let val = pair.get_item(1)?;
                mapping.entries.insert(
                    MapKey::Scalar(key),
                    plain_entry(py_to_node_inner(&val, schema, ctx)?),
                );
            }
            return Ok(YamlNode::Mapping(mapping));
        }
    }
    if obj.try_iter().is_ok() {
        let mut seq = YamlSequence::<YamlNode>::new();
        for item in obj.try_iter()? {
            let item = item?;
            seq.items.push(py_to_node_inner(&item, schema, ctx)?);
        }
        return Ok(YamlNode::Sequence(seq));
    }
    Err(PyRuntimeError::new_err(format!(
        "Cannot convert {obj} to a YAML node"
    )))
}

/// Build a synthetic alias node — used by `extract_yaml_node_inner` when
/// auto-anchor revisits a Py it has already seen.
fn synthetic_alias(name: String) -> YamlNode {
    YamlNode::Alias {
        name,
        resolved: Arc::new(YamlNode::Scalar(YamlScalar::null())),
        meta: NodeMeta::default(),
    }
}

/// Visit every typed yarutsk pyclass child in *children*, casting each to its
/// concrete pyclass and invoking *visit*. Opaque (non-yarutsk) `LivePy` values
/// are silently skipped — recursive container walks only descend into typed
/// children.
pub(crate) fn for_each_live_child<F>(
    py: Python<'_>,
    children: Vec<Py<PyAny>>,
    mut visit: F,
) -> PyResult<()>
where
    F: FnMut(ChildContainer<'_, '_>) -> PyResult<()>,
{
    for child in children {
        let bound = child.bind(py);
        if let Ok(m) = bound.cast::<PyYamlMapping>() {
            visit(ChildContainer::Mapping(m))?;
        } else if let Ok(s) = bound.cast::<PyYamlSequence>() {
            visit(ChildContainer::Sequence(s))?;
        } else if let Ok(sc) = bound.cast::<PyYamlScalar>() {
            visit(ChildContainer::Scalar(sc))?;
        }
    }
    Ok(())
}

pub(crate) enum ChildContainer<'py, 'a> {
    Mapping(&'a Bound<'py, PyYamlMapping>),
    Sequence(&'a Bound<'py, PyYamlSequence>),
    Scalar(&'a Bound<'py, PyYamlScalar>),
}

pub(crate) fn collect_live_children_from_mapping(
    m: &YamlMapping<LiveNode>,
    py: Python<'_>,
) -> Vec<Py<PyAny>> {
    m.entries
        .values()
        .filter_map(|e| match &e.value {
            LiveNode::LivePy(p) => Some(p.clone_ref(py)),
            _ => None,
        })
        .collect()
}

pub(crate) fn collect_live_children_from_sequence(
    s: &YamlSequence<LiveNode>,
    py: Python<'_>,
) -> Vec<Py<PyAny>> {
    s.items
        .iter()
        .filter_map(|item| match item {
            LiveNode::LivePy(p) => Some(p.clone_ref(py)),
            _ => None,
        })
        .collect()
}

/// Build a fresh `Py<PyYamlScalar>` from a scalar/alias live slot. Used by
/// `map_child_node` / `seq_child_node` to promote scalar children on first
/// `node()` access. For an alias slot, the wrapped `YamlNode::Alias` is stored
/// verbatim so the alias name and meta survive round-trip.
fn promote_scalar(py: Python<'_>, slot: LiveNode) -> PyResult<Py<PyAny>> {
    let inner = match slot {
        LiveNode::Scalar(s) => YamlNode::Scalar(s),
        LiveNode::Alias {
            name,
            resolved,
            meta,
            ..
        } => YamlNode::Alias {
            name,
            resolved,
            meta,
        },
        LiveNode::LivePy(_) => YamlNode::Scalar(YamlScalar::null()),
    };
    let scalar = PyYamlScalar { inner };
    let py_obj = Py::new(py, (scalar, crate::py::py_node::PyYamlNode::default()))?;
    Ok(py_obj.into_any())
}

/// Return the child at *key* of a mapping as a typed node object.
pub(crate) fn map_child_node(slf: &Bound<'_, PyYamlMapping>, key: &str) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let mk = MapKey::scalar(key);
    let is_scalar = match slf.borrow().inner.entries.get(&mk) {
        Some(entry) => entry.value.is_scalar_child(),
        None => return Err(PyKeyError::new_err(key.to_owned())),
    };
    if is_scalar {
        let mut borrow = slf.borrow_mut();
        let entry = borrow
            .inner
            .entries
            .get_mut(&mk)
            .ok_or_else(|| PyKeyError::new_err(key.to_owned()))?;
        let slot = std::mem::replace(&mut entry.value, LiveNode::Scalar(YamlScalar::null()));
        let py_obj = promote_scalar(py, slot)?;
        entry.value = LiveNode::LivePy(py_obj.clone_ref(py));
        return Ok(py_obj);
    }
    let borrow = slf.borrow();
    match borrow.inner.entries.get(&mk).map(|e| &e.value) {
        Some(LiveNode::LivePy(p)) => Ok(p.clone_ref(py)),
        Some(other) => node_to_py(py, other, None),
        None => Err(PyKeyError::new_err(key.to_owned())),
    }
}

/// Return the item at *idx* of a sequence as a typed node object.
pub(crate) fn seq_child_node(slf: &Bound<'_, PyYamlSequence>, idx: usize) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let is_scalar = slf.borrow().inner.items[idx].is_scalar_child();
    if is_scalar {
        let mut borrow = slf.borrow_mut();
        let slot_ref = &mut borrow.inner.items[idx];
        let slot = std::mem::replace(slot_ref, LiveNode::Scalar(YamlScalar::null()));
        let py_obj = promote_scalar(py, slot)?;
        *slot_ref = LiveNode::LivePy(py_obj.clone_ref(py));
        return Ok(py_obj);
    }
    let borrow = slf.borrow();
    match &borrow.inner.items[idx] {
        LiveNode::LivePy(p) => Ok(p.clone_ref(py)),
        other => node_to_py(py, other, None),
    }
}

/// Convert a top-level `YamlNode` to `PyYamlMapping`, `PyYamlSequence`, or
/// `PyYamlScalar`. Constructs a fresh `LoadCtx` so anchored containers and
/// the aliases pointing to them surface as the *same* Python object.
pub(crate) fn node_to_doc(
    py: Python<'_>,
    node: YamlNode,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    let mut ctx = LoadCtx::default();
    match node {
        YamlNode::Mapping(m) => {
            let anchor = m.meta.anchor.clone();
            let py_obj = mapping_to_py_obj_inner(py, m, meta, schema, &mut ctx)?;
            if let Some(name) = anchor {
                ctx.register(name, &py_obj, py);
            }
            Ok(py_obj)
        }
        YamlNode::Sequence(s) => {
            let anchor = s.meta.anchor.clone();
            let py_obj = sequence_to_py_obj_inner(py, s, meta, schema, &mut ctx)?;
            if let Some(name) = anchor {
                ctx.register(name, &py_obj, py);
            }
            Ok(py_obj)
        }
        YamlNode::Scalar(_) => {
            let scalar = PyYamlScalar { inner: node };
            let base = crate::py::py_node::PyYamlNode { meta };
            Ok(Py::new(py, (scalar, base))?.into_any())
        }
        YamlNode::Alias { resolved, .. } => {
            // Top-level alias is unusual. Surface the resolved subtree as a
            // standalone doc.
            node_to_doc(py, (*resolved).clone(), meta, schema)
        }
    }
}

/// Extract a `YamlNode` from a `PyYamlMapping`, `PyYamlSequence`, or `PyYamlScalar`.
pub(crate) fn extract_yaml_node(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    let mut ctx = EmitCtx::default();
    ctx.init_anchors(obj);
    extract_yaml_node_inner(obj, schema, &mut ctx)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn extract_yaml_node_inner(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    if let Ok(bound_m) = obj.cast::<PyYamlMapping>() {
        let ptr = obj.as_ptr() as usize;
        let explicit = bound_m.borrow().inner.meta.anchor.clone();
        let (alias, assigned_anchor) = ctx.check_anchor(ptr, explicit.as_deref());
        if let Some(alias_name) = alias {
            return Ok(synthetic_alias(alias_name));
        }
        return ctx.with_cycle(
            ptr,
            || PyRuntimeError::new_err("self-referential structure detected"),
            |ctx| extract_mapping_inner(bound_m, schema, ctx, assigned_anchor),
        );
    }
    if let Ok(bound_s) = obj.cast::<PyYamlSequence>() {
        let ptr = obj.as_ptr() as usize;
        let explicit = bound_s.borrow().inner.meta.anchor.clone();
        let (alias, assigned_anchor) = ctx.check_anchor(ptr, explicit.as_deref());
        if let Some(alias_name) = alias {
            return Ok(synthetic_alias(alias_name));
        }
        return ctx.with_cycle(
            ptr,
            || PyRuntimeError::new_err("self-referential structure detected"),
            |ctx| extract_sequence_inner(bound_s, schema, ctx, assigned_anchor),
        );
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    if let Some(scalar) = py_primitive_to_scalar(obj) {
        return Ok(YamlNode::Scalar(scalar));
    }
    if let Ok(d) = obj.cast::<PyDict>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = ctx.check_anchor(ptr, None);
        if let Some(name) = alias {
            return Ok(synthetic_alias(name));
        }
        let mut mapping = YamlMapping::<YamlNode>::new();
        if let Some(ref name) = anchor {
            mapping.meta.anchor = Some(name.clone());
        }
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            mapping.entries.insert(
                MapKey::Scalar(key),
                plain_entry(extract_yaml_node_inner(&v, schema, ctx)?),
            );
        }
        return Ok(YamlNode::Mapping(mapping));
    }
    if let Ok(l) = obj.cast::<PyList>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = ctx.check_anchor(ptr, None);
        if let Some(name) = alias {
            return Ok(synthetic_alias(name));
        }
        let mut seq = YamlSequence::<YamlNode>::new();
        if let Some(ref name) = anchor {
            seq.meta.anchor = Some(name.clone());
        }
        for item in l.iter() {
            seq.items.push(extract_yaml_node_inner(&item, schema, ctx)?);
        }
        return Ok(YamlNode::Sequence(seq));
    }
    if let Ok(t) = obj.cast::<PyTuple>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = ctx.check_anchor(ptr, None);
        if let Some(name) = alias {
            return Ok(synthetic_alias(name));
        }
        let mut seq = YamlSequence::<YamlNode>::new();
        if let Some(ref name) = anchor {
            seq.meta.anchor = Some(name.clone());
        }
        for item in t.iter() {
            seq.items.push(extract_yaml_node_inner(&item, schema, ctx)?);
        }
        return Ok(YamlNode::Sequence(seq));
    }
    py_to_node_inner(obj, schema, ctx)
}

fn extract_mapping_inner(
    bound_m: &Bound<'_, PyYamlMapping>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
    assigned_anchor: Option<String>,
) -> PyResult<YamlNode> {
    let py = bound_m.py();
    let borrow = bound_m.borrow();
    let mut mapping = YamlMapping::<YamlNode>::with_capacity(borrow.inner.entries.len());
    mapping.style = borrow.inner.style;
    mapping.meta.tag.clone_from(&borrow.inner.meta.tag);
    mapping.meta.anchor = assigned_anchor.or_else(|| borrow.inner.meta.anchor.clone());
    mapping.trailing_blank_lines = borrow.inner.trailing_blank_lines;
    mapping
        .meta
        .comment_inline
        .clone_from(&borrow.inner.meta.comment_inline);
    mapping
        .meta
        .comment_before
        .clone_from(&borrow.inner.meta.comment_before);
    mapping.meta.blank_lines_before = borrow.inner.meta.blank_lines_before;
    for (k, e) in &borrow.inner.entries {
        let value = extract_entry_value(py, &e.value, schema, ctx)?;
        mapping.entries.insert(
            k.clone(),
            YamlEntry {
                value,
                key_style: e.key_style,
                key_anchor: e.key_anchor.clone(),
                key_alias: e.key_alias.clone(),
                key_tag: e.key_tag.clone(),
                key_node: e.key_node.clone(),
            },
        );
    }
    Ok(YamlNode::Mapping(mapping))
}

fn extract_sequence_inner(
    bound_s: &Bound<'_, PyYamlSequence>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
    assigned_anchor: Option<String>,
) -> PyResult<YamlNode> {
    let py = bound_s.py();
    let borrow = bound_s.borrow();
    let mut seq = YamlSequence::<YamlNode>::with_capacity(borrow.inner.items.len());
    seq.style = borrow.inner.style;
    seq.meta.tag.clone_from(&borrow.inner.meta.tag);
    seq.meta.anchor = assigned_anchor.or_else(|| borrow.inner.meta.anchor.clone());
    seq.trailing_blank_lines = borrow.inner.trailing_blank_lines;
    seq.meta
        .comment_inline
        .clone_from(&borrow.inner.meta.comment_inline);
    seq.meta
        .comment_before
        .clone_from(&borrow.inner.meta.comment_before);
    seq.meta.blank_lines_before = borrow.inner.meta.blank_lines_before;
    for item in &borrow.inner.items {
        seq.items.push(extract_entry_value(py, item, schema, ctx)?);
    }
    Ok(YamlNode::Sequence(seq))
}

/// Convert a `LiveNode` to its pure `YamlNode` form. Used by `set_alias` and
/// other places that need to capture the current tree shape without anchor
/// tracking. Cycles inside `Container` Pys still trigger the cycle guard.
pub(crate) fn live_to_yamlnode(py: Python<'_>, value: &LiveNode) -> PyResult<YamlNode> {
    let mut ctx = EmitCtx::default();
    extract_entry_value(py, value, None, &mut ctx)
}

/// Resolve a stored live entry/item value into the pure `YamlNode` form.
pub(crate) fn extract_entry_value(
    py: Python<'_>,
    value: &LiveNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    match value {
        LiveNode::LivePy(p) => extract_yaml_node_inner(p.bind(py), schema, ctx),
        LiveNode::Scalar(s) => Ok(YamlNode::Scalar(s.clone())),
        LiveNode::Alias {
            name,
            resolved,
            meta,
            ..
        } => Ok(YamlNode::Alias {
            name: name.clone(),
            resolved: resolved.clone(),
            meta: meta.clone(),
        }),
    }
}

/// Convert a pyclass-stored mapping back to a pure `YamlNode` (no anchor
/// tracking). Used by `py_to_node_inner` when an existing pyclass value is
/// re-inserted into another container.
fn livenode_mapping_to_yamlnode(
    bound_m: &Bound<'_, PyYamlMapping>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    extract_mapping_inner(bound_m, schema, ctx, None)
}

fn livenode_sequence_to_yamlnode(
    bound_s: &Bound<'_, PyYamlSequence>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    extract_sequence_inner(bound_s, schema, ctx, None)
}

/// Wrap an already-live `YamlMapping<LiveNode>` into a fresh `Py<PyYamlMapping>`.
/// Used by `copy()` / `__deepcopy__` where the contents are already `LiveNode`s.
pub(crate) fn live_mapping_to_py_obj(
    py: Python<'_>,
    m: YamlMapping<LiveNode>,
    meta: DocMetadata,
) -> PyResult<Py<PyAny>> {
    let obj: Py<PyYamlMapping> = Py::new(
        py,
        (
            PyYamlMapping { inner: m },
            crate::py::py_node::PyYamlNode { meta },
        ),
    )?;
    Ok(obj.into_any())
}

/// Sequence counterpart of [`live_mapping_to_py_obj`].
pub(crate) fn live_sequence_to_py_obj(
    py: Python<'_>,
    s: YamlSequence<LiveNode>,
    meta: DocMetadata,
) -> PyResult<Py<PyAny>> {
    let obj: Py<PyYamlSequence> = Py::new(
        py,
        (
            PyYamlSequence { inner: s },
            crate::py::py_node::PyYamlNode { meta },
        ),
    )?;
    Ok(obj.into_any())
}

/// Convert a parsed `YamlMapping<YamlNode>` into a live
/// `YamlMapping<LiveNode>` by materialising each entry value.
pub(crate) fn materialise_mapping(
    py: Python<'_>,
    m: YamlMapping<YamlNode>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<YamlMapping<LiveNode>> {
    let mut live = YamlMapping::<LiveNode>::with_capacity(m.entries.len());
    live.style = m.style;
    live.meta = m.meta;
    live.trailing_blank_lines = m.trailing_blank_lines;
    for (k, e) in m.entries {
        let value = materialise_node(py, e.value, schema, ctx)?;
        live.entries.insert(
            k,
            YamlEntry {
                value,
                key_style: e.key_style,
                key_anchor: e.key_anchor,
                key_alias: e.key_alias,
                key_tag: e.key_tag,
                key_node: e.key_node,
            },
        );
    }
    Ok(live)
}

/// Sequence counterpart of [`materialise_mapping`].
pub(crate) fn materialise_sequence(
    py: Python<'_>,
    s: YamlSequence<YamlNode>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<YamlSequence<LiveNode>> {
    let mut live = YamlSequence::<LiveNode>::with_capacity(s.items.len());
    live.style = s.style;
    live.meta = s.meta;
    live.trailing_blank_lines = s.trailing_blank_lines;
    for item in s.items {
        live.items.push(materialise_node(py, item, schema, ctx)?);
    }
    Ok(live)
}

/// `__init__` body for `PyYamlMapping`: freeze the schema, extract the input,
/// and replace `slf.inner` while carrying over the shell's `style` / `tag`
/// (set in `__new__` from the constructor kwargs).
pub(crate) fn init_live_mapping(
    slf: &Bound<'_, PyYamlMapping>,
    obj: &Bound<'_, PyAny>,
    schema: Option<&Py<Schema>>,
) -> PyResult<()> {
    let py = slf.py();
    crate::py::schema::freeze_schema(py, schema);
    let sb = schema.map(|s| s.bind(py));
    // `extract_yaml_node` (not `py_to_node`) so self-referential dicts
    // round-trip via auto-anchor instead of erroring on the cycle guard.
    let node = extract_yaml_node(obj, sb.as_ref().copied())?;
    let YamlNode::Mapping(parsed) = node else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "YamlMapping requires a dict or mapping-like object",
        ));
    };
    let mut ctx = LoadCtx::default();
    let mut live = materialise_mapping(py, parsed, sb.as_ref().copied(), &mut ctx)?;
    let mut borrow = slf.borrow_mut();
    let style = borrow.inner.style;
    let tag = std::mem::take(&mut borrow.inner.meta.tag);
    live.style = style;
    live.meta.tag = tag;
    borrow.inner = live;
    Ok(())
}

/// Sequence counterpart of [`init_live_mapping`].
pub(crate) fn init_live_sequence(
    slf: &Bound<'_, PyYamlSequence>,
    obj: &Bound<'_, PyAny>,
    schema: Option<&Py<Schema>>,
) -> PyResult<()> {
    let py = slf.py();
    crate::py::schema::freeze_schema(py, schema);
    let sb = schema.map(|s| s.bind(py));
    let node = extract_yaml_node(obj, sb.as_ref().copied())?;
    let YamlNode::Sequence(parsed) = node else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "YamlSequence requires a list or iterable object",
        ));
    };
    let mut ctx = LoadCtx::default();
    let mut live = materialise_sequence(py, parsed, sb.as_ref().copied(), &mut ctx)?;
    let mut borrow = slf.borrow_mut();
    let style = borrow.inner.style;
    let tag = std::mem::take(&mut borrow.inner.meta.tag);
    live.style = style;
    live.meta.tag = tag;
    borrow.inner = live;
    Ok(())
}

pub(crate) fn mapping_to_py_obj_inner(
    py: Python<'_>,
    m: YamlMapping<YamlNode>,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    let live = materialise_mapping(py, m, schema, ctx)?;
    let obj: Py<PyYamlMapping> = Py::new(
        py,
        (
            PyYamlMapping { inner: live },
            crate::py::py_node::PyYamlNode { meta },
        ),
    )?;
    Ok(obj.into_any())
}

pub(crate) fn sequence_to_py_obj_inner(
    py: Python<'_>,
    s: YamlSequence<YamlNode>,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    let live = materialise_sequence(py, s, schema, ctx)?;
    let obj: Py<PyYamlSequence> = Py::new(
        py,
        (
            PyYamlSequence { inner: live },
            crate::py::py_node::PyYamlNode { meta },
        ),
    )?;
    Ok(obj.into_any())
}

/// Walk a live slot; for any `LivePy(Py<…>)` holding a typed yarutsk pyclass,
/// replace the wrapped `Py` with a freshly deep-copied one. Opaque values
/// are left in place. Used by `__deepcopy__`.
pub(crate) fn deep_clone_live(py: Python<'_>, slot: &mut LiveNode) -> PyResult<()> {
    let LiveNode::LivePy(p) = slot else {
        return Ok(());
    };
    let bound = p.bind(py);
    if let Ok(child_m) = bound.cast::<PyYamlMapping>() {
        *p = super::py_mapping::deep_copy_mapping(&child_m.borrow(), py)?;
    } else if let Ok(child_s) = bound.cast::<PyYamlSequence>() {
        *p = super::py_sequence::deep_copy_sequence(&child_s.borrow(), py)?;
    } else if let Ok(child_sc) = bound.cast::<PyYamlScalar>() {
        let cloned = PyYamlScalar {
            inner: child_sc.borrow().inner.clone(),
        };
        let new_py = Py::new(py, (cloned, crate::py::py_node::PyYamlNode::default()))?;
        *p = new_py.into_any();
    }
    Ok(())
}

/// Convert a pure `YamlNode` to its `LiveNode` form: mappings/sequences and
/// loader-transformed values become `LiveNode::LivePy(Py<…>)` (whether the
/// wrapped Py is a yarutsk pyclass or an opaque user value is decided by
/// downcast at access sites). Untagged or schema-less scalars stay as
/// `LiveNode::Scalar` so their style metadata round-trips losslessly. Aliases
/// get a `materialised: Some(p)` cache populated from the anchor's resolved
/// Py so identity is preserved across separate `__getitem__` calls.
pub(crate) fn materialise_node(
    py: Python<'_>,
    node: YamlNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<LiveNode> {
    match node {
        YamlNode::Mapping(m) => {
            let tag = m.meta.tag.clone();
            let anchor = m.meta.anchor.clone();
            let py_obj = mapping_to_py_obj_inner(py, m, DocMetadata::default(), schema, ctx)?;
            let final_obj = apply_loader(py, schema, tag.as_deref(), py_obj)?;
            if let Some(name) = anchor {
                ctx.register(name, &final_obj, py);
            }
            Ok(LiveNode::LivePy(final_obj))
        }
        YamlNode::Sequence(s) => {
            let tag = s.meta.tag.clone();
            let anchor = s.meta.anchor.clone();
            let py_obj = sequence_to_py_obj_inner(py, s, DocMetadata::default(), schema, ctx)?;
            let final_obj = apply_loader(py, schema, tag.as_deref(), py_obj)?;
            if let Some(name) = anchor {
                ctx.register(name, &final_obj, py);
            }
            Ok(LiveNode::LivePy(final_obj))
        }
        YamlNode::Scalar(s) => {
            // Custom-tagged scalars: collapse to `OpaquePy(loaded_py)`.
            if let Some(loader_fn) = lookup_loader(py, schema, s.meta.tag.as_deref()) {
                let tag = s.meta.tag.as_deref().unwrap_or("?").to_owned();
                let default_val = scalar_to_py(py, s.value())?;
                let py_obj = call_loader(py, &loader_fn, &tag, default_val)?;
                if let Some(name) = s.meta.anchor.clone() {
                    ctx.register(name, &py_obj, py);
                }
                return Ok(LiveNode::LivePy(py_obj));
            }
            Ok(LiveNode::Scalar(s))
        }
        YamlNode::Alias {
            name,
            resolved,
            meta,
        } => {
            let materialised = if let Some(cached) = ctx.lookup(&name, py) {
                Some(cached)
            } else {
                Some(yamlnode_to_py_inner(py, &resolved, schema, ctx)?)
            };
            Ok(LiveNode::Alias {
                name,
                resolved,
                materialised,
                meta,
            })
        }
    }
}

/// Repr of a stored live slot.
pub(crate) fn live_repr(py: Python<'_>, node: &LiveNode) -> String {
    match node {
        LiveNode::Scalar(s) => scalar_repr(s),
        LiveNode::Alias { resolved, .. } => yamlnode_repr(py, resolved),
        LiveNode::LivePy(p) => {
            let bound = p.bind(py);
            if let Ok(sc) = bound.cast::<PyYamlScalar>() {
                return yamlnode_repr(py, &sc.borrow().inner);
            }
            bound
                .repr()
                .map_or_else(|_| "<opaque>".to_string(), |s| s.to_string())
        }
    }
}

/// Repr of a pure `YamlNode` (used inside aliases).
pub(crate) fn yamlnode_repr(py: Python<'_>, node: &YamlNode) -> String {
    match node {
        YamlNode::Scalar(s) => scalar_repr(s),
        YamlNode::Mapping(m) => mapping_repr(py, m),
        YamlNode::Sequence(s) => sequence_repr(py, s),
        YamlNode::Alias { resolved, .. } => yamlnode_repr(py, resolved),
    }
}

fn scalar_repr(s: &YamlScalar) -> String {
    match s.value() {
        ScalarValue::Null => "None".to_string(),
        ScalarValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        ScalarValue::Int(n) => n.to_string(),
        ScalarValue::Float(f) => format!("{f}"),
        ScalarValue::Str(s) => format!("{s:?}"),
    }
}

pub(crate) fn mapping_repr<N: Node + ReprNode>(py: Python<'_>, m: &YamlMapping<N>) -> String {
    let pairs: Vec<String> = m
        .entries
        .iter()
        .map(|(k, e)| format!("{k:?}: {}", e.value.repr_str(py)))
        .collect();
    format!("YamlMapping({{{}}})", pairs.join(", "))
}

pub(crate) fn sequence_repr<N: Node + ReprNode>(py: Python<'_>, s: &YamlSequence<N>) -> String {
    let items: Vec<String> = s.items.iter().map(|i| i.repr_str(py)).collect();
    format!("YamlSequence([{}])", items.join(", "))
}

/// Helper trait so `mapping_repr` / `sequence_repr` work for both
/// `YamlMapping<YamlNode>` (used in aliases / extracted trees) and
/// `YamlMapping<LiveNode>` (used by pyclass `__repr__`).
pub(crate) trait ReprNode {
    fn repr_str(&self, py: Python<'_>) -> String;
}

impl ReprNode for YamlNode {
    fn repr_str(&self, py: Python<'_>) -> String {
        yamlnode_repr(py, self)
    }
}

impl ReprNode for LiveNode {
    fn repr_str(&self, py: Python<'_>) -> String {
        live_repr(py, self)
    }
}

/// Convert a pyclass-stored live tree back to plain Python (`to_python()`).
pub(crate) fn live_to_python(py: Python<'_>, node: &LiveNode) -> PyResult<Py<PyAny>> {
    match node {
        LiveNode::Scalar(s) => scalar_to_py_with_tag(py, s, None),
        LiveNode::Alias { resolved, .. } => yamlnode_to_python(py, resolved),
        LiveNode::LivePy(p) => {
            let bound = p.bind(py);
            if let Ok(child_m) = bound.cast::<PyYamlMapping>() {
                return live_mapping_to_python(py, &child_m.borrow().inner);
            }
            if let Ok(child_s) = bound.cast::<PyYamlSequence>() {
                return live_sequence_to_python(py, &child_s.borrow().inner);
            }
            if let Ok(child_sc) = bound.cast::<PyYamlScalar>() {
                return pyyamlscalar_to_py(py, &child_sc.borrow().inner, None);
            }
            // Opaque non-yarutsk Python value: return as-is.
            Ok(p.clone_ref(py))
        }
    }
}

fn yamlnode_to_python(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, None),
        YamlNode::Mapping(m) => yamlnode_mapping_to_python(py, m),
        YamlNode::Sequence(s) => yamlnode_sequence_to_python(py, s),
        YamlNode::Alias { resolved, .. } => yamlnode_to_python(py, resolved),
    }
}

pub(crate) fn live_mapping_to_python(
    py: Python<'_>,
    m: &YamlMapping<LiveNode>,
) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    for (k, e) in &m.entries {
        let v = live_to_python(py, &e.value)?;
        d.set_item(k.python_key(), v)?;
    }
    Ok(d.into_any().unbind())
}

pub(crate) fn live_sequence_to_python(
    py: Python<'_>,
    s: &YamlSequence<LiveNode>,
) -> PyResult<Py<PyAny>> {
    let items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|i| live_to_python(py, i))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, items)?.into_any().unbind())
}

fn yamlnode_mapping_to_python(py: Python<'_>, m: &YamlMapping<YamlNode>) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    for (k, e) in &m.entries {
        let v = yamlnode_to_python(py, &e.value)?;
        d.set_item(k.python_key(), v)?;
    }
    Ok(d.into_any().unbind())
}

fn yamlnode_sequence_to_python(py: Python<'_>, s: &YamlSequence<YamlNode>) -> PyResult<Py<PyAny>> {
    let items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|i| yamlnode_to_python(py, i))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, items)?.into_any().unbind())
}

pub(crate) fn parse_text(text: &str, schema: Option<&Schema>) -> PyResult<ParseOutput> {
    let policy = schema.and_then(Schema::tag_policy);
    parse_str(text, policy.as_ref()).map_err(|e| ParseError::new_err(e.clone()))
}

pub(crate) fn parse_stream(
    stream: &Bound<'_, PyAny>,
    schema: Option<&Schema>,
) -> PyResult<ParseOutput> {
    use std::sync::{Arc, Mutex};

    use super::streaming::{CharsSource, PyIoCharsIter};
    let policy = schema.and_then(Schema::tag_policy);
    let error_slot: Arc<Mutex<Option<PyErr>>> = Arc::new(Mutex::new(None));
    let iter = PyIoCharsIter::new(stream.clone().unbind(), error_slot.clone());
    let src = CharsSource::PyIo(iter);
    let result = parse_iter(src, policy.as_ref());
    if let Ok(mut guard) = error_slot.lock()
        && let Some(err) = guard.take()
    {
        return Err(err);
    }
    result.map_err(ParseError::new_err)
}
