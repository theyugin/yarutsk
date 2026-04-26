// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Pure-Rust YAML pipeline. Data flows in one direction:
//!
//! `bytes/str → scanner → parser → builder → YamlNode → emitter → bytes/str`
//!
//! - `scanner` and `parser` are vendored from yaml-rust2 (the only modification
//!   is that `scanner` emits `Comment` tokens instead of discarding them).
//! - `builder` consumes parser events and produces `YamlNode` trees, attaching
//!   comments/blank-lines, resolving aliases, and applying `TagPolicy`.
//! - `types` is the round-trip data model (`YamlNode`, `YamlMapping` (IndexMap),
//!   `YamlSequence`, `YamlScalar` with `ScalarStyle`/`ScalarRepr`).
//! - `emitter` is a hand-written block-style serialiser that preserves styles,
//!   comments, blank lines, tags, and anchors.
//! - `char_traits` and `debug` are vendored helpers.

pub mod builder;
pub mod char_traits;
pub mod debug;
pub mod emitter;
pub mod parser;
pub mod scanner;
pub mod types;
