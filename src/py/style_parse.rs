// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Parsers for the small string-typed enums in the public Python API:
//! `ScalarStyle` (`"plain"`/`"single"`/`"double"`/`"literal"`/`"folded"`),
//! `ContainerStyle` (`"block"`/`"flow"`), and YAML version strings (`"1.2"`).

use pyo3::prelude::*;

use crate::core::types::{ContainerStyle, ScalarStyle};

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
