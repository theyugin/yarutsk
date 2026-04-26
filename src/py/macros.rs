// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Declarative macros shared by `PyYamlMapping` and `PyYamlSequence`.
//!
//! After the dict/list-extension drop, both classes are standalone pyclasses
//! with mirror-image metadata accessors over `inner.meta` / `inner.style` /
//! `inner.trailing_blank_lines` and the document-level `yaml_version` field.
//! `container_metadata_pymethods!` emits all eight getter/setter pairs for
//! a given pyclass in one place so the two classes can't drift.

/// Emit a secondary `#[pymethods]` impl block carrying the eight metadata
/// getter/setter pairs shared by `PyYamlMapping` and `PyYamlSequence`.
///
/// Requires the `multiple-pymethods` `pyo3` feature (so this block coexists
/// with the per-class primary one) and `parse_container_style` (from
/// `super::style_parse`) to be in scope at the call site. The
/// `yaml_version`/`explicit_start`/`explicit_end`/`tag_directives` accessors
/// live on the abstract `PyYamlNode` base and are inherited by the subclass.
///
/// Usage:
/// ```ignore
/// container_metadata_pymethods!(PyYamlMapping);
/// container_metadata_pymethods!(PyYamlSequence);
/// ```
macro_rules! container_metadata_pymethods {
    ($type:ident) => {
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
        }
    };
}

pub(crate) use container_metadata_pymethods;
