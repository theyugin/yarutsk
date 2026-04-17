// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use pyo3::prelude::*;

use super::convert::{
    parse_scalar_style, parse_yaml_version, py_primitive_to_scalar, scalar_to_py,
};
use crate::core::types::*;

// ─── PyYamlScalar (Python: YamlScalar) ───────────────────────────────────────

/// A YAML scalar document node (int, float, bool, str, or null).
#[pyclass(name = "YamlScalar", from_py_object)]
#[derive(Clone)]
pub struct PyYamlScalar {
    pub(crate) inner: YamlNode, // YamlNode::Scalar or YamlNode::Null
    /// True when the document this node belongs to had an explicit `---` marker.
    pub explicit_start: bool,
    /// True when the document this node belongs to had an explicit `...` marker.
    pub explicit_end: bool,
    /// `%YAML major.minor` directive for this document, if any.
    pub yaml_version: Option<(u8, u8)>,
    /// `%TAG handle prefix` pairs for this document.
    pub tag_directives: Vec<(String, String)>,
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
                    s.tag = tag.map(str::to_owned);
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
                tag: Some(tag.unwrap_or("!!binary").to_owned()),
                original: None,
                anchor: None,
            })
        } else {
            // datetime.datetime / datetime.date
            let py = value.py();
            let datetime_mod = py.import("datetime")?;
            let datetime_type = datetime_mod.getattr("datetime")?;
            let date_type = datetime_mod.getattr("date")?;
            if value.is_instance(&datetime_type)? || value.is_instance(&date_type)? {
                let iso: String = value.call_method0("isoformat")?.extract()?;
                YamlNode::Scalar(YamlScalar {
                    value: ScalarValue::Str(iso),
                    style: scalar_style,
                    tag: Some(tag.unwrap_or("!!timestamp").to_owned()),
                    original: None,
                    anchor: None,
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
        })
    }

    /// The Python primitive value of this scalar.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            YamlNode::Scalar(s) => scalar_to_py(py, &s.value),
            _ => Ok(py.None()),
        }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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

    /// Strip cosmetic formatting, resetting to clean YAML defaults.
    /// Pass keyword flags (all ``True`` by default) to control which fields are reset:
    ///
    /// - ``styles``: scalar quoting → plain (literal for multi-line strings),
    ///   ``original`` cleared so non-canonical forms emit canonically.
    /// - ``comments``: no-op on scalars (kept for API consistency).
    /// - ``blank_lines``: no-op on scalars (kept for API consistency).
    ///
    /// Tags and anchors are always preserved.
    #[pyo3(signature = (*, styles=true, comments=true, blank_lines=true))]
    fn format(&mut self, styles: bool, comments: bool, blank_lines: bool) {
        let _ = (comments, blank_lines); // no-op on scalars, accepted for API consistency
        if styles && let YamlNode::Scalar(s) = &mut self.inner {
            s.format_with(FormatOptions {
                styles: true,
                comments: false,
                blank_lines: false,
            });
        }
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
    fn set_style(&mut self, style: &str) -> PyResult<()> {
        let new_style = parse_scalar_style(style)?;
        if let YamlNode::Scalar(s) = &mut self.inner {
            s.style = new_style;
        }
        Ok(())
    }

    /// The YAML tag on this scalar (e.g. ``"!!str"``), or ``None``.
    #[getter]
    fn get_tag(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.tag.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_tag(&mut self, tag: Option<&str>) {
        if let YamlNode::Scalar(s) = &mut self.inner {
            s.tag = tag.map(str::to_owned);
        }
    }

    /// The anchor name declared on this scalar (``&name``), or ``None``.
    #[getter]
    fn get_anchor(&self) -> Option<&str> {
        match &self.inner {
            YamlNode::Scalar(s) => s.anchor.as_deref(),
            _ => None,
        }
    }

    #[setter]
    fn set_anchor(&mut self, anchor: Option<&str>) {
        if let YamlNode::Scalar(s) = &mut self.inner {
            s.anchor = anchor.map(str::to_owned);
        }
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
}
