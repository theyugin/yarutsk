#![no_main]

use libfuzzer_sys::fuzz_target;
use yarutsk::core::scanner::{Scanner, TokenType};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let mut scanner = Scanner::new(s.chars());
    // Drain up to a bounded number of tokens; stop on StreamEnd or error.
    for _ in 0..10_000 {
        match scanner.next_token() {
            Ok(Some(tok)) => {
                if matches!(tok.1, TokenType::StreamEnd) {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
});
