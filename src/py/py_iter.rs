// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use super::convert::{DocMeta, node_to_doc};
use super::schema::Schema;
use super::streaming::CharsSource;
use crate::core::builder::{Builder, TagPolicy};
use crate::core::parser::{Event, Parser};

// ─── Inner state ──────────────────────────────────────────────────────────────

pub(crate) struct YamlIterInner {
    pub(crate) parser: Parser<CharsSource>,
    pub(crate) builder: Builder,
    pub(crate) policy: Option<TagPolicy>,
    pub(crate) done: bool,
    /// IO error slot shared with `PyIoCharsIter`.  `None` for string sources.
    pub(crate) error_slot: Option<Arc<Mutex<Option<PyErr>>>>,
}

// ─── PyYamlIter ───────────────────────────────────────────────────────────────

#[pyclass(name = "YamlIter")]
pub(crate) struct PyYamlIter {
    inner: Option<YamlIterInner>,
    schema: Option<Py<Schema>>,
}

impl PyYamlIter {
    pub(crate) fn new(inner: YamlIterInner, schema: Option<Py<Schema>>) -> Self {
        PyYamlIter {
            inner: Some(inner),
            schema,
        }
    }
}

#[pymethods]
impl PyYamlIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let schema = slf.schema.as_ref().map(|s| s.bind(py).clone());

        let inner = match slf.inner.as_mut() {
            None => return Ok(None),
            Some(i) if i.done => return Ok(None),
            Some(i) => i,
        };

        let docs_before = inner.builder.doc_explicit_end.len();

        loop {
            let (ev, mark) = inner
                .parser
                .next_token()
                .map_err(|e| crate::ParseError::new_err(format!("YAML parse error: {e}")))?;

            // Check for any IO error that was stored during the token scan.
            if let Some(slot) = &inner.error_slot
                && let Ok(mut guard) = slot.lock()
                && let Some(err) = guard.take()
            {
                inner.done = true;
                return Err(err);
            }

            let comments = inner.parser.drain_comments();
            inner.builder.absorb_comments(comments);

            let is_end = matches!(ev, Event::StreamEnd);
            inner.builder.process_event(ev, mark, inner.policy.as_ref());

            if is_end {
                inner.done = true;
                break;
            }

            if inner.builder.doc_explicit_end.len() > docs_before {
                // A DocumentEnd event was processed — one full doc is ready.
                break;
            }
        }

        if inner.builder.docs.is_empty() {
            return Ok(None);
        }

        // Extract the first (oldest) document from all five parallel vecs.
        let doc_node = inner.builder.docs.remove(0);
        let explicit_start = if inner.builder.doc_explicit.is_empty() {
            false
        } else {
            inner.builder.doc_explicit.remove(0)
        };
        let explicit_end = if inner.builder.doc_explicit_end.is_empty() {
            false
        } else {
            inner.builder.doc_explicit_end.remove(0)
        };
        let yaml_version = if inner.builder.doc_yaml_version.is_empty() {
            None
        } else {
            inner.builder.doc_yaml_version.remove(0)
        };
        let tag_directives = if inner.builder.doc_tag_directives.is_empty() {
            vec![]
        } else {
            inner.builder.doc_tag_directives.remove(0)
        };

        let meta = DocMeta {
            explicit_start,
            explicit_end,
            yaml_version,
            tag_directives,
        };

        let py_doc = node_to_doc(py, doc_node, meta, schema.as_ref())?;
        Ok(Some(py_doc))
    }
}
