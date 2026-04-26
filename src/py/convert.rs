// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Python ↔ `YamlNode` boundary.
//!
//! Two halves:
//! - **Load** (Rust → Python): `parse_text`/`parse_stream` drive the builder,
//!   then `node_to_doc` / `materialise_node` wrap each `YamlNode` in the
//!   matching pyclass (`PyYamlMapping`/`PyYamlSequence`/`PyYamlScalar`),
//!   running schema loaders against tagged scalars.
//! - **Dump** (Python → Rust): `extract_yaml_node` walks Python objects,
//!   running schema dumpers, tracking anchor identity, and materialising a
//!   `YamlNode` ready for the emitter.
//!
//! Invariant — `NodeParent` write-through: when `mapping.node(k)` /
//! `sequence.node(i)` returns a *scalar*, that `PyYamlScalar` carries a
//! `NodeParent` back-reference so setters reach into the parent's `inner`.
//! Without it, mutations would land on a clone and disappear at emit time.
//! Container children don't need the back-ref because the live child is
//! already the object stored in the parent collection.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyList, PyTuple, PyType};

use pyo3::exceptions::PyKeyError;

use super::py_mapping::PyYamlMapping;
use super::py_scalar::PyYamlScalar;
use super::py_sequence::PyYamlSequence;
use super::schema::Schema;
use crate::core::builder::{DocMetadata, ParseOutput, parse_iter, parse_str};
use crate::core::types::{
    MapKey, NodeMeta, ScalarRepr, ScalarStyle, ScalarValue, YamlEntry, YamlMapping, YamlNode,
    YamlScalar, YamlSequence,
};
use crate::{DumperError, LoaderError, ParseError};

//
// When `YamlMapping.node(k)` / `YamlSequence.node(i)` returns a `YamlScalar`,
// the returned object carries a `NodeParent` pointing back to the owning
// container so that setters propagate into the parent's `inner` — otherwise
// mutations would land in a clone and disappear on emission.
//
// Container children return the live `Container` child Py directly (mutations
// propagate without a back-reference).

#[derive(Default)]
pub(crate) enum NodeParent {
    #[default]
    None,
    Map {
        parent: Py<PyYamlMapping>,
        key: String,
    },
    Seq {
        parent: Py<PyYamlSequence>,
        idx: usize,
    },
}

impl Clone for NodeParent {
    fn clone(&self) -> Self {
        match self {
            NodeParent::None => NodeParent::None,
            NodeParent::Map { parent, key } => Python::attach(|py| NodeParent::Map {
                parent: parent.clone_ref(py),
                key: key.clone(),
            }),
            NodeParent::Seq { parent, idx } => Python::attach(|py| NodeParent::Seq {
                parent: parent.clone_ref(py),
                idx: *idx,
            }),
        }
    }
}

impl NodeParent {
    /// Apply *f* to the corresponding node slot in the parent container.
    /// No-op for `NodeParent::None` or when the key/index no longer exists.
    pub(crate) fn with_node_mut<F>(&self, py: Python<'_>, f: F)
    where
        F: FnOnce(&mut YamlNode),
    {
        match self {
            NodeParent::None => {}
            NodeParent::Map { parent, key } => {
                let mut borrow = parent.borrow_mut(py);
                if let Some(entry) = borrow.inner.entries.get_mut(&MapKey::scalar(key.as_str())) {
                    f(&mut entry.value);
                }
            }
            NodeParent::Seq { parent, idx } => {
                let mut borrow = parent.borrow_mut(py);
                if let Some(item) = borrow.inner.items.get_mut(*idx) {
                    f(item);
                }
            }
        }
    }
}

static DATETIME_TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static DATE_TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

pub(crate) fn datetime_type(py: Python<'_>) -> PyResult<&Bound<'_, PyType>> {
    DATETIME_TYPE.import(py, "datetime", "datetime")
}

pub(crate) fn date_type(py: Python<'_>) -> PyResult<&Bound<'_, PyType>> {
    DATE_TYPE.import(py, "datetime", "date")
}

//
// `EmitCtx` bundles the cycle guard and anchor state for one top-level
// extraction, threaded explicitly through the recursive `*_inner` helpers.

struct AnchorEmitState {
    /// Plain-container objects that appear more than once in a document.
    /// Value: `None` = needs an anchor but name not yet assigned (first encounter);
    ///        `Some(name)` = anchor already emitted (subsequent encounters → alias).
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
/// that aliases can be returned as the *same* `Py<PyAny>`. This gives the
/// Python-side reference semantics requested for B1 — `*foo` and the
/// `&foo`-anchored container are the same object, mutations propagate.
///
/// Lives for the duration of one top-level `node_to_doc` / `node_to_py` call.
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

/// Per-call extraction state. One instance lives for the duration of a single
/// top-level `extract_yaml_node` / `py_to_node` call and is passed by mutable
/// reference into every recursive callee.
#[derive(Default)]
pub(crate) struct EmitCtx {
    /// Set of Python object ptrs currently on the conversion stack — used to
    /// short-circuit self-referential dicts/lists/tuples before they overflow
    /// the Rust stack.
    cycle_set: HashSet<usize>,
    /// Auto-anchor state for the document being extracted. `None` outside of
    /// `extract_yaml_node` (anchor handling only applies on the dump path).
    anchors: Option<AnchorEmitState>,
}

impl EmitCtx {
    /// Run `f` with `ptr` registered as on the current call stack. Returns
    /// the error produced by `cycle_err` if `ptr` is already present (cycle).
    /// `ptr` is removed on the way out, including when `f` returns `Err`.
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

    /// Initialise per-document anchor state by walking `doc` once and
    /// recording every plain container that appears more than once.
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

    /// Check whether *ptr* needs special anchor/alias treatment during emit.
    ///
    /// Returns `(Some(alias_name), None)` if the object was already serialised
    /// and should be emitted as `*alias_name`.
    /// Returns `(None, Some(anchor_name))` on the first encounter of a
    /// multi-ref object; the caller attaches `anchor_name` to the node.
    /// Returns `(None, None)` if no anchor tracking applies.
    ///
    /// `explicit_anchor` is the name already present on the value (preserved
    /// from a loaded `&name` declaration). When provided, it's used verbatim
    /// for the first-encounter anchor name instead of the synthetic `id001`.
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
/// once — including self-cycles — get an anchor on emit and become aliases
/// on subsequent encounters.
fn prepass(obj: &Bound<'_, PyAny>, ref_count: &mut HashMap<usize, usize>) {
    let py = obj.py();
    if let Ok(m_bound) = obj.cast::<PyYamlMapping>() {
        let ptr = obj.as_ptr() as usize;
        if *ref_count.entry(ptr).or_insert(0) > 0 {
            return; // already visited — avoid infinite recursion
        }
        *ref_count.entry(ptr).or_insert(0) = 1;
        let m = m_bound.borrow();
        for entry in m.inner.entries.values() {
            if let YamlNode::Container(p) | YamlNode::OpaquePy(p) = &entry.value {
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
            if let YamlNode::Container(p) | YamlNode::OpaquePy(p) = item {
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
        return; // already walked (or it's a back-edge / cycle) — don't recurse again
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

/// Look up the schema loader registered for `tag`, returning a cloned `Py` reference.
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

/// Invoke a schema loader with `arg`, wrapping any error in `LoaderError`.
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
///
/// - Schema loader for the tag (if any) fires first.
/// - `!!binary` → `bytes` (base64-decoded)
/// - `!!timestamp` → `datetime.datetime` or `datetime.date`
/// - everything else → delegated to `scalar_to_py`
pub(crate) fn scalar_to_py_with_tag(
    py: Python<'_>,
    s: &YamlScalar,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    // Schema loader takes priority over all built-in tag handlers.
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
            // YAML allows a space in place of 'T' between date and time.
            let normalized = raw.replacen(' ', "T", 1);
            // Date-only values contain no 'T' and no time ':' after the date portion.
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

/// Construct a plain (unquoted, untagged) `YamlNode::Scalar` from a typed value.
/// Used when converting Python primitives to YAML nodes during dump.
pub(crate) fn plain_scalar(value: ScalarValue) -> YamlNode {
    YamlNode::Scalar(YamlScalar {
        repr: ScalarRepr::Canonical(value),
        style: ScalarStyle::Plain,
        chomping: None,
        meta: NodeMeta::default(),
    })
}

/// A `YamlEntry` with plain key style. Used when inserting entries via Python
/// mutations (dict ops, update, etc.).
pub(crate) fn plain_entry(value: YamlNode) -> YamlEntry {
    YamlEntry {
        value,
        key_style: ScalarStyle::Plain,
        key_anchor: None,
        key_alias: None,
        key_tag: None,
        key_node: None,
    }
}

/// Run `f` against the live child's `inner.meta` if `p` is a
/// `PyYamlMapping` / `PyYamlSequence`. No-op for arbitrary user opaques.
fn with_opaque_meta<R>(
    py: Python<'_>,
    p: &Py<PyAny>,
    f_read: impl FnOnce(&NodeMeta) -> R,
    f_default: impl FnOnce() -> R,
) -> R {
    let bound = p.bind(py);
    if let Ok(m) = bound.cast::<PyYamlMapping>() {
        return f_read(&m.borrow().inner.meta);
    }
    if let Ok(s) = bound.cast::<PyYamlSequence>() {
        return f_read(&s.borrow().inner.meta);
    }
    f_default()
}

fn with_opaque_meta_mut(py: Python<'_>, p: &Py<PyAny>, f: impl FnOnce(&mut NodeMeta)) {
    let bound = p.bind(py);
    if let Ok(m) = bound.cast::<PyYamlMapping>() {
        f(&mut m.borrow_mut().inner.meta);
        return;
    }
    if let Ok(s) = bound.cast::<PyYamlSequence>() {
        f(&mut s.borrow_mut().inner.meta);
    }
}

/// Read `(comment_inline, comment_before, blank_lines_before)` off a value
/// that lives in `inner.entries` / `inner.items`. For `Container(Py<…>)` the
/// real metadata is on the wrapped Py's `inner.meta`, not on the node
/// accessor (which `node_accessor!` defines as no-op for `Container`/
/// `OpaquePy`). `OpaquePy` carries no metadata of its own.
pub(crate) fn read_metadata(node: &YamlNode) -> (Option<String>, Option<String>, u8) {
    if let YamlNode::Container(p) = node {
        return Python::attach(|py| {
            with_opaque_meta(
                py,
                p,
                |meta| {
                    (
                        meta.comment_inline.clone(),
                        meta.comment_before.clone(),
                        meta.blank_lines_before,
                    )
                },
                || (None, None, 0),
            )
        });
    }
    (
        node.comment_inline().map(str::to_owned),
        node.comment_before().map(str::to_owned),
        node.blank_lines_before(),
    )
}

/// Carry inline/before-comment and `blank_lines_before` from the previous
/// slot onto a freshly-converted node, but only when the new node didn't
/// bring its own value for that field. Used by `__setitem__` to preserve
/// comment metadata across in-place value swaps.
pub(crate) fn carry_metadata(
    node: &mut YamlNode,
    old_inline: Option<String>,
    old_before: Option<String>,
    old_blanks: u8,
) {
    if let YamlNode::Container(p) = node {
        Python::attach(|py| {
            with_opaque_meta_mut(py, p, |meta| {
                if meta.comment_inline.is_none() {
                    meta.comment_inline = old_inline;
                }
                if meta.comment_before.is_none() {
                    meta.comment_before = old_before;
                }
                if meta.blank_lines_before == 0 {
                    meta.blank_lines_before = old_blanks;
                }
            });
        });
        return;
    }
    if node.comment_inline().is_none() {
        node.set_comment_inline(old_inline);
    }
    if node.comment_before().is_none() {
        node.set_comment_before(old_before);
    }
    if node.blank_lines_before() == 0 {
        node.set_blank_lines_before(old_blanks);
    }
}

/// Resolve a Python sequence index (supports negative indices).
/// Returns an error if the index is out of range.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // see py_sequence::__setitem__
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

/// Convert a Python value into the `YamlNode` form used inside `inner.entries`
/// / `inner.items`. Container values are materialised into
/// `Container(Py<PyYamlMapping|PyYamlSequence>)` so subsequent reads return
/// the same Py and mutations propagate. Anything `py_to_node` can't convert
/// lands as `OpaquePy(value)` — the schema dumper, if any, fires at dump time
/// via `extract_yaml_node`.
///
/// Note: assigning an existing `PyYamlMapping`/`PyYamlSequence` *snapshots*
/// it (deep-clones via `py_to_node` then materialises into a fresh Py), so
/// `m['a'] = m['b']` makes `m['a'] is m['b']` False. Required to avoid
/// `m['self'] = m` creating a cycle.
pub(crate) fn py_to_stored_node(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    // Custom objects with no native form: store verbatim, defer to schema at dump.
    let Ok(mut node) = py_to_node(value, schema) else {
        return Ok(YamlNode::OpaquePy(value.clone().unbind()));
    };
    let mut ctx = LoadCtx::default();
    materialise_node(py, &mut node, schema, &mut ctx)?;
    Ok(node)
}

/// Convert a `YamlNode` to its Python representation.
/// Mapping → `PyYamlMapping`, Sequence → `PyYamlSequence`, scalar/null → Python primitive.
///
/// Top-level wrapper that constructs a fresh [`LoadCtx`]. For recursion, call
/// [`node_to_py_inner`] with the existing ctx so anchor identity is shared.
pub(crate) fn node_to_py(
    py: Python<'_>,
    node: &YamlNode,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    let mut ctx = LoadCtx::default();
    node_to_py_inner(py, node, schema, &mut ctx)
}

/// Recursive body of [`node_to_py`]. For `Alias`, returns the cached Py for
/// the named anchor if one is registered (so all references share identity).
pub(crate) fn node_to_py_inner(
    py: Python<'_>,
    node: &YamlNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
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
        YamlNode::Alias {
            name,
            resolved,
            materialised,
            ..
        } => {
            // Materialised slot wins — the live Py was already built and
            // shared by `materialise_node` (load path).
            if let Some(p) = materialised.as_ref() {
                return Ok(p.clone_ref(py));
            }
            // Anchor cache (mid-recursion at load time before this Alias was
            // visited) wins next.
            if let Some(cached) = ctx.lookup(name, py) {
                return Ok(cached);
            }
            // Last resort — convert the resolved subtree as a fresh Py.
            node_to_py_inner(py, resolved, schema, ctx)
        }
        // Container holds a typed `PyYamlMapping`/`PyYamlSequence`; OpaquePy
        // holds an arbitrary Python value. Either round-trips back to Python
        // verbatim — the schema dumper, if any, only fires at dump time via
        // `extract_yaml_node`.
        YamlNode::Container(p) | YamlNode::OpaquePy(p) => Ok(p.clone_ref(py)),
    }
}

/// If *schema* has a loader for *tag*, call it with *`py_obj`* and return the result.
/// Otherwise return *`py_obj`* unchanged.
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

/// Convert a Python primitive (None/bool/int/float/str) to a scalar `YamlNode`.
/// Returns None if *obj* is not a recognised primitive type.
pub(crate) fn py_primitive_to_scalar(obj: &Bound<'_, PyAny>) -> Option<YamlNode> {
    if obj.is_none() {
        return Some(plain_scalar(ScalarValue::Null));
    }
    // bool must come before i64 (Python bool is a subtype of int)
    if let Ok(b) = obj.extract::<bool>() {
        return Some(plain_scalar(ScalarValue::Bool(b)));
    }
    if let Ok(n) = obj.extract::<i64>() {
        return Some(plain_scalar(ScalarValue::Int(n)));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Some(plain_scalar(ScalarValue::Float(f)));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Some(plain_scalar(ScalarValue::Str(s)));
    }
    None
}

/// Convert a Python object to a `YamlNode`. Top-level wrapper that constructs
/// a fresh [`EmitCtx`] for cycle detection — for recursion, call
/// [`py_to_node_inner`] with the existing ctx.
pub(crate) fn py_to_node(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    let mut ctx = EmitCtx::default();
    py_to_node_inner(obj, schema, &mut ctx)
}

/// Recursive body of [`py_to_node`]. See `py_to_node` docs.
#[allow(clippy::too_many_lines)] // single dispatch over Python types
pub(crate) fn py_to_node_inner(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    // Schema dumpers are checked first (before all built-in type handling).
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
                _ => {}
            }
            return Ok(node);
        }
    }

    // Custom types before primitives so a `YamlMapping`/`YamlSequence`
    // round-trips through its rich `inner` rather than via the dict/list
    // extraction path below.
    if let Ok(m) = obj.extract::<PyYamlMapping>() {
        return Ok(YamlNode::Mapping(m.inner));
    }
    if let Ok(s) = obj.extract::<PyYamlSequence>() {
        return Ok(YamlNode::Sequence(s.inner));
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    if let Some(node) = py_primitive_to_scalar(obj) {
        return Ok(node);
    }
    // tuple / list → sequence (checked before bytes to prevent small-integer
    // collections being misinterpreted as Vec<u8> by PyO3's sequence extraction).
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
                let mut seq = YamlSequence::new();
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
                let mut seq = YamlSequence::new();
                for item in l.iter() {
                    seq.items.push(py_to_node_inner(&item, schema, ctx)?);
                }
                Ok(YamlNode::Sequence(seq))
            },
        );
    }
    // bytes/bytearray → !!binary scalar (base64-encoded).
    // Check the Python type explicitly to avoid PyO3's Vec<u8> extraction
    // matching any iterable of small ints (e.g. deque([1,2,3])).
    if (obj.is_instance_of::<pyo3::types::PyBytes>()
        || obj.is_instance_of::<pyo3::types::PyByteArray>())
        && let Ok(b) = obj.extract::<Vec<u8>>()
    {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let encoded = STANDARD.encode(&b);
        return Ok(YamlNode::Scalar(YamlScalar {
            repr: ScalarRepr::Canonical(ScalarValue::Str(encoded)),
            style: ScalarStyle::Plain,
            chomping: None,
            meta: NodeMeta {
                tag: Some("!!binary".to_owned()),
                ..NodeMeta::default()
            },
        }));
    }
    // datetime.datetime / datetime.date → !!timestamp scalar
    {
        let py = obj.py();
        if obj.is_instance(datetime_type(py)?)? || obj.is_instance(date_type(py)?)? {
            let iso: String = obj.call_method0("isoformat")?.extract()?;
            return Ok(YamlNode::Scalar(YamlScalar {
                repr: ScalarRepr::Canonical(ScalarValue::Str(iso)),
                style: ScalarStyle::Plain,
                chomping: None,
                meta: NodeMeta {
                    tag: Some("!!timestamp".to_owned()),
                    ..NodeMeta::default()
                },
            }));
        }
    }
    // Plain dict fallback (for users passing native Python dicts).
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
                let mut mapping = YamlMapping::new();
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
    // Abstract Mapping (collections.abc.Mapping) — covers OrderedDict-likes,
    // ChainMap, and any user type implementing the Mapping protocol that
    // doesn't subclass dict.
    {
        let abc = obj.py().import("collections.abc")?;
        let mapping_type = abc.getattr("Mapping")?;
        if obj.is_instance(&mapping_type)? {
            let items = obj.call_method0("items")?;
            let mut mapping = YamlMapping::new();
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
    // Abstract Iterable — covers set, frozenset, deque, generators, and any
    // user type implementing __iter__.  Checked after str/bytes/dict (which
    // are iterable but handled above).
    if obj.try_iter().is_ok() {
        let mut seq = YamlSequence::new();
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

/// Build a synthetic `Alias` node: just a name to emit as `*name`, with no
/// resolved subtree or materialised Py. Used by `extract_yaml_node_inner`
/// when auto-anchor revisits a Py it has already seen.
fn synthetic_alias(name: String) -> YamlNode {
    YamlNode::Alias {
        name,
        resolved: Arc::new(YamlNode::Null),
        meta: NodeMeta::default(),
        materialised: None,
    }
}

/// Visit every `Container(Py<…>)` child in *children*, casting each to its
/// concrete pyclass and invoking *visit*. Non-`Container` variants
/// (`Scalar`/`Alias`/`OpaquePy`/etc.) are skipped — recursive descent in
/// `format` / `sort_keys` / `sort` only follows live container `Py`s.
///
/// *children* must be collected up-front (e.g. via
/// `collect_container_children_*`) so the parent's borrow can be released
/// before recursion: nested calls re-enter `borrow_mut` on the parent's own
/// children.
pub(crate) fn for_each_opaque_child<F>(
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
        }
    }
    Ok(())
}

/// Borrow handed to the closure passed to `for_each_opaque_child`.
pub(crate) enum ChildContainer<'py, 'a> {
    Mapping(&'a Bound<'py, PyYamlMapping>),
    Sequence(&'a Bound<'py, PyYamlSequence>),
}

/// Snapshot of every `Container(Py<…>)` value in *m* — used by recursive
/// descent (sort/format) to release the parent borrow before recursing into
/// children. `OpaquePy` slots are intentionally skipped: they hold arbitrary
/// Python values, not typed pyclass children to recurse into.
pub(crate) fn collect_opaque_children_from_mapping(
    m: &YamlMapping,
    py: Python<'_>,
) -> Vec<Py<PyAny>> {
    m.entries
        .values()
        .filter_map(|e| match &e.value {
            YamlNode::Container(p) => Some(p.clone_ref(py)),
            _ => None,
        })
        .collect()
}

/// Snapshot of every `Container(Py<…>)` item in *s* — see
/// `collect_opaque_children_from_mapping`.
pub(crate) fn collect_opaque_children_from_sequence(
    s: &YamlSequence,
    py: Python<'_>,
) -> Vec<Py<PyAny>> {
    s.items
        .iter()
        .filter_map(|item| match item {
            YamlNode::Container(p) => Some(p.clone_ref(py)),
            _ => None,
        })
        .collect()
}

/// Return the child at *key* of a mapping as a typed node object.
///
/// Container children return the live `Container` Py so mutations propagate.
/// Scalar children return a fresh `PyYamlScalar` whose setters write through
/// via a `NodeParent` back-reference.
pub(crate) fn map_child_node(slf: &Bound<'_, PyYamlMapping>, key: &str) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let mk = MapKey::scalar(key);
    let kind = {
        let borrow = slf.borrow();
        match borrow.inner.entries.get(&mk) {
            Some(entry) => ChildKind::from_node(&entry.value),
            None => return Err(PyKeyError::new_err(key.to_owned())),
        }
    };
    match kind {
        ChildKind::Container => {
            let borrow = slf.borrow();
            match borrow.inner.entries.get(&mk).map(|e| &e.value) {
                Some(YamlNode::Container(p) | YamlNode::OpaquePy(p)) => Ok(p.clone_ref(py)),
                // Container kind without a typed/opaque slot shouldn't happen
                // post-materialisation; fall back to converting the node fresh.
                Some(other) => node_to_py(py, other, None),
                None => Err(PyKeyError::new_err(key.to_owned())),
            }
        }
        ChildKind::Scalar => {
            let node = slf.borrow().inner.entries.get(&mk).map(|e| e.value.clone());
            let Some(node) = node else {
                return Err(PyKeyError::new_err(key.to_owned()));
            };
            let scalar = PyYamlScalar {
                inner: node,
                parent: NodeParent::Map {
                    parent: slf.clone().unbind(),
                    key: key.to_owned(),
                },
            };
            Ok(Py::new(py, (scalar, crate::py::py_node::PyYamlNode::default()))?.into_any())
        }
    }
}

/// Return the item at *idx* of a sequence as a typed node object. Mirror of
/// `map_child_node` — see there for semantics.
pub(crate) fn seq_child_node(slf: &Bound<'_, PyYamlSequence>, idx: usize) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let kind = {
        let borrow = slf.borrow();
        ChildKind::from_node(&borrow.inner.items[idx])
    };
    match kind {
        ChildKind::Container => {
            let borrow = slf.borrow();
            match &borrow.inner.items[idx] {
                YamlNode::Container(p) | YamlNode::OpaquePy(p) => Ok(p.clone_ref(py)),
                other => node_to_py(py, other, None),
            }
        }
        ChildKind::Scalar => {
            let node = slf.borrow().inner.items[idx].clone();
            let scalar = PyYamlScalar {
                inner: node,
                parent: NodeParent::Seq {
                    parent: slf.clone().unbind(),
                    idx,
                },
            };
            Ok(Py::new(py, (scalar, crate::py::py_node::PyYamlNode::default()))?.into_any())
        }
    }
}

enum ChildKind {
    Container,
    Scalar,
}

impl ChildKind {
    fn from_node(n: &YamlNode) -> Self {
        match n {
            // After materialisation, container children live as `Container(Py)`;
            // an `OpaquePy` slot holds an arbitrary Python value (loader output
            // or user-assigned custom type) — surfacing that to the caller as a
            // "container" matches the live-Py contract of `m[k]` returning the
            // stored value identity-stable. Pre-materialisation `Mapping`/
            // `Sequence` are treated as containers for safety.
            YamlNode::Mapping(_)
            | YamlNode::Sequence(_)
            | YamlNode::Container(_)
            | YamlNode::OpaquePy(_) => ChildKind::Container,
            // Aliases follow the resolved node's kind (runtime `set_alias()`
            // case — load-time aliases became `Container` during materialisation).
            YamlNode::Alias { resolved, .. } => Self::from_node(resolved),
            YamlNode::Scalar(_) | YamlNode::Null => ChildKind::Scalar,
        }
    }
}

/// Convert a top-level `YamlNode` to `PyYamlMapping`, `PyYamlSequence`, or
/// `PyYamlScalar`. Constructs a fresh [`LoadCtx`] so anchored containers and
/// the aliases pointing to them surface as the *same* Python object across
/// the whole document.
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
            // Top-level container can itself be an anchor (uncommon but legal);
            // register it so any nested *self-reference* alias picks it up.
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
        other => {
            let scalar = PyYamlScalar {
                inner: other,
                parent: NodeParent::None,
            };
            let base = crate::py::py_node::PyYamlNode { meta };
            Ok(Py::new(py, (scalar, base))?.into_any())
        }
    }
}

/// Extract a `YamlNode` from a `PyYamlMapping`, `PyYamlSequence`, or `PyYamlScalar` for serialisation.
///
/// Top-level wrapper that creates a fresh [`EmitCtx`] and initialises anchor
/// tracking by walking *obj* once. For recursion, call [`extract_yaml_node_inner`]
/// with the existing ctx.
pub(crate) fn extract_yaml_node(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    let mut ctx = EmitCtx::default();
    ctx.init_anchors(obj);
    extract_yaml_node_inner(obj, schema, &mut ctx)
}

/// Recursive body of [`extract_yaml_node`].
#[allow(clippy::too_many_lines)] // single dispatch over container/scalar types
pub(crate) fn extract_yaml_node_inner(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    if let Ok(bound_m) = obj.cast::<PyYamlMapping>() {
        let ptr = obj.as_ptr() as usize;
        // Auto-anchor: if this Py was seen before in the prepass and we're
        // visiting it a second+ time, emit as alias. Honor any explicit
        // anchor name on the mapping's meta.
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
    // Scalars passed directly (int, str, etc.)
    if let Some(node) = py_primitive_to_scalar(obj) {
        return Ok(node);
    }
    // Plain dict fallback — no comment/style metadata, but values are correct.
    // Uses extract_yaml_node_inner recursively so nested YamlMapping/YamlSequence
    // objects inside the dict still preserve their metadata.
    if let Ok(d) = obj.cast::<PyDict>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = ctx.check_anchor(ptr, None);
        if let Some(name) = alias {
            return Ok(synthetic_alias(name));
        }
        let mut mapping = YamlMapping::new();
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
        let mut seq = YamlSequence::new();
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
        let mut seq = YamlSequence::new();
        if let Some(ref name) = anchor {
            seq.meta.anchor = Some(name.clone());
        }
        for item in t.iter() {
            seq.items.push(extract_yaml_node_inner(&item, schema, ctx)?);
        }
        return Ok(YamlNode::Sequence(seq));
    }
    // Fall through to py_to_node_inner for bytes, datetime, schema dumpers, and
    // abstract Mapping/Iterable types.
    py_to_node_inner(obj, schema, ctx)
}

/// Extract the `YamlMapping` for a `PyYamlMapping` by walking `inner.entries`.
/// Container children live as `Container(Py)` post-materialisation; recurse into
/// their Pys via `extract_yaml_node_inner` so anchor/alias detection (B1) and
/// the per-Py cycle guard apply uniformly.
fn extract_mapping_inner(
    bound_m: &Bound<'_, PyYamlMapping>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
    assigned_anchor: Option<String>,
) -> PyResult<YamlNode> {
    let py = bound_m.py();
    let borrow = bound_m.borrow();
    let mut mapping = YamlMapping::with_capacity(borrow.inner.entries.len());
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

/// Sequence counterpart of [`extract_mapping_inner`].
fn extract_sequence_inner(
    bound_s: &Bound<'_, PyYamlSequence>,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
    assigned_anchor: Option<String>,
) -> PyResult<YamlNode> {
    let py = bound_s.py();
    let borrow = bound_s.borrow();
    let mut seq = YamlSequence::with_capacity(borrow.inner.items.len());
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

/// Resolve a stored entry/item value into the `YamlNode` form for emission.
fn extract_entry_value(
    py: Python<'_>,
    value: &YamlNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut EmitCtx,
) -> PyResult<YamlNode> {
    match value {
        // Materialised containers / opaque user values: walk via the live Py.
        YamlNode::Container(p) | YamlNode::OpaquePy(p) => {
            extract_yaml_node_inner(p.bind(py), schema, ctx)
        }
        // Native variants (incl. Mapping/Sequence if the user built a YamlNode
        // tree by hand and never went through materialisation) pass through.
        _ => Ok(value.clone()),
    }
}

/// Create a `PyYamlMapping` from a Rust `YamlMapping`.
///
/// Top-level wrapper that constructs a fresh [`LoadCtx`]. Use
/// [`mapping_to_py_obj_inner`] when recursing inside an existing context so
/// anchored values surface as the same Python object across aliases.
pub(crate) fn mapping_to_py_obj(
    py: Python<'_>,
    m: YamlMapping,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    let mut ctx = LoadCtx::default();
    mapping_to_py_obj_inner(py, m, meta, schema, &mut ctx)
}

pub(crate) fn mapping_to_py_obj_inner(
    py: Python<'_>,
    mut m: YamlMapping,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    // Materialise children in-place: containers become `Container(Py<…>)`
    // (or `OpaquePy(...)` if a schema loader transformed them); aliases reuse
    // the anchor's Py via `LoadCtx`. Scalars
    // and Null are left as typed values.
    for entry in m.entries.values_mut() {
        materialise_node(py, &mut entry.value, schema, ctx)?;
    }

    let obj: Py<PyYamlMapping> = Py::new(
        py,
        (
            PyYamlMapping { inner: m },
            crate::py::py_node::PyYamlNode { meta },
        ),
    )?;

    Ok(obj.into_any())
}

/// Walk `node`; for any `Container(Py<PyYamlMapping|PyYamlSequence>)` slot,
/// replace the wrapped Py with a freshly deep-copied one. Used by both
/// `PyYamlMapping::__deepcopy__` and `PyYamlSequence::__deepcopy__` so each
/// nested container becomes an independent Py with its own `inner`.
///
/// `OpaquePy` slots (arbitrary Python values) are left as-is — `copy.deepcopy`
/// is the user's responsibility for arbitrary types. Only `Container` slots
/// are deep-cloned, since they're known to hold typed `PyYamlMapping`/
/// `PyYamlSequence` instances with a defined deep-copy semantics.
pub(crate) fn deep_clone_opaque(py: Python<'_>, node: &mut YamlNode) -> PyResult<()> {
    let YamlNode::Container(p) = node else {
        return Ok(());
    };
    let bound = p.bind(py);
    if let Ok(child_m) = bound.cast::<PyYamlMapping>() {
        *p = super::py_mapping::deep_copy_mapping(&child_m.borrow(), py)?;
    } else if let Ok(child_s) = bound.cast::<PyYamlSequence>() {
        *p = super::py_sequence::deep_copy_sequence(&child_s.borrow(), py)?;
    }
    Ok(())
}

/// Wrap a materialised Python object into the right `YamlNode` variant.
///
/// `Container` for `PyYamlMapping`/`PyYamlSequence` (the no-loader case, plus
/// schema dumpers that intentionally returned a typed yarutsk container);
/// `OpaquePy` for everything else — loader-transformed values, custom user
/// classes, etc. Reads of these slots later use `extract_yaml_node` to
/// dump-time-convert `OpaquePy` while `Container` round-trips directly.
fn wrap_materialised(py: Python<'_>, obj: Py<PyAny>) -> YamlNode {
    let bound = obj.bind(py);
    if bound.cast::<PyYamlMapping>().is_ok() || bound.cast::<PyYamlSequence>().is_ok() {
        YamlNode::Container(obj)
    } else {
        YamlNode::OpaquePy(obj)
    }
}

/// Replace `*node` in-place with its materialised form: untagged
/// mappings/sequences become `Container(Py<PyYamlMapping|PyYamlSequence>)`;
/// loader-transformed mappings/sequences and loader-transformed scalars
/// become `OpaquePy(<loaded Py>)`. Untagged or schema-less scalars are left
/// as `Scalar(YamlScalar)` so their original style/source/metadata round-
/// trips losslessly.
pub(crate) fn materialise_node(
    py: Python<'_>,
    node: &mut YamlNode,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<()> {
    match node {
        YamlNode::Mapping(m) => {
            let m_owned = std::mem::take(m);
            let tag = m_owned.meta.tag.clone();
            let anchor = m_owned.meta.anchor.clone();
            let py_obj = mapping_to_py_obj_inner(py, m_owned, DocMetadata::default(), schema, ctx)?;
            let final_obj = apply_loader(py, schema, tag.as_deref(), py_obj)?;
            if let Some(name) = anchor {
                ctx.register(name, &final_obj, py);
            }
            *node = wrap_materialised(py, final_obj);
        }
        YamlNode::Sequence(s) => {
            let s_owned = std::mem::take(s);
            let tag = s_owned.meta.tag.clone();
            let anchor = s_owned.meta.anchor.clone();
            let py_obj =
                sequence_to_py_obj_inner(py, s_owned, DocMetadata::default(), schema, ctx)?;
            let final_obj = apply_loader(py, schema, tag.as_deref(), py_obj)?;
            if let Some(name) = anchor {
                ctx.register(name, &final_obj, py);
            }
            *node = wrap_materialised(py, final_obj);
        }
        YamlNode::Scalar(s) => {
            // Custom-tagged scalars: collapse to `OpaquePy(loaded_py)` so
            // `doc[k]` returns the loader's value. Built-in tags
            // (`!!binary`, `!!timestamp`) and untagged scalars stay as
            // `Scalar` and re-resolve on each access — they don't need
            // schema availability at read time.
            if let Some(loader_fn) = lookup_loader(py, schema, s.meta.tag.as_deref()) {
                let tag = s.meta.tag.as_deref().unwrap_or("?").to_owned();
                let default_val = scalar_to_py(py, s.value())?;
                let py_obj = call_loader(py, &loader_fn, &tag, default_val)?;
                if let Some(name) = s.meta.anchor.clone() {
                    ctx.register(name, &py_obj, py);
                }
                *node = YamlNode::OpaquePy(py_obj);
            }
        }
        YamlNode::Alias {
            name,
            resolved,
            materialised,
            ..
        } => {
            // Fill `materialised` so `__getitem__` returns the anchor's Py
            // (identity sharing) while the `Alias` variant is preserved for
            // round-trip and `get_alias()`.
            if materialised.is_none() {
                let py_obj = if let Some(cached) = ctx.lookup(name, py) {
                    cached
                } else {
                    // No anchor cached yet — materialise the resolved subtree
                    // as a fresh standalone Py.
                    node_to_py_inner(py, resolved, schema, ctx)?
                };
                *materialised = Some(py_obj);
            }
        }
        YamlNode::Null | YamlNode::Container(_) | YamlNode::OpaquePy(_) => {}
    }
    Ok(())
}

/// Create a `PyYamlSequence` from a Rust `YamlSequence`.
///
/// Top-level wrapper that constructs a fresh [`LoadCtx`]. Use
/// [`sequence_to_py_obj_inner`] when recursing inside an existing context so
/// anchored items surface as the same Python object across aliases.
pub(crate) fn sequence_to_py_obj(
    py: Python<'_>,
    s: YamlSequence,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    let mut ctx = LoadCtx::default();
    sequence_to_py_obj_inner(py, s, meta, schema, &mut ctx)
}

pub(crate) fn sequence_to_py_obj_inner(
    py: Python<'_>,
    mut s: YamlSequence,
    meta: DocMetadata,
    schema: Option<&Bound<'_, Schema>>,
    ctx: &mut LoadCtx,
) -> PyResult<Py<PyAny>> {
    // Materialise nested containers / aliases in-place — see
    // `mapping_to_py_obj_inner` for the same pattern.
    for item in &mut s.items {
        materialise_node(py, item, schema, ctx)?;
    }

    let obj: Py<PyYamlSequence> = Py::new(
        py,
        (
            PyYamlSequence { inner: s },
            crate::py::py_node::PyYamlNode { meta },
        ),
    )?;

    Ok(obj.into_any())
}

pub(crate) fn node_repr(py: Python<'_>, node: &YamlNode) -> String {
    match node {
        YamlNode::Mapping(m) => mapping_repr(py, m),
        YamlNode::Sequence(s) => sequence_repr(py, s),
        YamlNode::Scalar(s) => match &s.value() {
            ScalarValue::Null => "None".to_string(),
            ScalarValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            ScalarValue::Int(n) => n.to_string(),
            ScalarValue::Float(f) => format!("{f}"),
            ScalarValue::Str(s) => format!("{s:?}"),
        },
        YamlNode::Null => "None".to_string(),
        YamlNode::Alias { resolved, .. } => node_repr(py, resolved),
        YamlNode::Container(p) | YamlNode::OpaquePy(p) => p
            .bind(py)
            .repr()
            .map_or_else(|_| "<opaque>".to_string(), |s| s.to_string()),
    }
}

pub(crate) fn mapping_repr(py: Python<'_>, m: &YamlMapping) -> String {
    let pairs: Vec<String> = m
        .entries
        .iter()
        .map(|(k, e)| format!("{k:?}: {}", node_repr(py, &e.value)))
        .collect();
    format!("YamlMapping({{{}}})", pairs.join(", "))
}

pub(crate) fn sequence_repr(py: Python<'_>, s: &YamlSequence) -> String {
    let items: Vec<String> = s.items.iter().map(|i| node_repr(py, i)).collect();
    format!("YamlSequence([{}])", items.join(", "))
}

pub(crate) fn node_to_python(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, None),
        YamlNode::Mapping(m) => mapping_to_python(py, m),
        YamlNode::Sequence(s) => sequence_to_python(py, s),
        YamlNode::Alias { resolved, .. } => node_to_python(py, resolved),
        // `Container` slots hold a typed `PyYamlMapping`/`PyYamlSequence` —
        // recurse so `to_python()` yields a fully-plain tree. `OpaquePy`
        // values (loader output / unrecognised user objects) pass through
        // unchanged.
        YamlNode::Container(p) => {
            let bound = p.bind(py);
            if let Ok(child_m) = bound.cast::<PyYamlMapping>() {
                return mapping_to_python(py, &child_m.borrow().inner);
            }
            if let Ok(child_s) = bound.cast::<PyYamlSequence>() {
                return sequence_to_python(py, &child_s.borrow().inner);
            }
            Ok(p.clone_ref(py))
        }
        YamlNode::OpaquePy(p) => Ok(p.clone_ref(py)),
    }
}

pub(crate) fn mapping_to_python(py: Python<'_>, m: &YamlMapping) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    for (k, e) in &m.entries {
        let v = node_to_python(py, &e.value)?;
        d.set_item(k.python_key(), v)?;
    }
    Ok(d.into_any().unbind())
}

pub(crate) fn sequence_to_python(py: Python<'_>, s: &YamlSequence) -> PyResult<Py<PyAny>> {
    let items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|i| node_to_python(py, i))
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
    // Check if the iterator stored an IO error (e.g. no read() method,
    // read() returned wrong type, or invalid UTF-8 bytes).
    if let Ok(mut guard) = error_slot.lock()
        && let Some(err) = guard.take()
    {
        return Err(err);
    }
    result.map_err(ParseError::new_err)
}
