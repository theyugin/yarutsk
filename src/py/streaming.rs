// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Reads from a Python IO object in 8 KB chunks, yielding `char` values one at
/// a time.  Supports both text (`str`) and binary (`bytes`) streams.
///
/// Uses a `String` + byte cursor so ASCII content costs 1 byte per char (vs.
/// 4 bytes in a `VecDeque<char>`). Also retains any trailing partial UTF-8
/// byte sequence across chunks so multi-byte characters that straddle the
/// 8 KB boundary decode correctly.
///
/// IO errors are stored in `error` so the caller can surface them after
/// parsing completes (since `Iterator::next` cannot return `Result`).
pub(crate) struct PyIoCharsIter {
    stream: Py<PyAny>,
    /// Decoded chunk; `cursor..buf.len()` is the unread tail.
    buf: String,
    cursor: usize,
    /// Undecoded trailing bytes left over from a chunk that ended mid-codepoint
    /// (binary-mode streams only). Prepended to the next chunk before UTF-8 decode.
    pending_bytes: Vec<u8>,
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
            buf: String::new(),
            cursor: 0,
            pending_bytes: Vec::new(),
            done: false,
            error,
        }
    }

    fn fill_buf(&mut self) {
        Python::attach(|py| match self.read_chunk(py) {
            Ok(Some(s)) => {
                self.buf = s;
                self.cursor = 0;
            }
            Ok(None) => self.done = true,
            Err(e) => self.set_error(e),
        });
    }

    /// Read one chunk from the Python stream.
    /// `Ok(Some(s))` = non-empty chunk, `Ok(None)` = EOF, `Err(_)` = fatal error.
    fn read_chunk(&mut self, py: Python<'_>) -> PyResult<Option<String>> {
        let chunk = self.stream.call_method1(py, "read", (8192_usize,))?;
        if chunk.is_none(py) {
            return Err(PyRuntimeError::new_err(
                "stream.read() must return str or bytes",
            ));
        }
        if let Ok(s) = chunk.extract::<String>(py) {
            return Ok((!s.is_empty()).then_some(s));
        }
        if let Ok(mut b) = chunk.extract::<Vec<u8>>(py) {
            // Prepend any partial codepoint bytes retained from the previous chunk.
            if !self.pending_bytes.is_empty() {
                let mut combined = std::mem::take(&mut self.pending_bytes);
                combined.append(&mut b);
                b = combined;
            }
            if b.is_empty() {
                return Ok(None);
            }
            match String::from_utf8(b) {
                Ok(s) => Ok(Some(s)),
                Err(e) => {
                    // The error may be a real decode failure or a chunk boundary
                    // that fell mid-codepoint. `utf8_error().valid_up_to()` tells
                    // us how far the prefix is valid; if the remaining bytes are
                    // a valid *partial* start of a codepoint (length 1–3), save
                    // them for the next chunk.
                    let utf8_err = e.utf8_error();
                    let valid_up_to = utf8_err.valid_up_to();
                    let mut bytes = e.into_bytes();
                    let trailing_len = bytes.len() - valid_up_to;
                    if trailing_len <= 3 && is_utf8_partial(&bytes[valid_up_to..]) {
                        self.pending_bytes = bytes.split_off(valid_up_to);
                        // SAFETY: bytes[..valid_up_to] is guaranteed valid UTF-8.
                        let s = unsafe { String::from_utf8_unchecked(bytes) };
                        return Ok((!s.is_empty()).then_some(s));
                    }
                    Err(PyRuntimeError::new_err(format!(
                        "UTF-8 decode error: {utf8_err}"
                    )))
                }
            }
        } else {
            Err(PyRuntimeError::new_err(
                "stream.read() must return str or bytes",
            ))
        }
    }

    fn set_error(&mut self, err: PyErr) {
        if let Ok(mut guard) = self.error.lock() {
            *guard = Some(err);
        }
        self.done = true;
    }
}

/// Returns true if *bytes* is a valid *prefix* of a multibyte UTF-8 codepoint
/// (1–3 bytes, consistent with the expected sequence length of the lead byte).
fn is_utf8_partial(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let lead = bytes[0];
    let expected = if lead & 0b1000_0000 == 0 {
        1 // ASCII — not a partial of anything
    } else if lead & 0b1110_0000 == 0b1100_0000 {
        2
    } else if lead & 0b1111_0000 == 0b1110_0000 {
        3
    } else if lead & 0b1111_1000 == 0b1111_0000 {
        4
    } else {
        return false; // invalid lead byte
    };
    if bytes.len() >= expected {
        return false; // would have decoded; not a partial
    }
    // Trailing continuation bytes must match 10xxxxxx.
    bytes[1..].iter().all(|b| b & 0b1100_0000 == 0b1000_0000)
}

impl Iterator for PyIoCharsIter {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        while self.cursor >= self.buf.len() && !self.done {
            self.fill_buf();
        }
        let c = self.buf[self.cursor..].chars().next()?;
        self.cursor += c.len_utf8();
        Some(c)
    }
}

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
            None => {
                if self.stream.call_method1(py, "write", (s,)).is_ok() {
                    self.text_mode = Some(true);
                } else {
                    self.stream.call_method1(py, "write", (s.as_bytes(),))?;
                    self.text_mode = Some(false);
                }
                return Ok(());
            }
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
