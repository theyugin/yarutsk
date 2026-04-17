// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::py_mapping::PyYamlMapping;
use super::py_scalar::PyYamlScalar;
use super::py_sequence::PyYamlSequence;
use super::schema::Schema;
use crate::core::builder::{ParseOutput, parse_iter, parse_str};
use crate::core::types::*;
use crate::{DumperError, LoaderError, ParseError};

// ─── Cycle detection ─────────────────────────────────────────────────────────

// Thread-local set of Python object pointer IDs currently on the serialisation
// call stack.  Used to detect self-referential dicts/lists/tuples before they
// overflow the Rust stack.
thread_local! {
    static CYCLE_GUARD: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

/// RAII guard: inserts *ptr* into `CYCLE_GUARD` on creation and removes it on
/// drop, even if the enclosing function returns an error.
struct CycleGuard(usize);

impl CycleGuard {
    /// Returns `Some(guard)` if *ptr* was not already in the set (safe to
    /// recurse), or `None` if a cycle is detected.
    fn enter(ptr: usize) -> Option<Self> {
        CYCLE_GUARD.with(|s| {
            if s.borrow_mut().insert(ptr) {
                Some(CycleGuard(ptr))
            } else {
                None
            }
        })
    }
}

impl Drop for CycleGuard {
    fn drop(&mut self) {
        CYCLE_GUARD.with(|s| {
            s.borrow_mut().remove(&self.0);
        });
    }
}

// ─── Auto-anchor state ───────────────────────────────────────────────────────

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

thread_local! {
    static ANCHOR_EMIT: RefCell<Option<AnchorEmitState>> = const { RefCell::new(None) };
}

/// Walk plain Python containers (dict/list/tuple), counting how many times each
/// object identity appears.  Objects seen more than once — including objects that
/// are their own ancestor (cycles) — will receive a YAML anchor during emit.
///
/// `PyYamlMapping`/`PyYamlSequence` are skipped: they clone `inner` on extract
/// so Python-level cycles through them are impossible.
fn prepass(obj: &Bound<'_, PyAny>, ref_count: &mut HashMap<usize, usize>) {
    // Must check our custom types *before* PyDict/PyList — PyYamlMapping extends PyDict.
    // Descend into their parent dict/list values so that plain containers
    // embedded inside a YamlMapping/YamlSequence are discovered for
    // anchor/alias handling (including cycle detection).
    if obj.cast::<PyYamlMapping>().is_ok() {
        let ptr = obj.as_ptr() as usize;
        if *ref_count.entry(ptr).or_insert(0) > 0 {
            return; // already visited — avoid infinite recursion
        }
        *ref_count.entry(ptr).or_insert(0) = 1;
        if let Ok(d) = obj.cast::<PyDict>() {
            for (_, v) in d.iter() {
                prepass(&v, ref_count);
            }
        }
        return;
    }
    if obj.cast::<PyYamlSequence>().is_ok() {
        let ptr = obj.as_ptr() as usize;
        if *ref_count.entry(ptr).or_insert(0) > 0 {
            return;
        }
        *ref_count.entry(ptr).or_insert(0) = 1;
        if let Ok(l) = obj.cast::<PyList>() {
            for item in l.iter() {
                prepass(&item, ref_count);
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

/// Initialise per-document anchor state.  Run the pre-pass over *doc* to find
/// all plain containers that appear more than once, then store the result in the
/// thread-local `ANCHOR_EMIT`.  Call `clear_anchor_state` when serialisation is
/// complete.
pub(crate) fn init_anchor_state(doc: &Bound<'_, PyAny>) {
    let mut ref_count: HashMap<usize, usize> = HashMap::new();
    prepass(doc, &mut ref_count);
    let anchors = ref_count
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(ptr, _)| (ptr, None))
        .collect();
    ANCHOR_EMIT.with(|s| {
        *s.borrow_mut() = Some(AnchorEmitState {
            anchors,
            counter: 0,
        });
    });
}

/// Clear the anchor state after serialising a document.
pub(crate) fn clear_anchor_state() {
    ANCHOR_EMIT.with(|s| {
        *s.borrow_mut() = None;
    });
}

/// Check whether *ptr* needs special anchor/alias treatment during emit.
///
/// Returns `(Some(alias_name), None)` if the object was already serialised
/// and should be emitted as `*alias_name`.
///
/// Returns `(None, Some(anchor_name))` if this is the first encounter of a
/// multi-ref object; the caller should attach `anchor_name` to the node.
///
/// Returns `(None, None)` if no anchor tracking is needed.
fn check_anchor(ptr: usize) -> (Option<String>, Option<String>) {
    ANCHOR_EMIT.with(|s| {
        let mut borrow = s.borrow_mut();
        if let Some(st) = borrow.as_mut()
            && st.anchors.contains_key(&ptr)
        {
            // Clone the current slot value to avoid holding a borrow while
            // we call `next_name` (which mutably borrows `st.counter`).
            let current = st.anchors.get(&ptr).and_then(|v| v.clone());
            match current {
                Some(name) => return (Some(name), None), // emit alias
                None => {
                    // First encounter of a multi-ref object — assign anchor.
                    let name = st.next_name();
                    st.anchors.insert(ptr, Some(name.clone()));
                    return (None, Some(name));
                }
            }
        }
        (None, None)
    })
}

// ─── Scalar conversion ────────────────────────────────────────────────────────

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
    if let Some(schema_bound) = schema {
        let loader = {
            let sr = schema_bound.borrow();
            s.tag
                .as_deref()
                .and_then(|t| sr.loaders.get(t).map(|f| f.clone_ref(py)))
        };
        if let Some(loader_fn) = loader {
            let tag_name = s.tag.as_deref().unwrap_or("?");
            let default_val = scalar_to_py(py, &s.value)?;
            return loader_fn
                .bind(py)
                .call1((default_val,))
                .map(|v| v.unbind())
                .map_err(|e| {
                    LoaderError::new_err(format!("Schema loader for tag '{tag_name}' raised: {e}"))
                });
        }
    }
    match s.tag.as_deref() {
        Some("!!binary") | Some("tag:yaml.org,2002:binary") => {
            let raw = s
                .original
                .as_deref()
                .or(if let ScalarValue::Str(ref st) = s.value {
                    Some(st.as_str())
                } else {
                    None
                })
                .unwrap_or("");
            let stripped: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            use base64::{Engine, engine::general_purpose::STANDARD};
            let bytes = STANDARD
                .decode(stripped.as_bytes())
                .map_err(|e| PyRuntimeError::new_err(format!("!!binary decode error: {e}")))?;
            use pyo3::types::PyBytes;
            Ok(PyBytes::new(py, &bytes).into_any().unbind())
        }
        Some("!!timestamp") | Some("tag:yaml.org,2002:timestamp") => {
            let raw = s
                .original
                .as_deref()
                .or(if let ScalarValue::Str(ref st) = s.value {
                    Some(st.as_str())
                } else {
                    None
                })
                .unwrap_or("");
            // YAML allows a space in place of 'T' between date and time.
            let normalized = raw.replacen(' ', "T", 1);
            let datetime_mod = py.import("datetime")?;
            // Date-only values contain no 'T' and no time ':' after the date portion.
            if !normalized.contains('T') && normalized.len() > 5 && !normalized[5..].contains(':') {
                let date = datetime_mod
                    .getattr("date")?
                    .call_method1("fromisoformat", (&*normalized,))?;
                Ok(date.into_any().unbind())
            } else {
                let dt = datetime_mod
                    .getattr("datetime")?
                    .call_method1("fromisoformat", (&*normalized,))?;
                Ok(dt.into_any().unbind())
            }
        }
        _ => scalar_to_py(py, &s.value),
    }
}

// ─── Node ↔ Python conversion ─────────────────────────────────────────────────

/// Construct a plain (unquoted, untagged) `YamlNode::Scalar` from a typed value.
/// Used when converting Python primitives to YAML nodes during dump.
pub(crate) fn plain_scalar(value: ScalarValue) -> YamlNode {
    YamlNode::Scalar(YamlScalar {
        value,
        style: ScalarStyle::Plain,
        tag: None,
        original: None,
        anchor: None,
    })
}

/// A `YamlEntry` with no comments, no blank lines, and plain key style.
/// Used when inserting entries via Python mutations (dict ops, update, etc.).
pub(crate) fn plain_entry(value: YamlNode) -> YamlEntry {
    YamlEntry {
        value,
        comment_before: None,
        comment_inline: None,
        blank_lines_before: 0,
        key_style: ScalarStyle::Plain,
        key_anchor: None,
        key_alias: None,
        key_tag: None,
        key_node: None,
    }
}

/// A `YamlItem` with no comments and no blank lines.
/// Used when inserting items via Python mutations (append, insert, extend, etc.).
pub(crate) fn plain_item(value: YamlNode) -> YamlItem {
    YamlItem {
        value,
        comment_before: None,
        comment_inline: None,
        blank_lines_before: 0,
    }
}

/// Resolve a Python sequence index (supports negative indices).
/// Returns an error if the index is out of range.
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

/// Convert a YamlNode to its Python representation.
/// Mapping → PyYamlMapping (dict subclass), Sequence → PyYamlSequence (list subclass),
/// scalar/null → Python primitive.
pub(crate) fn node_to_py(
    py: Python<'_>,
    node: &YamlNode,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, schema),
        YamlNode::Mapping(m) => {
            let tag = m.tag.clone();
            let py_obj = mapping_to_py_obj(py, m.clone(), DocMeta::none(), schema)?;
            apply_loader(py, schema, tag.as_deref(), py_obj)
        }
        YamlNode::Sequence(s) => {
            let tag = s.tag.clone();
            let py_obj = sequence_to_py_obj(py, s.clone(), DocMeta::none(), schema)?;
            apply_loader(py, schema, tag.as_deref(), py_obj)
        }
        YamlNode::Alias { resolved, .. } => node_to_py(py, resolved, schema),
    }
}

/// If *schema* has a loader for *tag*, call it with *py_obj* and return the result.
/// Otherwise return *py_obj* unchanged.
pub(crate) fn apply_loader(
    py: Python<'_>,
    schema: Option<&Bound<'_, Schema>>,
    tag: Option<&str>,
    py_obj: Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    if let (Some(schema_bound), Some(t)) = (schema, tag) {
        let loader = {
            let sr = schema_bound.borrow();
            sr.loaders.get(t).map(|f| f.clone_ref(py))
        };
        if let Some(loader_fn) = loader {
            return loader_fn
                .bind(py)
                .call1((py_obj,))
                .map(|v| v.unbind())
                .map_err(|e| {
                    LoaderError::new_err(format!("Schema loader for tag '{t}' raised: {e}"))
                });
        }
    }
    Ok(py_obj)
}

/// Convert a Python primitive (None/bool/int/float/str) to a scalar YamlNode.
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

/// Convert a Python object to a YamlNode.
pub(crate) fn py_to_node(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
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
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "?".to_string());
            let call_result = dumper_fn.bind(obj.py()).call1((obj,)).map_err(|e| {
                DumperError::new_err(format!("Schema dumper for {type_name} raised: {e}"))
            })?;
            let (tag, data): (String, Bound<'_, PyAny>) = call_result.extract().map_err(|e| {
                DumperError::new_err(format!(
                    "Schema dumper for {type_name} must return (tag, data) tuple: {e}"
                ))
            })?;
            let mut node = py_to_node(&data, schema)?;
            match &mut node {
                YamlNode::Scalar(s) => s.tag = Some(tag),
                YamlNode::Mapping(m) => m.tag = Some(tag),
                YamlNode::Sequence(s) => s.tag = Some(tag),
                _ => {}
            }
            return Ok(node);
        }
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
    if let Some(node) = py_primitive_to_scalar(obj) {
        return Ok(node);
    }
    // tuple / list → sequence (checked before bytes to prevent small-integer
    // collections being misinterpreted as Vec<u8> by PyO3's sequence extraction).
    if let Ok(t) = obj.cast::<PyTuple>() {
        let ptr = obj.as_ptr() as usize;
        let _guard = CycleGuard::enter(ptr).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "cannot serialize a recursive structure: self-referential tuple detected",
            )
        })?;
        let mut seq = YamlSequence::new();
        for item in t.iter() {
            seq.items.push(plain_item(py_to_node(&item, schema)?));
        }
        return Ok(YamlNode::Sequence(seq));
    }
    if let Ok(l) = obj.cast::<PyList>() {
        let ptr = obj.as_ptr() as usize;
        let _guard = CycleGuard::enter(ptr).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "cannot serialize a recursive structure: self-referential list detected",
            )
        })?;
        let mut seq = YamlSequence::new();
        for item in l.iter() {
            seq.items.push(plain_item(py_to_node(&item, schema)?));
        }
        return Ok(YamlNode::Sequence(seq));
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
            value: ScalarValue::Str(encoded),
            style: ScalarStyle::Plain,
            tag: Some("!!binary".to_owned()),
            original: None,
            anchor: None,
        }));
    }
    // datetime.datetime / datetime.date → !!timestamp scalar
    {
        let datetime_mod = obj.py().import("datetime")?;
        let datetime_type = datetime_mod.getattr("datetime")?;
        let date_type = datetime_mod.getattr("date")?;
        if obj.is_instance(&datetime_type)? || obj.is_instance(&date_type)? {
            let iso: String = obj.call_method0("isoformat")?.extract()?;
            return Ok(YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str(iso),
                style: ScalarStyle::Plain,
                tag: Some("!!timestamp".to_owned()),
                original: None,
                anchor: None,
            }));
        }
    }
    // Plain dict fallback (for users passing native Python dicts).
    // Note: PyYamlMapping extends PyDict so it would match cast::<PyDict>() too,
    // but we already handled it with extract::<PyYamlMapping>() above.
    // Plain list is already handled above before the bytes check.
    if let Ok(d) = obj.cast::<PyDict>() {
        let ptr = obj.as_ptr() as usize;
        let _guard = CycleGuard::enter(ptr).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "cannot serialize a recursive structure: self-referential dict detected",
            )
        })?;
        let mut mapping = YamlMapping::new();
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            mapping
                .entries
                .insert(key, plain_entry(py_to_node(&v, schema)?));
        }
        return Ok(YamlNode::Mapping(mapping));
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
                mapping
                    .entries
                    .insert(key, plain_entry(py_to_node(&val, schema)?));
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
            seq.items.push(plain_item(py_to_node(&item, schema)?));
        }
        return Ok(YamlNode::Sequence(seq));
    }
    Err(PyRuntimeError::new_err(format!(
        "Cannot convert {obj} to a YAML node"
    )))
}

// ─── Document metadata ────────────────────────────────────────────────────────

/// Document-level metadata attached to every top-level YAML node.
pub(crate) struct DocMeta {
    pub(crate) explicit_start: bool,
    pub(crate) explicit_end: bool,
    pub(crate) yaml_version: Option<(u8, u8)>,
    pub(crate) tag_directives: Vec<(String, String)>,
}

impl DocMeta {
    pub(crate) fn none() -> Self {
        DocMeta {
            explicit_start: false,
            explicit_end: false,
            yaml_version: None,
            tag_directives: vec![],
        }
    }
}

/// Convert a top-level YamlNode to PyYamlMapping, PyYamlSequence, or PyYamlScalar.
pub(crate) fn node_to_doc(
    py: Python<'_>,
    node: YamlNode,
    meta: DocMeta,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Mapping(m) => mapping_to_py_obj(py, m, meta, schema),
        YamlNode::Sequence(s) => sequence_to_py_obj(py, s, meta, schema),
        other => Ok(PyYamlScalar {
            inner: other,
            explicit_start: meta.explicit_start,
            explicit_end: meta.explicit_end,
            yaml_version: meta.yaml_version,
            tag_directives: meta.tag_directives,
        }
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
pub(crate) fn extract_yaml_node(
    obj: &Bound<'_, PyAny>,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<YamlNode> {
    if let Ok(bound_m) = obj.cast::<PyYamlMapping>() {
        let ptr = obj.as_ptr() as usize;
        let _guard = CycleGuard::enter(ptr)
            .ok_or_else(|| PyRuntimeError::new_err("self-referential structure detected"))?;
        let borrow = bound_m.borrow();
        let dict_part = bound_m.as_super();
        let mut mapping = YamlMapping::with_capacity(borrow.inner.entries.len());
        // Preserve container style, tag, anchor, and trailing blank lines from inner.
        mapping.style = borrow.inner.style;
        mapping.tag = borrow.inner.tag.clone();
        mapping.anchor = borrow.inner.anchor.clone();
        mapping.trailing_blank_lines = borrow.inner.trailing_blank_lines;
        // Walk inner.entries for key order and comment data.
        // For scalar/null values, inner.entries[k].value is always current and has
        // the original style/tag info, so use it directly.
        // For container values, read from the parent dict so that any mutations to
        // returned child objects (which don't propagate back to inner) are visible.
        for (k, e) in &borrow.inner.entries {
            let node = match &e.value {
                YamlNode::Scalar(_) | YamlNode::Null | YamlNode::Alias { .. } => e.value.clone(),
                _ => {
                    let py_val = match dict_part.get_item(k)? {
                        Some(v) => v,
                        None => continue, // key was removed; skip
                    };
                    extract_yaml_node(&py_val, schema)?
                }
            };
            mapping.entries.insert(
                k.clone(),
                YamlEntry {
                    value: node,
                    ..e.clone()
                },
            );
        }
        return Ok(YamlNode::Mapping(mapping));
    }
    if let Ok(bound_s) = obj.cast::<PyYamlSequence>() {
        let ptr = obj.as_ptr() as usize;
        let _guard = CycleGuard::enter(ptr)
            .ok_or_else(|| PyRuntimeError::new_err("self-referential structure detected"))?;
        let borrow = bound_s.borrow();
        let list_part = bound_s.as_super();
        let inner_len = borrow.inner.items.len();
        let mut seq = YamlSequence::with_capacity(inner_len);
        // Preserve container style, tag, anchor, and trailing blank lines from inner.
        seq.style = borrow.inner.style;
        seq.tag = borrow.inner.tag.clone();
        seq.anchor = borrow.inner.anchor.clone();
        seq.trailing_blank_lines = borrow.inner.trailing_blank_lines;
        for i in 0..inner_len {
            let node = match &borrow.inner.items[i].value {
                YamlNode::Scalar(_) | YamlNode::Null | YamlNode::Alias { .. } => {
                    borrow.inner.items[i].value.clone()
                }
                _ => {
                    let py_val = list_part.get_item(i)?;
                    extract_yaml_node(&py_val, schema)?
                }
            };
            seq.items.push(YamlItem {
                value: node,
                ..borrow.inner.items[i].clone()
            });
        }
        return Ok(YamlNode::Sequence(seq));
    }
    if let Ok(sc) = obj.extract::<PyYamlScalar>() {
        return Ok(sc.inner);
    }
    // Scalars passed directly (int, str, etc.)
    if let Some(node) = py_primitive_to_scalar(obj) {
        return Ok(node);
    }
    // Plain dict fallback — no comment/style metadata, but values are correct.
    // Uses extract_yaml_node recursively so nested YamlMapping/YamlSequence
    // objects inside the dict still preserve their metadata.
    if let Ok(d) = obj.cast::<PyDict>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = check_anchor(ptr);
        if let Some(name) = alias {
            return Ok(YamlNode::Alias {
                name,
                resolved: Box::new(YamlNode::Null),
            });
        }
        let mut mapping = YamlMapping::new();
        if let Some(ref name) = anchor {
            mapping.anchor = Some(name.clone());
        }
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            mapping
                .entries
                .insert(key, plain_entry(extract_yaml_node(&v, schema)?));
        }
        return Ok(YamlNode::Mapping(mapping));
    }
    if let Ok(l) = obj.cast::<PyList>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = check_anchor(ptr);
        if let Some(name) = alias {
            return Ok(YamlNode::Alias {
                name,
                resolved: Box::new(YamlNode::Null),
            });
        }
        let mut seq = YamlSequence::new();
        if let Some(ref name) = anchor {
            seq.anchor = Some(name.clone());
        }
        for item in l.iter() {
            seq.items
                .push(plain_item(extract_yaml_node(&item, schema)?));
        }
        return Ok(YamlNode::Sequence(seq));
    }
    if let Ok(t) = obj.cast::<PyTuple>() {
        let ptr = obj.as_ptr() as usize;
        let (alias, anchor) = check_anchor(ptr);
        if let Some(name) = alias {
            return Ok(YamlNode::Alias {
                name,
                resolved: Box::new(YamlNode::Null),
            });
        }
        let mut seq = YamlSequence::new();
        if let Some(ref name) = anchor {
            seq.anchor = Some(name.clone());
        }
        for item in t.iter() {
            seq.items
                .push(plain_item(extract_yaml_node(&item, schema)?));
        }
        return Ok(YamlNode::Sequence(seq));
    }
    // Fall through to py_to_node for bytes, datetime, schema dumpers, and
    // abstract Mapping/Iterable types.
    py_to_node(obj, schema)
}

// ─── Python object creation helpers ──────────────────────────────────────────

/// Create a PyYamlMapping (dict subclass) from a Rust YamlMapping.
/// The parent dict is populated with the mapping's entries.
pub(crate) fn mapping_to_py_obj(
    py: Python<'_>,
    m: YamlMapping,
    meta: DocMeta,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    // Build Python values before moving m into the struct.
    let py_pairs: Vec<(String, Py<PyAny>)> = m
        .entries
        .iter()
        .map(|(k, e)| {
            let v = node_to_py(py, &e.value, schema)?;
            Ok((k.clone(), v))
        })
        .collect::<PyResult<_>>()?;

    let obj: Py<PyYamlMapping> = Py::new(
        py,
        PyYamlMapping {
            inner: m,
            explicit_start: meta.explicit_start,
            explicit_end: meta.explicit_end,
            yaml_version: meta.yaml_version,
            tag_directives: meta.tag_directives,
        },
    )?;

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
pub(crate) fn sequence_to_py_obj(
    py: Python<'_>,
    s: YamlSequence,
    meta: DocMeta,
    schema: Option<&Bound<'_, Schema>>,
) -> PyResult<Py<PyAny>> {
    // Build Python values before moving s into the struct.
    let py_items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|i| node_to_py(py, &i.value, schema))
        .collect::<PyResult<_>>()?;

    let obj: Py<PyYamlSequence> = Py::new(
        py,
        PyYamlSequence {
            inner: s,
            explicit_start: meta.explicit_start,
            explicit_end: meta.explicit_end,
            yaml_version: meta.yaml_version,
            tag_directives: meta.tag_directives,
        },
    )?;

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

pub(crate) fn node_repr(py: Python<'_>, node: &YamlNode) -> String {
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
        YamlNode::Alias { resolved, .. } => node_repr(py, resolved),
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
    let items: Vec<String> = s.items.iter().map(|i| node_repr(py, &i.value)).collect();
    format!("YamlSequence([{}])", items.join(", "))
}

// ─── Dict conversion helpers ──────────────────────────────────────────────────

pub(crate) fn node_to_dict(py: Python<'_>, node: &YamlNode) -> PyResult<Py<PyAny>> {
    match node {
        YamlNode::Null => Ok(py.None()),
        YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
        YamlNode::Mapping(m) => mapping_to_dict(py, m),
        YamlNode::Sequence(s) => sequence_to_dict(py, s),
        YamlNode::Alias { resolved, .. } => node_to_dict(py, resolved),
    }
}

pub(crate) fn mapping_to_dict(py: Python<'_>, m: &YamlMapping) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    for (k, e) in &m.entries {
        let v = node_to_dict(py, &e.value)?;
        d.set_item(k, v)?;
    }
    Ok(d.into_any().unbind())
}

pub(crate) fn sequence_to_dict(py: Python<'_>, s: &YamlSequence) -> PyResult<Py<PyAny>> {
    let items: Vec<Py<PyAny>> = s
        .items
        .iter()
        .map(|i| node_to_dict(py, &i.value))
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, items)?.into_any().unbind())
}

// ─── Parse / emit helpers ─────────────────────────────────────────────────────

pub(crate) fn parse_text(text: &str, schema: Option<&Schema>) -> PyResult<ParseOutput> {
    let policy = schema.and_then(Schema::tag_policy);
    parse_str(text, policy.as_ref()).map_err(|e| ParseError::new_err(e.to_string()))
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

// ─── Sort helpers ─────────────────────────────────────────────────────────────

pub(crate) fn py_compare<'py>(
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

pub(crate) fn sort_mapping(
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

// ─── Style-parsing helpers ────────────────────────────────────────────────────

pub(crate) fn parse_scalar_style(style: &str) -> PyResult<ScalarStyle> {
    match style {
        "plain" => Ok(ScalarStyle::Plain),
        "single" => Ok(ScalarStyle::SingleQuoted),
        "double" => Ok(ScalarStyle::DoubleQuoted),
        "literal" => Ok(ScalarStyle::Literal),
        "folded" => Ok(ScalarStyle::Folded),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown style {other:?}; expected plain/single/double/literal/folded"
        ))),
    }
}

pub(crate) fn parse_container_style(style: &str) -> PyResult<ContainerStyle> {
    match style {
        "block" => Ok(ContainerStyle::Block),
        "flow" => Ok(ContainerStyle::Flow),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown style {other:?}; expected \"block\" or \"flow\""
        ))),
    }
}

// ─── YAML version parsing ─────────────────────────────────────────────────────

/// Parse a YAML version string like `"1.2"` into `(major, minor)`.
pub(crate) fn parse_yaml_version(s: Option<&str>) -> PyResult<Option<(u8, u8)>> {
    match s {
        None => Ok(None),
        Some(v) => v
            .split_once('.')
            .and_then(|(maj, min)| Some((maj.parse::<u8>().ok()?, min.parse::<u8>().ok()?)))
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "invalid YAML version {v:?}; expected \"major.minor\" (e.g. \"1.2\")"
                ))
            })
            .map(Some),
    }
}
