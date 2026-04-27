// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use pyo3::prelude::*;

use super::convert::{date_type, datetime_type, py_primitive_to_scalar, scalar_to_py_with_tag};
use super::py_node::PyYamlNode;
use super::style_parse::parse_scalar_style;
use crate::core::types::{FormatOptions, NodeMeta, ScalarStyle, ScalarValue, YamlNode, YamlScalar};

/// A YAML scalar document node (int, float, bool, str, or null), or an alias
/// resolving to a scalar.
///
/// Scalar children of a `PyYamlMapping`/`PyYamlSequence` are lazily promoted
/// into the parent's tree as `LiveNode::LivePy(Py<PyYamlScalar>)` on the
/// first `node()` access; alias-to-scalar slots are promoted the same way so
/// the alias name and meta survive the round-trip.
///
/// `inner` is constrained to `YamlNode::Scalar` or `YamlNode::Alias` (whose
/// `resolved` points at a scalar). Other `YamlNode` variants are not valid.
#[pyclass(name = "YamlScalar", extends = PyYamlNode, from_py_object)]
#[derive(Clone)]
pub struct PyYamlScalar {
    pub(crate) inner: YamlNode,
}

impl PyYamlScalar {
    /// Borrow the underlying `YamlScalar` (the resolved scalar for an alias).
    fn scalar(&self) -> Option<&YamlScalar> {
        match &self.inner {
            YamlNode::Scalar(s) => Some(s),
            YamlNode::Alias { resolved, .. } => match resolved.as_ref() {
                YamlNode::Scalar(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }
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
    fn new(
        value: &Bound<'_, PyAny>,
        style: &str,
        tag: Option<&str>,
    ) -> PyResult<(Self, PyYamlNode)> {
        let scalar_style = parse_scalar_style(style)?;
        let scalar = if let Some(mut s) = py_primitive_to_scalar(value) {
            s.style = scalar_style;
            s.meta.tag = tag.map(str::to_owned);
            s
        } else if (value.is_instance_of::<pyo3::types::PyBytes>()
            || value.is_instance_of::<pyo3::types::PyByteArray>())
            && let Ok(b) = value.extract::<Vec<u8>>()
        {
            use base64::{Engine, engine::general_purpose::STANDARD};
            YamlScalar {
                value: ScalarValue::Str(STANDARD.encode(&b)),
                source: None,
                style: scalar_style,
                chomping: None,
                meta: NodeMeta {
                    tag: Some(tag.unwrap_or("!!binary").to_owned()),
                    ..NodeMeta::default()
                },
            }
        } else {
            let py = value.py();
            if value.is_instance(datetime_type(py)?)? || value.is_instance(date_type(py)?)? {
                let iso: String = value.call_method0("isoformat")?.extract()?;
                YamlScalar {
                    value: ScalarValue::Str(iso),
                    source: None,
                    style: scalar_style,
                    chomping: None,
                    meta: NodeMeta {
                        tag: Some(tag.unwrap_or("!!timestamp").to_owned()),
                        ..NodeMeta::default()
                    },
                }
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "YamlScalar value must be str, int, float, bool, None, bytes, bytearray, datetime, or date",
                ));
            }
        };
        Ok((
            PyYamlScalar {
                inner: YamlNode::Scalar(scalar),
            },
            PyYamlNode::default(),
        ))
    }

    /// The Python value of this scalar.
    ///
    /// Applies built-in tag handling: ``!!binary`` → ``bytes``,
    /// ``!!timestamp`` → ``datetime.datetime`` / ``datetime.date``. All other
    /// tags yield the raw primitive (``int | float | bool | str | None``).
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.scalar() {
            Some(s) => scalar_to_py_with_tag(py, s, None),
            None => Ok(py.None()),
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
    /// scalar are cleared. ``blank_lines`` is accepted for signature parity
    /// with the container `format()` methods but has no effect on scalars.
    /// Tags and anchors are preserved.
    #[pyo3(signature = (*, styles=true, comments=true, blank_lines=true))]
    fn format(&mut self, styles: bool, comments: bool, blank_lines: bool) {
        let _ = blank_lines;
        let opts = FormatOptions {
            styles,
            comments,
            blank_lines: false,
        };
        self.inner.format_with(opts);
    }

    /// The inline comment on this scalar (appears after it on the same line),
    /// or ``None``. Assign ``None`` to clear.
    #[getter]
    fn get_comment_inline(&self) -> Option<&str> {
        self.inner.comment_inline()
    }

    #[setter]
    fn set_comment_inline(&mut self, comment: Option<&str>) {
        self.inner.set_comment_inline(comment.map(str::to_owned));
    }

    /// The block comment on the lines preceding this scalar, or ``None``.
    /// Assign ``None`` to clear.
    #[getter]
    fn get_comment_before(&self) -> Option<&str> {
        self.inner.comment_before()
    }

    #[setter]
    fn set_comment_before(&mut self, comment: Option<&str>) {
        self.inner.set_comment_before(comment.map(str::to_owned));
    }

    /// The scalar style: ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, or ``"folded"``.
    /// Newly created scalars use ``"plain"``.
    #[getter]
    fn style(&self) -> &'static str {
        let style = match self.scalar() {
            Some(s) => s.style,
            None => return "plain",
        };
        match style {
            ScalarStyle::Plain => "plain",
            ScalarStyle::SingleQuoted => "single",
            ScalarStyle::DoubleQuoted => "double",
            ScalarStyle::Literal => "literal",
            ScalarStyle::Folded => "folded",
        }
    }

    #[setter]
    fn set_style(&mut self, style: &str) -> PyResult<()> {
        let new_style = parse_scalar_style(style)?;
        if let YamlNode::Scalar(s) = &mut self.inner {
            s.style = new_style;
        }
        Ok(())
    }

    /// The YAML tag on this scalar (e.g. ``"!!str"``), or ``None``. Aliases
    /// can't carry their own tag.
    #[getter]
    fn get_tag(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.meta.tag.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_tag(&mut self, tag: Option<&str>) {
        if let YamlNode::Scalar(s) = &mut self.inner {
            s.meta.tag = tag.map(str::to_owned);
        }
    }

    /// The anchor name declared on this scalar (``&name``), or ``None``.
    /// Aliases can't carry their own anchor.
    #[getter]
    fn get_anchor(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.meta.anchor.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_anchor(&mut self, anchor: Option<&str>) {
        if let YamlNode::Scalar(s) = &mut self.inner {
            s.meta.anchor = anchor.map(str::to_owned);
        }
    }

    /// The number of blank lines emitted before this scalar (0–255).
    #[getter]
    fn get_blank_lines_before(&self) -> u8 {
        self.inner.blank_lines_before()
    }

    #[setter]
    fn set_blank_lines_before(&mut self, n: u32) {
        self.inner.set_blank_lines_before(n.min(255) as u8);
    }
}
