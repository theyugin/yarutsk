// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! `PyO3` adapter layer over the pure-Rust `core` pipeline.
//!
//! - `py_node` — `PyYamlNode`, the empty abstract base class the three
//!   concrete node pyclasses extend. Exists so `isinstance(x, YamlNode)` works.
//! - `py_mapping`, `py_sequence`, `py_scalar` — the three Python-visible
//!   pyclasses (`YamlMapping`/`YamlSequence`/`YamlScalar`). They are
//!   standalone pyclasses (not extending `dict`/`list`); the dict/list
//!   protocol on the first two is implemented manually via dunder methods.
//! - `convert` — the boundary between Python objects and `core::types::YamlNode`,
//!   plus anchor/alias state. Scalars accessed via `node()` are lazily
//!   promoted into `LiveNode::LivePy(Py<PyYamlScalar>)` so subsequent
//!   reads share identity and setters land directly on the borrowed pyclass.
//! - `schema` — per-call loader/dumper registry for custom tag handling.
//! - `streaming` — char-source adapters for `load_all*` over Python IO objects.
//! - `py_iter` — backing iterator for `iter_load_all*`.
//! - `sort`, `style_parse` — small focused helpers extracted from `convert`.
//! - `macros` — small declarative macros shared by the pyclasses.

pub(crate) mod convert;
pub(crate) mod live;
pub(crate) mod macros;
pub(crate) mod py_iter;
pub(crate) mod py_mapping;
pub(crate) mod py_node;
pub(crate) mod py_scalar;
pub(crate) mod py_sequence;
pub(crate) mod schema;
pub(crate) mod sort;
pub(crate) mod streaming;
pub(crate) mod style_parse;
