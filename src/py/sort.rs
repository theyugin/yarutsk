// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Mapping-key sorting that preserves round-trip metadata.
//!
//! `sort_mapping` reorders a `YamlMapping`'s entries while leaving comments,
//! styles, and blank lines attached to the entries that move with them.
//! `descend_sort_keys` is the recursive variant: mappings inside sequences are
//! sorted (so deep `sort_keys=True` works) but sequence item order is never
//! disturbed — `sort_keys` is a *mapping-key* operation.
//!
//! Comparisons honour Python semantics by calling `__lt__`; if a `__lt__` raise
//! occurs mid-sort it is captured in `py_compare`'s `err` slot and surfaced
//! after the sort completes.

use pyo3::prelude::*;

use crate::core::types::{MapKey, YamlEntry, YamlMapping, YamlNode};

/// Compare two Python values using `a < b`.
///
/// Only the `Lt` rich-compare is dispatched; `!Less` is treated as `Greater` rather than
/// disambiguating Equal vs Greater with a second call. This is safe for stable-sort
/// call sites (Rust's `sort_by` only branches on `Less` vs `!Less`), and halves the
/// Python dispatch cost on the non-Less branch.
pub(crate) fn py_compare<'py>(
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
    err: &mut Option<PyErr>,
) -> std::cmp::Ordering {
    match a.lt(b) {
        Ok(true) => std::cmp::Ordering::Less,
        Ok(false) => std::cmp::Ordering::Greater,
        Err(e) => {
            *err = Some(e);
            std::cmp::Ordering::Equal
        }
    }
}

/// Walk a node tree applying `sort_mapping` to every mapping found.
///
/// Sequence items are visited (so mappings nested inside lists are sorted) but
/// the sequence itself is never reordered — `sort_keys` is a mapping-key
/// operation, not an item-order operation.
fn descend_sort_keys(
    py: Python<'_>,
    node: &mut YamlNode,
    key: Option<&Py<PyAny>>,
    reverse: bool,
) -> PyResult<()> {
    match node {
        YamlNode::Mapping(nested) => sort_mapping(py, nested, key, reverse, true),
        YamlNode::Sequence(seq) => {
            for item in &mut seq.items {
                descend_sort_keys(py, item, key, reverse)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn sort_mapping(
    py: Python<'_>,
    m: &mut YamlMapping,
    key: Option<&Py<PyAny>>,
    reverse: bool,
    recursive: bool,
) -> PyResult<()> {
    if recursive {
        for (_, entry) in &mut m.entries {
            descend_sort_keys(py, &mut entry.value, key, reverse)?;
        }
    }

    let mut entries: Vec<(MapKey, YamlEntry)> = m.entries.drain(..).collect();

    if let Some(key_fn) = key {
        let computed: Vec<Py<PyAny>> = entries
            .iter()
            .map(|(k, _)| {
                key_fn
                    .bind(py)
                    .call1((k.python_key(),))
                    .map(pyo3::Bound::unbind)
            })
            .collect::<PyResult<_>>()?;

        let mut zipped: Vec<(Py<PyAny>, (MapKey, YamlEntry))> =
            computed.into_iter().zip(entries).collect();

        let mut err: Option<PyErr> = None;
        zipped.sort_by(|(ka, _), (kb, _)| {
            if err.is_some() {
                return std::cmp::Ordering::Equal;
            }
            py_compare(ka.bind(py), kb.bind(py), &mut err)
        });
        if let Some(e) = err {
            return Err(e);
        }
        entries = zipped.into_iter().map(|(_, e)| e).collect();
    } else {
        entries.sort_by_key(|(k1, _)| k1.python_key());
    }

    if reverse {
        entries.reverse();
    }
    for (k, v) in entries {
        m.entries.insert(k, v);
    }
    Ok(())
}
