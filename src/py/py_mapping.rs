// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use super::convert::{
    DocMeta, mapping_repr, mapping_to_dict, mapping_to_py_obj, node_to_doc, node_to_py,
    parse_container_style, parse_yaml_version, plain_entry, py_to_node, sort_mapping,
};
use super::py_sequence::PyYamlSequence;
use super::schema::Schema;
use crate::core::types::*;

// ─── PyYamlMapping (Python: YamlMapping extends dict) ─────────────────────────

/// A YAML mapping node. Subclass of dict; the parent dict is always kept in
/// sync with `inner` so that standard dict operations work transparently.
#[pyclass(name = "YamlMapping", extends = PyDict, from_py_object)]
#[derive(Clone)]
pub struct PyYamlMapping {
    pub(crate) inner: YamlMapping,
    /// True when the document this mapping belongs to had an explicit `---` marker.
    pub explicit_start: bool,
    /// True when the document this mapping belongs to had an explicit `...` marker.
    pub explicit_end: bool,
    /// `%YAML major.minor` directive for this document, if any.
    pub yaml_version: Option<(u8, u8)>,
    /// `%TAG handle prefix` pairs for this document.
    pub tag_directives: Vec<(String, String)>,
}

#[pymethods]
impl PyYamlMapping {
    #[new]
    #[pyo3(signature = (mapping = None, *, style = "block", tag = None, schema = None))]
    fn new(
        mapping: Option<&Bound<'_, PyAny>>,
        style: &str,
        tag: Option<&str>,
        schema: Option<Py<Schema>>,
    ) -> PyResult<Self> {
        let _ = (mapping, schema); // populated in __init__ once the parent dict is available
        let mut inner = YamlMapping::new();
        inner.style = parse_container_style(style)?;
        inner.tag = tag.map(str::to_owned);
        Ok(PyYamlMapping {
            inner,
            explicit_start: false,
            explicit_end: false,
            yaml_version: None,
            tag_directives: vec![],
        })
    }

    // Intercept __init__ so that Python does not forward args to dict.__init__,
    // which would otherwise insert them as dict entries. Populate from `mapping`
    // here because the parent dict is accessible via slf.as_super() at this point.
    #[pyo3(signature = (mapping = None, *, style = "block", tag = None, schema = None))]
    fn __init__(
        slf: &Bound<'_, Self>,
        mapping: Option<&Bound<'_, PyAny>>,
        style: &str,
        tag: Option<&str>,
        schema: Option<Py<Schema>>,
    ) -> PyResult<()> {
        let _ = (style, tag); // already applied in __new__
        if let Some(m) = mapping {
            if let Some(ref schema_py) = schema {
                // Schema path: use py_to_node (which invokes schema dumpers) then
                // rebuild from the resulting YamlMapping node.
                let py = slf.py();
                let sb = schema_py.bind(py);
                let node = py_to_node(m, Some(sb))?;
                if let YamlNode::Mapping(parsed) = node {
                    let dict_part = slf.as_super();
                    let mut borrow = slf.borrow_mut();
                    // Preserve style/tag from __new__, overlay entries from parsed.
                    let style = borrow.inner.style;
                    let tag = borrow.inner.tag.clone();
                    borrow.inner = parsed;
                    borrow.inner.style = style;
                    borrow.inner.tag = tag;
                    drop(borrow);
                    // Sync the parent dict with Python-visible values.
                    let borrow = slf.borrow();
                    for (k, e) in &borrow.inner.entries {
                        let py_val = node_to_py(py, &e.value, Some(sb))?;
                        dict_part.set_item(k, py_val.bind(py))?;
                    }
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "YamlMapping requires a dict or mapping-like object",
                    ));
                }
            } else {
                PyYamlMapping::update(slf, m)?;
            }
        }
        Ok(())
    }

    // ── Mutations (must sync parent dict) ────────────────────────────────────

    fn __setitem__(slf: &Bound<'_, Self>, key: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        // For unknown types (custom Python objects), store an opaque empty-mapping
        // placeholder in inner so that extract_yaml_node reads the real value from
        // the parent dict at dump time (where the schema's dumpers can handle it).
        let (node, py_val) = match py_to_node(value, None) {
            Ok(n) => {
                let pv = node_to_py(py, &n, None)?;
                (n, pv)
            }
            Err(_) => (
                YamlNode::Mapping(YamlMapping::new()),
                value.clone().unbind(),
            ),
        };
        {
            let mut borrow = slf.borrow_mut();
            if let Some(entry) = borrow.inner.entries.get_mut(key) {
                entry.value = node;
            } else {
                borrow
                    .inner
                    .entries
                    .insert(key.to_owned(), plain_entry(node));
            }
        }
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
                node_to_py(py, &e.value, None)
            }
            None => match default {
                Some(d) => Ok(d),
                None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
            },
        }
    }

    fn clear(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.borrow_mut().inner.entries.clear();
        // PyDict::clear() calls PyDict_Clear at C level — does not re-enter our override.
        slf.as_super().clear();
        Ok(())
    }

    fn popitem(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
        let key = slf.borrow().inner.entries.last().map(|(k, _)| k.clone());
        match key {
            None => Err(PyKeyError::new_err("dictionary is empty")),
            Some(k) => {
                let py_val = slf
                    .as_super()
                    .get_item(&k)?
                    .map_or_else(|| py.None(), |v| v.unbind());
                Self::__delitem__(slf, &k)?;
                Ok((k, py_val))
            }
        }
    }

    fn copy(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.__copy__(py)
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
                let (node, py_val) = match py_to_node(&val, None) {
                    Ok(n) => {
                        let pv = node_to_py(py, &n, None)?;
                        (n, pv)
                    }
                    Err(_) => (YamlNode::Mapping(YamlMapping::new()), val.clone().unbind()),
                };
                slf.borrow_mut()
                    .inner
                    .entries
                    .insert(k.clone(), plain_entry(node));
                dict_part.set_item(k.as_str(), py_val.bind(py))?;
            }
            return Ok(());
        }
        for item in other.try_iter()? {
            let item = item?;
            let (k, val): (String, Bound<'_, PyAny>) = item.extract()?;
            let (node, py_val) = match py_to_node(&val, None) {
                Ok(n) => {
                    let pv = node_to_py(py, &n, None)?;
                    (n, pv)
                }
                Err(_) => (YamlNode::Mapping(YamlMapping::new()), val.clone().unbind()),
            };
            slf.borrow_mut()
                .inner
                .entries
                .insert(k.clone(), plain_entry(node));
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
            let dv = default_val.bind(py);
            let (node, py_val) = match py_to_node(dv, None) {
                Ok(n) => {
                    let pv = node_to_py(py, &n, None)?;
                    (n, pv)
                }
                Err(_) => (
                    YamlNode::Mapping(YamlMapping::new()),
                    default_val.clone_ref(py),
                ),
            };
            slf.borrow_mut()
                .inner
                .entries
                .insert(key.to_owned(), plain_entry(node));
            slf.as_super().set_item(key, py_val.bind(py))?;
        }
        // Return the real Python value from the parent dict (not node_to_py, which
        // would return an opaque placeholder for custom types stored via __setitem__).
        slf.as_super()
            .get_item(key)?
            .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            .map(|v| v.unbind())
    }

    #[pyo3(signature = (key=None, reverse=false, recursive=false))]
    pub fn sort_keys(
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
            // Preserve Python objects from the parent dict (same as the non-recursive
            // path) — do NOT call node_to_py, which would convert the empty-mapping
            // placeholder stored for custom types back to {}, losing the custom object.
            // For nested PyYamlMapping values, call sort_keys recursively so their
            // Python dict is synced to their already-sorted inner.
            let sorted_keys: Vec<String> = slf.borrow().inner.entries.keys().cloned().collect();
            let py_vals: Vec<Py<PyAny>> = sorted_keys
                .iter()
                .filter_map(|k| dict_part.get_item(k).ok()?.map(|v| v.unbind()))
                .collect();
            dict_part.clear();
            for (k, v) in sorted_keys.iter().zip(py_vals.iter()) {
                let py_val = v.bind(py);
                if let Ok(nested) = py_val.cast::<PyYamlMapping>() {
                    PyYamlMapping::sort_keys(
                        nested,
                        py,
                        key.as_ref().map(|k| k.clone_ref(py)),
                        reverse,
                        true,
                    )?;
                }
                dict_part.set_item(k.as_str(), py_val)?;
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

    /// Read or write the inline comment for *key*.
    /// `comment_inline(key)` returns the current comment (``str | None``).
    /// `comment_inline(key, comment)` sets it; pass ``None`` to clear.
    #[pyo3(signature = (key, *args))]
    fn comment_inline(
        &mut self,
        py: Python<'_>,
        key: &str,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        match args.len() {
            0 => Ok(self
                .inner
                .entries
                .get(key)
                .and_then(|e| e.comment_inline.clone())
                .into_pyobject(py)?
                .into_any()
                .unbind()),
            1 => {
                let comment: Option<String> = args.get_item(0)?.extract()?;
                match self.inner.entries.get_mut(key) {
                    Some(entry) => {
                        entry.comment_inline = comment;
                        Ok(py.None())
                    }
                    None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
                }
            }
            _ => Err(PyRuntimeError::new_err(
                "comment_inline takes 1 or 2 positional arguments",
            )),
        }
    }

    /// Read or write the block comment above *key*.
    /// `comment_before(key)` returns the current comment (``str | None``).
    /// `comment_before(key, comment)` sets it; pass ``None`` to clear.
    #[pyo3(signature = (key, *args))]
    fn comment_before(
        &mut self,
        py: Python<'_>,
        key: &str,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        match args.len() {
            0 => Ok(self
                .inner
                .entries
                .get(key)
                .and_then(|e| e.comment_before.clone())
                .into_pyobject(py)?
                .into_any()
                .unbind()),
            1 => {
                let comment: Option<String> = args.get_item(0)?.extract()?;
                match self.inner.entries.get_mut(key) {
                    Some(entry) => {
                        entry.comment_before = comment;
                        Ok(py.None())
                    }
                    None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
                }
            }
            _ => Err(PyRuntimeError::new_err(
                "comment_before takes 1 or 2 positional arguments",
            )),
        }
    }

    /// Return the inline comment for *key*, or ``None`` if unset.
    /// Raises ``KeyError`` if *key* is absent.
    fn get_comment_inline(&self, key: &str) -> PyResult<Option<String>> {
        self.inner
            .entries
            .get(key)
            .map(|e| e.comment_inline.clone())
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
    }

    /// Set the inline comment for *key*; pass ``None`` to clear.
    /// Raises ``KeyError`` if *key* is absent.
    fn set_comment_inline(&mut self, key: &str, comment: Option<&str>) -> PyResult<()> {
        self.inner
            .entries
            .get_mut(key)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
            .map(|e| {
                e.comment_inline = comment.map(str::to_owned);
            })
    }

    /// Return the block comment above *key*, or ``None`` if unset.
    /// Raises ``KeyError`` if *key* is absent.
    fn get_comment_before(&self, key: &str) -> PyResult<Option<String>> {
        self.inner
            .entries
            .get(key)
            .map(|e| e.comment_before.clone())
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
    }

    /// Set the block comment above *key*; pass ``None`` to clear.
    /// Raises ``KeyError`` if *key* is absent.
    fn set_comment_before(&mut self, key: &str, comment: Option<&str>) -> PyResult<()> {
        self.inner
            .entries
            .get_mut(key)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
            .map(|e| {
                e.comment_before = comment.map(str::to_owned);
            })
    }

    /// Return the YAML alias name if the value at *key* is an alias (``*name``), else ``None``.
    /// Raises ``KeyError`` if *key* is absent.
    fn alias_name(&self, key: &str) -> PyResult<Option<&str>> {
        match self.inner.entries.get(key) {
            Some(entry) => Ok(match &entry.value {
                YamlNode::Alias { name, .. } => Some(name.as_str()),
                _ => None,
            }),
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
        }
    }

    /// Mark the value at *key* as a YAML alias that emits ``*anchor_name``.
    /// The current value is kept as the resolved node so Python reads still work.
    /// Raises ``KeyError`` if *key* is absent.
    fn set_alias(&mut self, key: &str, anchor_name: &str) -> PyResult<()> {
        let entry = self
            .inner
            .entries
            .get_mut(key)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_owned()))?;
        let resolved = Box::new(entry.value.clone());
        entry.value = YamlNode::Alias {
            name: anchor_name.to_owned(),
            resolved,
        };
        Ok(())
    }

    /// The YAML tag on this mapping (e.g. ``"!!map"``), or ``None``.
    #[getter]
    fn get_tag(&self) -> Option<&str> {
        self.inner.tag.as_deref()
    }

    #[setter]
    fn set_tag(&mut self, tag: Option<&str>) {
        self.inner.tag = tag.map(str::to_owned);
    }

    /// The anchor name declared on this mapping (``&name``), or ``None``.
    #[getter]
    fn get_anchor(&self) -> Option<&str> {
        self.inner.anchor.as_deref()
    }

    #[setter]
    fn set_anchor(&mut self, anchor: Option<&str>) {
        self.inner.anchor = anchor.map(str::to_owned);
    }

    /// The container style: ``"block"`` or ``"flow"``.
    #[getter]
    fn get_style(&self) -> &str {
        match self.inner.style {
            ContainerStyle::Block => "block",
            ContainerStyle::Flow => "flow",
        }
    }

    #[setter]
    fn set_style(&mut self, style: &str) -> PyResult<()> {
        self.inner.style = parse_container_style(style)?;
        Ok(())
    }

    /// The number of blank lines emitted after all entries in this mapping.
    #[getter]
    fn get_trailing_blank_lines(&self) -> u8 {
        self.inner.trailing_blank_lines
    }

    #[setter]
    fn set_trailing_blank_lines(&mut self, n: u8) {
        self.inner.trailing_blank_lines = n;
    }

    /// Whether this document had an explicit `---` marker in the source.
    #[getter]
    fn get_explicit_start(&self) -> bool {
        self.explicit_start
    }

    #[setter]
    fn set_explicit_start(&mut self, value: bool) {
        self.explicit_start = value;
    }

    /// Whether this document had an explicit `...` marker in the source.
    #[getter]
    fn get_explicit_end(&self) -> bool {
        self.explicit_end
    }

    #[setter]
    fn set_explicit_end(&mut self, value: bool) {
        self.explicit_end = value;
    }

    /// The `%YAML` version directive for this document (e.g. ``"1.2"``), or ``None``.
    #[getter]
    fn get_yaml_version(&self) -> Option<String> {
        self.yaml_version.map(|(maj, min)| format!("{maj}.{min}"))
    }

    #[setter]
    fn set_yaml_version(&mut self, version: Option<&str>) -> PyResult<()> {
        self.yaml_version = parse_yaml_version(version)?;
        Ok(())
    }

    /// The ``%TAG`` directives for this document as a list of ``(handle, prefix)`` pairs.
    #[getter]
    fn get_tag_directives(&self) -> Vec<(String, String)> {
        self.tag_directives.clone()
    }

    #[setter]
    fn set_tag_directives(&mut self, directives: Vec<(String, String)>) {
        self.tag_directives = directives;
    }

    /// Return the underlying YAML node for a key as a YamlScalar, YamlMapping,
    /// or YamlSequence object, preserving style/tag metadata.
    /// Raises KeyError if the key is absent.
    fn node(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match self.inner.entries.get(key) {
            Some(entry) => Ok(node_to_doc(py, entry.value.clone(), DocMeta::none(), None)?),
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
        }
    }

    /// Set the scalar style for the value at `key`.
    /// `style` must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
    /// Raises ``KeyError`` if the key is absent; ``ValueError`` for unknown styles;
    /// ``TypeError`` if the value is not a scalar (use ``container_style()`` instead).
    fn scalar_style(&mut self, key: &str, style: &str) -> PyResult<()> {
        let new_style = match style {
            "plain" => ScalarStyle::Plain,
            "single" => ScalarStyle::SingleQuoted,
            "double" => ScalarStyle::DoubleQuoted,
            "literal" => ScalarStyle::Literal,
            "folded" => ScalarStyle::Folded,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown style {other:?}; expected plain/single/double/literal/folded"
                )));
            }
        };
        match self.inner.entries.get_mut(key) {
            Some(entry) => match &mut entry.value {
                YamlNode::Scalar(s) => {
                    s.style = new_style;
                    Ok(())
                }
                _ => Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "value at key {key:?} is not a scalar; use container_style() for mappings and sequences"
                ))),
            },
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
        }
    }

    /// Set the block/flow style for the container value at *key*.
    /// *style* must be ``"block"`` or ``"flow"``.
    /// No-op when the value is a scalar or null.
    /// Raises ``KeyError`` if *key* is absent; ``ValueError`` for unknown styles.
    fn container_style(slf: &Bound<'_, Self>, key: &str, style: &str) -> PyResult<()> {
        let new_style = match style {
            "block" => ContainerStyle::Block,
            "flow" => ContainerStyle::Flow,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown style {other:?}; expected \"block\" or \"flow\""
                )));
            }
        };
        {
            let mut borrow = slf.borrow_mut();
            match borrow.inner.entries.get_mut(key) {
                Some(entry) => {
                    match &mut entry.value {
                        YamlNode::Mapping(m) => m.style = new_style,
                        YamlNode::Sequence(s) => s.style = new_style,
                        _ => {} // scalar / null / alias — silently ignored
                    }
                }
                None => return Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
            }
        }
        // Also sync the Python-side object stored in the parent dict so that
        // extract_yaml_node (which reads inner.style from the child object) sees the change.
        if let Some(py_val) = slf.as_super().get_item(key)? {
            if let Ok(bound_m) = py_val.cast::<PyYamlMapping>() {
                bound_m.borrow_mut().inner.style = new_style;
            } else if let Ok(bound_s) = py_val.cast::<PyYamlSequence>() {
                bound_s.borrow_mut().inner.style = new_style;
            }
        }
        Ok(())
    }

    /// Read or write the number of blank lines emitted before *key*.
    /// `blank_lines_before(key)` returns the current count (``int``).
    /// `blank_lines_before(key, n)` sets it; values are clamped to 0–255.
    /// Raises ``KeyError`` if *key* is absent.
    #[pyo3(signature = (key, *args))]
    fn blank_lines_before(
        &mut self,
        py: Python<'_>,
        key: &str,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        match args.len() {
            0 => match self.inner.entries.get(key) {
                Some(entry) => Ok((entry.blank_lines_before as u32)
                    .into_pyobject(py)?
                    .into_any()
                    .unbind()),
                None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
            },
            1 => {
                let n: u32 = args.get_item(0)?.extract()?;
                let n = n.min(255) as u8;
                match self.inner.entries.get_mut(key) {
                    Some(entry) => {
                        entry.blank_lines_before = n;
                        Ok(py.None())
                    }
                    None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_owned())),
                }
            }
            _ => Err(PyRuntimeError::new_err(
                "blank_lines_before takes 1 or 2 positional arguments",
            )),
        }
    }

    /// Strip cosmetic formatting metadata, resetting to clean YAML defaults.
    ///
    /// All three keyword flags default to ``True``:
    ///
    /// - ``styles``: scalar quoting → plain (or ``literal`` for multi-line strings),
    ///   container style → block, scalar ``original`` values cleared.
    /// - ``comments``: ``comment_before`` and ``comment_inline`` cleared on all entries.
    /// - ``blank_lines``: ``blank_lines_before`` zeroed on all entries;
    ///   ``trailing_blank_lines`` zeroed on this container.
    ///
    /// Tags, anchors, and document-level markers are always preserved.
    /// Recurses into all nested containers.
    #[pyo3(signature = (*, styles=true, comments=true, blank_lines=true))]
    pub fn format(
        slf: &Bound<'_, Self>,
        styles: bool,
        comments: bool,
        blank_lines: bool,
    ) -> PyResult<()> {
        let opts = FormatOptions {
            styles,
            comments,
            blank_lines,
        };
        // Step 1: reset Rust-inner tree (covers scalar style and entry metadata)
        slf.borrow_mut().inner.format_with(opts);
        // Step 2: propagate to Python-side child objects (their own .inner must also be updated)
        for (_, val) in slf.as_super().iter() {
            if let Ok(child_m) = val.cast::<PyYamlMapping>() {
                PyYamlMapping::format(child_m, styles, comments, blank_lines)?;
            } else if let Ok(child_s) = val.cast::<PyYamlSequence>() {
                PyYamlSequence::format(child_s, styles, comments, blank_lines)?;
            }
        }
        Ok(())
    }

    /// Return a list of ``(key, node)`` pairs for all entries in this mapping.
    ///
    /// Each node is a ``YamlMapping``, ``YamlSequence``, or ``YamlScalar``,
    /// preserving style/tag metadata. Unlike ``items()``, which returns Python
    /// primitives, ``nodes()`` returns the full typed node objects.
    fn nodes(&self, py: Python<'_>) -> PyResult<Vec<(String, Py<PyAny>)>> {
        self.inner
            .entries
            .iter()
            .map(|(k, entry)| {
                let node = node_to_doc(py, entry.value.clone(), DocMeta::none(), None)?;
                Ok((k.clone(), node))
            })
            .collect()
    }

    /// Return a shallow copy of this mapping (comments, style metadata, and
    /// nested structure are all cloned).
    fn __copy__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let meta = DocMeta {
            explicit_start: self.explicit_start,
            explicit_end: self.explicit_end,
            yaml_version: self.yaml_version,
            tag_directives: self.tag_directives.clone(),
        };
        mapping_to_py_obj(py, self.inner.clone(), meta, None)
    }

    /// Return a deep copy of this mapping.
    ///
    /// Because ``YamlMapping`` owns all its data (no ``Rc``/``Arc`` sharing),
    /// the Rust ``Clone`` is already a deep copy. The *memo* dict is accepted
    /// for API compatibility but is not used.
    fn __deepcopy__(&self, py: Python<'_>, _memo: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        self.__copy__(py)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        mapping_repr(py, &self.inner)
    }
}
