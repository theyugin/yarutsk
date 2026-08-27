// Copyright (c) yarutsk authors. Licensed under MIT — see LICENSE.

//! Char-source adapters and the `PyStreamWriter` sink.
//!
//! The scanner consumes `Iterator<Item = char>`, but Python callers pass IO
//! objects or `str`s. `PyIoCharsIter` reads any text-or-binary Python stream in
//! 8 KB chunks (preserving partial UTF-8 sequences across chunk boundaries) and
//! `StringCharsIter` adapts an in-memory `String`; both implement the common
//! `CharsSource` trait so `iter_load_all*` can stream input lazily.
//!
//! `PyStreamWriter` is the symmetric output side — a `fmt::Write` that forwards
//! into a Python `write()` callable for `dump_to`.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyRuntimeError, PyUnicodeDecodeError};
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
    /// Once a non-empty chunk is observed, subsequent chunks must use the
    /// same text/binary representation.
    text_mode: Option<bool>,
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
            text_mode: None,
            done: false,
            error,
        }
    }

    fn fill_buf(&mut self) {
        Python::attach(|py| {
            // A chunk may contain only the leading bytes of one UTF-8
            // codepoint. Keep reading until there is decoded data, true EOF,
            // or an error; an empty decoded prefix is not EOF.
            loop {
                match self.read_chunk(py) {
                    Ok(ReadChunk::Data(s)) => {
                        self.buf = s;
                        self.cursor = 0;
                        break;
                    }
                    Ok(ReadChunk::NeedMore) => {}
                    Ok(ReadChunk::Eof) => {
                        self.done = true;
                        break;
                    }
                    Err(e) => {
                        self.set_error(e);
                        break;
                    }
                }
            }
        });
    }

    /// Read one chunk from the Python stream.
    fn read_chunk(&mut self, py: Python<'_>) -> PyResult<ReadChunk> {
        let chunk = self.stream.call_method1(py, "read", (8192_usize,))?;
        if chunk.is_none(py) {
            return Err(PyRuntimeError::new_err(
                "stream.read() must return str or bytes",
            ));
        }
        if let Ok(s) = chunk.extract::<String>(py) {
            if s.is_empty() {
                if !self.pending_bytes.is_empty() {
                    return Err(incomplete_utf8_error(py, &self.pending_bytes));
                }
                return Ok(ReadChunk::Eof);
            }
            if self.text_mode == Some(false) || !self.pending_bytes.is_empty() {
                return Err(PyRuntimeError::new_err(
                    "stream.read() must not switch between bytes and str",
                ));
            }
            self.text_mode = Some(true);
            return Ok(ReadChunk::Data(s));
        }
        if let Ok(mut b) = chunk.extract::<Vec<u8>>(py) {
            if b.is_empty() {
                if !self.pending_bytes.is_empty() {
                    return Err(incomplete_utf8_error(py, &self.pending_bytes));
                }
                return Ok(ReadChunk::Eof);
            }
            if self.text_mode == Some(true) {
                return Err(PyRuntimeError::new_err(
                    "stream.read() must not switch between str and bytes",
                ));
            }
            self.text_mode = Some(false);
            // Prepend any partial codepoint bytes retained from the previous chunk.
            if !self.pending_bytes.is_empty() {
                let mut combined = std::mem::take(&mut self.pending_bytes);
                combined.append(&mut b);
                b = combined;
            }
            match String::from_utf8(b) {
                Ok(s) => Ok(ReadChunk::Data(s)),
                Err(e) => {
                    // The error may be a real decode failure or a chunk boundary
                    // that fell mid-codepoint. `utf8_error().valid_up_to()` tells
                    // us how far the prefix is valid; if the remaining bytes are
                    // a valid *partial* start of a codepoint (length 1–3), save
                    // them for the next chunk.
                    let utf8_err = e.utf8_error();
                    let invalid_bytes = e.as_bytes().to_vec();
                    let valid_up_to = utf8_err.valid_up_to();
                    let mut bytes = e.into_bytes();
                    let trailing_len = bytes.len() - valid_up_to;
                    if trailing_len <= 3 && is_utf8_partial(&bytes[valid_up_to..]) {
                        self.pending_bytes = bytes.split_off(valid_up_to);
                        // SAFETY: bytes[..valid_up_to] is guaranteed valid UTF-8.
                        let s = unsafe { String::from_utf8_unchecked(bytes) };
                        return Ok(if s.is_empty() {
                            ReadChunk::NeedMore
                        } else {
                            ReadChunk::Data(s)
                        });
                    }
                    Err(unicode_decode_error(py, &invalid_bytes, utf8_err))
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

enum ReadChunk {
    Data(String),
    NeedMore,
    Eof,
}

fn incomplete_utf8_error(py: Python<'_>, bytes: &[u8]) -> PyErr {
    let err = std::str::from_utf8(bytes).expect_err("pending bytes are incomplete UTF-8");
    unicode_decode_error(py, bytes, err)
}

fn unicode_decode_error(py: Python<'_>, bytes: &[u8], err: std::str::Utf8Error) -> PyErr {
    match PyUnicodeDecodeError::new_utf8(py, bytes, err) {
        Ok(value) => PyErr::from_value(value.into_any()),
        Err(pyerr) => pyerr,
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
    /// Coalesce the emitter's many tiny `fmt::Write` calls before crossing
    /// into Python.
    buffer: String,
}

impl PyStreamWriter {
    pub(crate) fn new(stream: Py<PyAny>) -> Self {
        PyStreamWriter {
            stream,
            text_mode: None,
            error: None,
            buffer: String::with_capacity(16 * 1024),
        }
    }

    pub(crate) fn finish(&mut self) -> PyResult<()> {
        if let Some(err) = self.error.take() {
            return Err(err);
        }
        self.flush_buffer()
    }

    fn flush_buffer(&mut self) -> PyResult<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.buffer);
        let result = Python::attach(|py| self.try_write(py, &pending));
        self.buffer = String::with_capacity(16 * 1024);
        result
    }

    /// Write one chunk, tracking/latching text-vs-bytes mode on the first call.
    /// On the first call both forms are attempted; later calls reuse the latched mode.
    fn try_write(&mut self, py: Python<'_>, s: &str) -> PyResult<()> {
        match self.text_mode {
            Some(true) => self.stream.call_method1(py, "write", (s,))?,
            Some(false) => self.stream.call_method1(py, "write", (s.as_bytes(),))?,
            None => {
                match self.stream.call_method1(py, "write", (s,)) {
                    Ok(_) => self.text_mode = Some(true),
                    Err(err) if err.is_instance_of::<pyo3::exceptions::PyTypeError>(py) => {
                        self.stream.call_method1(py, "write", (s.as_bytes(),))?;
                        self.text_mode = Some(false);
                    }
                    Err(err) => return Err(err),
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
        self.buffer.push_str(s);
        if self.buffer.len() < 16 * 1024 {
            return Ok(());
        }
        match self.flush_buffer() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.error = Some(e);
                Err(std::fmt::Error)
            }
        }
    }
}
