#![no_main]

use libfuzzer_sys::fuzz_target;
use yarutsk::core::parser::{Event, Parser};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let mut parser = Parser::new_from_str(s);
    for _ in 0..10_000 {
        match parser.next_token() {
            Ok((Event::StreamEnd, _)) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
});
