// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

// ─── PyIoCharsIter ────────────────────────────────────────────────────────────

/// Reads from a Python IO object in 8 KB chunks, yielding `char` values one at
/// a time.  Supports both text (`str`) and binary (`bytes`) streams.
///
/// IO errors are stored in `error` so the caller can surface them after
/// parsing completes (since `Iterator::next` cannot return `Result`).
pub(crate) struct PyIoCharsIter {
    stream: Py<PyAny>,
    buf: VecDeque<char>,
    done: bool,
    /// Shared slot: on error, `fill_buf` stores the `PyErr` here and sets
    /// `done = true`.  The slot is checked by `parse_stream` after
    /// `parse_iter` returns.
    pub(crate) error: Arc<Mutex<Option<PyErr>>>,
}

impl PyIoCharsIter {
    pub(crate) fn new(stream: Py<PyAny>, error: Arc<Mutex<Option<PyErr>>>) -> Self {
        PyIoCharsIter {
            stream,
            buf: VecDeque::new(),
            done: false,
            error,
        }
    }

    fn fill_buf(&mut self) {
        Python::attach(|py| {
            let chunk = self.stream.call_method1(py, "read", (8192_usize,));
            match chunk {
                Err(e) => {
                    if let Ok(mut guard) = self.error.lock() {
                        *guard = Some(e);
                    }
                    self.done = true;
                }
                Ok(c) => {
                    if c.is_none(py) {
                        // None is not a valid stream result
                        if let Ok(mut guard) = self.error.lock() {
                            *guard = Some(PyRuntimeError::new_err(
                                "stream.read() must return str or bytes",
                            ));
                        }
                        self.done = true;
                        return;
                    }
                    if let Ok(s) = c.extract::<String>(py) {
                        if s.is_empty() {
                            self.done = true;
                        } else {
                            self.buf.extend(s.chars());
                        }
                    } else if let Ok(b) = c.extract::<Vec<u8>>(py) {
                        if b.is_empty() {
                            self.done = true;
                        } else {
                            match String::from_utf8(b) {
                                Ok(s) => self.buf.extend(s.chars()),
                                Err(e) => {
                                    if let Ok(mut guard) = self.error.lock() {
                                        *guard = Some(PyRuntimeError::new_err(format!(
                                            "UTF-8 decode error: {e}"
                                        )));
                                    }
                                    self.done = true;
                                }
                            }
                        }
                    } else {
                        // e.g. read() returned an int or some other type
                        if let Ok(mut guard) = self.error.lock() {
                            *guard = Some(PyRuntimeError::new_err(
                                "stream.read() must return str or bytes",
                            ));
                        }
                        self.done = true;
                    }
                }
            }
        });
    }
}

impl Iterator for PyIoCharsIter {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        while self.buf.is_empty() && !self.done {
            self.fill_buf();
        }
        self.buf.pop_front()
    }
}

// ─── StringCharsIter ─────────────────────────────────────────────────────────

/// Owns a `String` and iterates its chars.  Used for `iter_loads_all` where
/// the text is already in memory but we need a concrete `Iterator<Item=char>`.
pub(crate) struct StringCharsIter {
    s: String,
    pos: usize,
}

impl StringCharsIter {
    pub(crate) fn new(s: String) -> Self {
        StringCharsIter { s, pos: 0 }
    }
}

impl Iterator for StringCharsIter {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        let c = self.s[self.pos..].chars().next()?;
        self.pos += c.len_utf8();
        Some(c)
    }
}

// ─── CharsSource ─────────────────────────────────────────────────────────────

/// Type-erased character source for `Parser<CharsSource>`.
/// The enum is `Send` because `Py<PyAny>: Send` and `Arc<Mutex<>>: Send`.
pub(crate) enum CharsSource {
    PyIo(PyIoCharsIter),
    Str(StringCharsIter),
}

impl Iterator for CharsSource {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        match self {
            CharsSource::PyIo(i) => i.next(),
            CharsSource::Str(i) => i.next(),
        }
    }
}

// ─── PyStreamWriter ──────────────────────────────────────────────────────────

/// A `fmt::Write` sink that writes directly to a Python IO stream.
///
/// Write errors (e.g. `stream.write()` raising a Python exception) are stored
/// in `error` for the caller to inspect after emission completes; the
/// `fmt::Error` value returned from `write_str` is otherwise opaque.
pub(crate) struct PyStreamWriter {
    /// Owned reference to the Python stream (text or binary).
    stream: Py<PyAny>,
    /// True once the first successful write determines the stream mode.
    /// `None` = not yet tried; `Some(true)` = text; `Some(false)` = binary.
    text_mode: Option<bool>,
    /// The first Python exception raised by `stream.write()`, if any.
    pub(crate) error: Option<PyErr>,
}

impl PyStreamWriter {
    pub(crate) fn new(stream: Py<PyAny>) -> Self {
        PyStreamWriter {
            stream,
            text_mode: None,
            error: None,
        }
    }

    /// Return any stored write error, clearing the slot.
    pub(crate) fn take_error(&mut self) -> Option<PyErr> {
        self.error.take()
    }
}

impl std::fmt::Write for PyStreamWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.error.is_some() {
            // Stop writing once an error has occurred.
            return Err(std::fmt::Error);
        }
        Python::attach(|py| {
            let ok = if self.text_mode == Some(false) {
                // Known binary stream.
                match self.stream.call_method1(py, "write", (s.as_bytes(),)) {
                    Ok(_) => true,
                    Err(e) => {
                        self.error = Some(e);
                        false
                    }
                }
            } else {
                // Try text first.
                match self.stream.call_method1(py, "write", (s,)) {
                    Ok(_) => {
                        self.text_mode = Some(true);
                        true
                    }
                    Err(e) => {
                        if self.text_mode.is_none() {
                            // Fall back to bytes.
                            match self.stream.call_method1(py, "write", (s.as_bytes(),)) {
                                Ok(_) => {
                                    self.text_mode = Some(false);
                                    true
                                }
                                Err(e2) => {
                                    self.error = Some(e2);
                                    false
                                }
                            }
                        } else {
                            self.error = Some(e);
                            false
                        }
                    }
                }
            };
            if ok { Ok(()) } else { Err(std::fmt::Error) }
        })
    }
}
