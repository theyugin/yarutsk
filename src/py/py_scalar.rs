// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use pyo3::prelude::*;

use super::convert::{
    NodeParent, date_type, datetime_type, parse_scalar_style, parse_yaml_version,
    py_primitive_to_scalar, scalar_to_py_with_tag,
};
use crate::core::types::{FormatOptions, NodeMeta, ScalarStyle, ScalarValue, YamlNode, YamlScalar};

// ─── PyYamlScalar (Python: YamlScalar) ───────────────────────────────────────

/// A YAML scalar document node (int, float, bool, str, or null).
#[pyclass(name = "YamlScalar", from_py_object)]
#[derive(Clone)]
pub struct PyYamlScalar {
    pub(crate) inner: YamlNode, // YamlNode::Scalar or YamlNode::Null
    /// True when the document this node belongs to had an explicit `---` marker.
    #[pyo3(get, set)]
    pub explicit_start: bool,
    /// True when the document this node belongs to had an explicit `...` marker.
    #[pyo3(get, set)]
    pub explicit_end: bool,
    /// `%YAML major.minor` directive for this document, if any.
    /// Exposed to Python as a `"major.minor"` string via manual getter/setter.
    pub yaml_version: Option<(u8, u8)>,
    /// `%TAG handle prefix` pairs for this document.
    #[pyo3(get, set)]
    pub tag_directives: Vec<(String, String)>,
    /// Back-reference to the containing mapping/sequence when this scalar was
    /// obtained via `YamlMapping.node(key)` / `YamlSequence.node(idx)`. All
    /// setters propagate mutations through this reference so that mutations
    /// on `m.node(k)` land in the parent's `inner` instead of a dead clone.
    pub(crate) parent: NodeParent,
}

#[pymethods]
impl PyYamlScalar {
    /// Create a new ``YamlScalar`` from a Python value.
    ///
    /// *value* can be ``str``, ``int``, ``float``, ``bool``, ``None``,
    /// ``bytes``, ``bytearray``, ``datetime.datetime``, or ``datetime.date``.
    /// *style* controls the quoting style when serialized; defaults to ``"plain"``.
    /// *tag* is an optional YAML tag string (e.g. ``"!mytag"``).
    #[new]
    #[pyo3(signature = (value, *, style = "plain", tag = None))]
    fn new(value: &Bound<'_, PyAny>, style: &str, tag: Option<&str>) -> PyResult<Self> {
        let scalar_style = parse_scalar_style(style)?;
        // Try plain primitives first.
        let inner = if let Some(node) = py_primitive_to_scalar(value) {
            match node {
                YamlNode::Scalar(mut s) => {
                    s.style = scalar_style;
                    s.meta.tag = tag.map(str::to_owned);
                    YamlNode::Scalar(s)
                }
                other => other, // Null
            }
        } else if (value.is_instance_of::<pyo3::types::PyBytes>()
            || value.is_instance_of::<pyo3::types::PyByteArray>())
            && let Ok(b) = value.extract::<Vec<u8>>()
        {
            use base64::{Engine, engine::general_purpose::STANDARD};
            YamlNode::Scalar(YamlScalar {
                value: ScalarValue::Str(STANDARD.encode(&b)),
                style: scalar_style,
                original: None,
                chomping: None,
                meta: NodeMeta {
                    tag: Some(tag.unwrap_or("!!binary").to_owned()),
                    ..NodeMeta::default()
                },
            })
        } else {
            // datetime.datetime / datetime.date
            let py = value.py();
            if value.is_instance(datetime_type(py)?)? || value.is_instance(date_type(py)?)? {
                let iso: String = value.call_method0("isoformat")?.extract()?;
                YamlNode::Scalar(YamlScalar {
                    value: ScalarValue::Str(iso),
                    style: scalar_style,
                    original: None,
                    chomping: None,
                    meta: NodeMeta {
                        tag: Some(tag.unwrap_or("!!timestamp").to_owned()),
                        ..NodeMeta::default()
                    },
                })
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "YamlScalar value must be str, int, float, bool, None, bytes, bytearray, datetime, or date",
                ));
            }
        };
        Ok(PyYamlScalar {
            inner,
            explicit_start: false,
            explicit_end: false,
            yaml_version: None,
            tag_directives: vec![],
            parent: NodeParent::None,
        })
    }

    /// The Python value of this scalar.
    ///
    /// Applies built-in tag handling: ``!!binary`` → ``bytes``,
    /// ``!!timestamp`` → ``datetime.datetime`` / ``datetime.date``. All other
    /// tags yield the raw primitive (``int | float | bool | str | None``).
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            YamlNode::Scalar(s) => scalar_to_py_with_tag(py, s, None),
            _ => Ok(py.None()),
        }
    }

    fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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

    /// Strip cosmetic scalar formatting, resetting to clean YAML defaults.
    ///
    /// When *styles* is ``True`` (the default), scalar quoting is reset to
    /// plain (literal for multi-line strings) and ``original`` is cleared so
    /// non-canonical forms emit canonically. When *comments* is ``True``
    /// (the default), any inline or before-key comments attached to this
    /// scalar are cleared. Tags and anchors are preserved.
    #[pyo3(signature = (*, styles=true, comments=true))]
    fn format(&mut self, py: Python<'_>, styles: bool, comments: bool) {
        let opts = FormatOptions {
            styles,
            comments,
            blank_lines: false,
        };
        self.propagate(py, |node| {
            if let YamlNode::Scalar(s) = node {
                s.format_with(opts);
            }
        });
    }

    /// The inline comment on this scalar (appears after it on the same line),
    /// or ``None``. Assign ``None`` to clear.
    #[getter]
    fn get_comment_inline(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.meta.comment_inline.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_comment_inline(&mut self, py: Python<'_>, comment: Option<&str>) {
        let owned = comment.map(str::to_owned);
        self.propagate(py, |node| node.set_comment_inline(owned.clone()));
    }

    /// The block comment on the lines preceding this scalar, or ``None``.
    /// Assign ``None`` to clear.
    #[getter]
    fn get_comment_before(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.meta.comment_before.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_comment_before(&mut self, py: Python<'_>, comment: Option<&str>) {
        let owned = comment.map(str::to_owned);
        self.propagate(py, |node| node.set_comment_before(owned.clone()));
    }

    /// The scalar style: ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, or ``"folded"``.
    /// Newly created scalars use ``"plain"``.
    #[getter]
    fn style(&self) -> &'static str {
        match &self.inner {
            YamlNode::Scalar(s) => match s.style {
                ScalarStyle::Plain => "plain",
                ScalarStyle::SingleQuoted => "single",
                ScalarStyle::DoubleQuoted => "double",
                ScalarStyle::Literal => "literal",
                ScalarStyle::Folded => "folded",
            },
            _ => "plain",
        }
    }

    #[setter]
    fn set_style(&mut self, py: Python<'_>, style: &str) -> PyResult<()> {
        let new_style = parse_scalar_style(style)?;
        self.propagate(py, |node| {
            if let YamlNode::Scalar(s) = node {
                s.style = new_style;
            }
        });
        Ok(())
    }

    /// The YAML tag on this scalar (e.g. ``"!!str"``), or ``None``.
    #[getter]
    fn get_tag(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.meta.tag.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_tag(&mut self, py: Python<'_>, tag: Option<&str>) {
        let owned = tag.map(str::to_owned);
        self.propagate(py, |node| {
            if let YamlNode::Scalar(s) = node {
                s.meta.tag.clone_from(&owned);
            }
        });
    }

    /// The anchor name declared on this scalar (``&name``), or ``None``.
    #[getter]
    fn get_anchor(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.meta.anchor.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_anchor(&mut self, py: Python<'_>, anchor: Option<&str>) {
        let owned = anchor.map(str::to_owned);
        self.propagate(py, |node| {
            if let YamlNode::Scalar(s) = node {
                s.meta.anchor.clone_from(&owned);
            }
        });
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

    /// The number of blank lines emitted before this scalar (0–255).
    #[getter]
    fn get_blank_lines_before(&self) -> u8 {
        self.inner.blank_lines_before()
    }

    #[setter]
    fn set_blank_lines_before(&mut self, py: Python<'_>, n: u32) {
        let clamped = n.min(255) as u8;
        self.propagate(py, |node| node.set_blank_lines_before(clamped));
    }
}

impl PyYamlScalar {
    /// Apply *f* to both `self.inner` and the corresponding node slot in the
    /// parent container (if any). Keeps the local wrapper and the parent's
    /// `inner` in lock-step on every mutation.
    fn propagate<F>(&mut self, py: Python<'_>, mut f: F)
    where
        F: FnMut(&mut YamlNode),
    {
        f(&mut self.inner);
        self.parent.with_node_mut(py, f);
    }
}
