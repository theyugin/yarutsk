// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Declarative macros shared by `PyYamlMapping` and `PyYamlSequence`.
//!
//! After the dict/list-extension drop, both classes are standalone pyclasses
//! with mirror-image metadata accessors over `inner.meta` / `inner.style` /
//! `inner.trailing_blank_lines` and the document-level `yaml_version` field.
//! `container_metadata_pymethods!` emits all eight metadata getter/setter
//! pairs **plus** the byte-identical `__copy__` / `copy` implementations
//! for a given pyclass in one place so the two classes can't drift.

/// Emit a secondary `#[pymethods]` impl block carrying the eight metadata
/// getter/setter pairs and the `__copy__` / `copy` implementations shared
/// by `PyYamlMapping` and `PyYamlSequence`. (`__eq__` / `__ne__` cannot
/// live here: `PyO3`'s `multiple-pymethods` requires every richcompare
/// operator in a single impl block, and the per-class blocks already
/// carry sequence-only ordering operators.)
///
/// Requires the `multiple-pymethods` `pyo3` feature (so this block coexists
/// with the per-class primary one) and `parse_container_style` (from
/// `super::style_parse`) to be in scope at the call site. The
/// `yaml_version`/`explicit_start`/`explicit_end`/`tag_directives` accessors
/// live on the abstract `PyYamlNode` base and are inherited by the subclass.
///
/// `$to_py_obj` is the free function that wraps an owned
/// `YamlMapping<LiveNode>` / `YamlSequence<LiveNode>` plus a `DocMetadata`
/// into a fresh `Py<PyAny>` (i.e. `live_mapping_to_py_obj` /
/// `live_sequence_to_py_obj`); `__copy__` calls it with `slf.inner.clone()`.
///
/// Usage:
/// ```ignore
/// container_metadata_pymethods!(PyYamlMapping, live_mapping_to_py_obj);
/// container_metadata_pymethods!(PyYamlSequence, live_sequence_to_py_obj);
/// ```
macro_rules! container_metadata_pymethods {
    ($type:ident, $to_py_obj:path) => {
        #[pyo3::prelude::pymethods]
        impl $type {
            /// The YAML tag on this node (e.g. ``"!!map"`` / ``"!!seq"``), or ``None``.
            #[getter]
            fn get_tag(&self) -> Option<&str> {
                self.inner.meta.tag.as_deref()
            }

            #[setter]
            fn set_tag(&mut self, tag: Option<&str>) {
                self.inner.meta.tag = tag.map(str::to_owned);
            }

            /// The anchor name declared on this node (``&name``), or ``None``.
            #[getter]
            fn get_anchor(&self) -> Option<&str> {
                self.inner.meta.anchor.as_deref()
            }

            #[setter]
            fn set_anchor(&mut self, anchor: Option<&str>) {
                self.inner.meta.anchor = anchor.map(str::to_owned);
            }

            /// The container style: ``"block"`` or ``"flow"``.
            #[getter]
            fn get_style(&self) -> &str {
                match self.inner.style {
                    crate::core::types::ContainerStyle::Block => "block",
                    crate::core::types::ContainerStyle::Flow => "flow",
                }
            }

            #[setter]
            fn set_style(&mut self, style: &str) -> pyo3::PyResult<()> {
                self.inner.style = parse_container_style(style)?;
                Ok(())
            }

            /// The number of blank lines emitted after all entries/items in this container.
            #[getter]
            fn get_trailing_blank_lines(&self) -> u8 {
                self.inner.trailing_blank_lines
            }

            #[setter]
            fn set_trailing_blank_lines(&mut self, n: u8) {
                self.inner.trailing_blank_lines = n;
            }

            /// The number of blank lines emitted before this node (0–255).
            #[getter]
            fn get_blank_lines_before(&self) -> u8 {
                self.inner.meta.blank_lines_before
            }

            #[setter]
            #[allow(clippy::cast_possible_truncation)] // n.min(255) bounds it
            fn set_blank_lines_before(&mut self, n: u32) {
                self.inner.meta.blank_lines_before = n.min(255) as u8;
            }

            /// The inline (trailing) comment on this node, or ``None``.
            #[getter]
            fn get_comment_inline(&self) -> Option<&str> {
                self.inner.meta.comment_inline.as_deref()
            }

            #[setter]
            fn set_comment_inline(&mut self, comment: Option<String>) {
                self.inner.meta.comment_inline = comment;
            }

            /// The block comment preceding this node, or ``None``.
            #[getter]
            fn get_comment_before(&self) -> Option<&str> {
                self.inner.meta.comment_before.as_deref()
            }

            #[setter]
            fn set_comment_before(&mut self, comment: Option<String>) {
                self.inner.meta.comment_before = comment;
            }

            /// Shallow copy: `LivePy(Py<…>)` slots clone via `Py` refcount, so
            /// child containers are shared with the source — matches
            /// `dict.copy()` / `list.copy()` semantics.
            #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 receivers are by-value
            fn copy(
                slf: pyo3::PyRef<'_, Self>,
                py: pyo3::Python<'_>,
            ) -> pyo3::PyResult<pyo3::Py<pyo3::types::PyAny>> {
                Self::__copy__(slf, py)
            }

            #[allow(clippy::needless_pass_by_value)] // pymethod: PyO3 receivers are by-value
            fn __copy__(
                slf: pyo3::PyRef<'_, Self>,
                py: pyo3::Python<'_>,
            ) -> pyo3::PyResult<pyo3::Py<pyo3::types::PyAny>> {
                let meta = slf.as_super().doc_metadata().clone();
                $to_py_obj(py, slf.inner.clone(), meta)
            }
        }
    };
}

pub(crate) use container_metadata_pymethods;
