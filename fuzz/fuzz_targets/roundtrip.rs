#![no_main]

//! Round-trip fuzz target: parse → build → emit → parse → compare doc count.
//!
//! Full structural equality is not asserted because style/whitespace details
//! are expected to change across an emit. Goal is to flush out panics or
//! asymmetric parse/emit behaviour.

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
    let out = emit_docs(&b1.docs, &b1.docs_meta, 2);
    let Some(b2) = parse(&out) else {
        return;
    };
    assert_eq!(b1.docs.len(), b2.docs.len(), "doc count drift on re-parse");
});
