// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! `PyYamlNode` — abstract base class shared by `PyYamlMapping`,
//! `PyYamlSequence`, and `PyYamlScalar`.
//!
//! Existence makes `isinstance(x, yarutsk.YamlNode)` work and gives the
//! public type alias a real Python class. The base also owns the
//! document-level metadata (`explicit_start`/`explicit_end`/`yaml_version`/
//! `tag_directives`) that every concrete node carries — held as a single
//! `DocMetadata` field rather than four separate fields, so load/dump can
//! move the struct in/out without per-field copies.
//!
//! Constructing `YamlNode` directly raises `TypeError`; callers must
//! instantiate one of the concrete subclasses.

use pyo3::prelude::*;

use super::style_parse::parse_yaml_version;
use crate::core::types::DocMetadata;

#[pyclass(name = "YamlNode", subclass, skip_from_py_object)]
#[derive(Default, Clone)]
pub struct PyYamlNode {
    /// Document-level metadata for this node's document. Only the root's
    /// values are honoured at emit time; nested nodes carry whatever was
    /// set on construction (typically defaults).
    pub meta: DocMetadata,
}

#[pymethods]
impl PyYamlNode {
    #[new]
    fn new() -> PyResult<Self> {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "YamlNode is an abstract base class; instantiate YamlMapping, \
             YamlSequence, or YamlScalar instead",
        ))
    }

    /// Whether the source document had an explicit `---` marker.
    #[getter]
    fn get_explicit_start(&self) -> bool {
        self.meta.explicit_start
    }

    #[setter]
    fn set_explicit_start(&mut self, value: bool) {
        self.meta.explicit_start = value;
    }

    /// Whether the source document had an explicit `...` marker.
    #[getter]
    fn get_explicit_end(&self) -> bool {
        self.meta.explicit_end
    }

    #[setter]
    fn set_explicit_end(&mut self, value: bool) {
        self.meta.explicit_end = value;
    }

    /// The `%YAML` version directive for this document (e.g. `"1.2"`), or `None`.
    #[getter]
    fn get_yaml_version(&self) -> Option<String> {
        self.meta
            .yaml_version
            .map(|(maj, min)| format!("{maj}.{min}"))
    }

    #[setter]
    fn set_yaml_version(&mut self, version: Option<&str>) -> PyResult<()> {
        self.meta.yaml_version = parse_yaml_version(version)?;
        Ok(())
    }

    /// `%TAG handle prefix` pairs (empty if none).
    #[getter]
    fn get_tag_directives(&self) -> Vec<(String, String)> {
        self.meta.tag_directives.clone()
    }

    #[setter]
    fn set_tag_directives(&mut self, value: Vec<(String, String)>) {
        self.meta.tag_directives = value;
    }
}

impl PyYamlNode {
    /// Borrow the document-level metadata directly. Avoids the per-field
    /// clone that the previous `doc_metadata() -> DocMetadata` accessor did
    /// at every dump-path call site.
    pub(crate) fn doc_metadata(&self) -> &DocMetadata {
        &self.meta
    }
}
