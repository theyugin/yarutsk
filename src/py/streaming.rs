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
        Python::attach(|py| match self.read_chunk(py) {
            Ok(Some(s)) => self.buf.extend(s.chars()),
            Ok(None) => self.done = true,
            Err(e) => self.set_error(e),
        });
    }

    /// Read one chunk from the Python stream.
    /// `Ok(Some(s))` = non-empty chunk, `Ok(None)` = EOF, `Err(_)` = fatal error.
    fn read_chunk(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let chunk = self.stream.call_method1(py, "read", (8192_usize,))?;
        if chunk.is_none(py) {
            return Err(PyRuntimeError::new_err(
                "stream.read() must return str or bytes",
            ));
        }
        if let Ok(s) = chunk.extract::<String>(py) {
            return Ok((!s.is_empty()).then_some(s));
        }
        if let Ok(b) = chunk.extract::<Vec<u8>>(py) {
            if b.is_empty() {
                return Ok(None);
            }
            return String::from_utf8(b)
                .map(Some)
                .map_err(|e| PyRuntimeError::new_err(format!("UTF-8 decode error: {e}")));
        }
        Err(PyRuntimeError::new_err(
            "stream.read() must return str or bytes",
        ))
    }

    fn set_error(&mut self, err: PyErr) {
        if let Ok(mut guard) = self.error.lock() {
            *guard = Some(err);
        }
        self.done = true;
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

    /// Write one chunk, tracking/latching text-vs-bytes mode on the first call.
    /// On the first call both forms are attempted; later calls reuse the latched mode.
    fn try_write(&mut self, py: Python<'_>, s: &str) -> PyResult<()> {
        match self.text_mode {
            Some(true) => self.stream.call_method1(py, "write", (s,))?,
            Some(false) => self.stream.call_method1(py, "write", (s.as_bytes(),))?,
            None => match self.stream.call_method1(py, "write", (s,)) {
                Ok(_) => {
                    self.text_mode = Some(true);
                    return Ok(());
                }
                Err(_) => {
                    self.stream.call_method1(py, "write", (s.as_bytes(),))?;
                    self.text_mode = Some(false);
                    return Ok(());
                }
            },
        };
        Ok(())
    }
}

impl std::fmt::Write for PyStreamWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.error.is_some() {
            // Stop writing once an error has occurred.
            return Err(std::fmt::Error);
        }
        Python::attach(|py| match self.try_write(py, s) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.error = Some(e);
                Err(std::fmt::Error)
            }
        })
    }
}
