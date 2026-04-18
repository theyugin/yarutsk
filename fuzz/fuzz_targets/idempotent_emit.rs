#![no_main]

//! Idempotent-emit fuzz target: parse → emit → parse → emit, assert the two
//! emit outputs are byte-identical.
//!
//! Parsing + emitting normalises cosmetic variance (styles, whitespace), so
//! from the first emit onward the serialisation must be a fixed point. Any
//! drift is an emitter bug that would cause repeated round-trips to mutate
//! files unnecessarily.

use libfuzzer_sys::fuzz_target;
use yarutsk::core::builder::Builder;
use yarutsk::core::emitter::emit_docs;
use yarutsk::core::parser::{Event, Parser};

fn parse(input: &str) -> Option<Builder> {
    let mut parser = Parser::new_from_str(input);
    let mut builder = Builder::new();
    for _ in 0..10_000 {
        let (ev, mark) = parser.next_token().ok()?;
        let is_end = matches!(ev, Event::StreamEnd);
        let comments = parser.drain_comments();
        builder.absorb_comments(comments);
        builder.process_event(ev, mark, None);
        if is_end {
            return Some(builder);
        }
    }
    None
}

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Some(b1) = parse(s) else {
        return;
    };
    let out1 = emit_docs(&b1.docs, &b1.docs_meta, 2);
    let Some(b2) = parse(&out1) else {
        return;
    };
    assert_eq!(b1.docs.len(), b2.docs.len(), "doc count drift on re-parse");
    let out2 = emit_docs(&b2.docs, &b2.docs_meta, 2);
    assert_eq!(out1, out2, "emit not idempotent after one reparse");
});
