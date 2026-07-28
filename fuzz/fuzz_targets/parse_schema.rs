#![no_main]
//! The DSL parser must never panic on arbitrary input.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        let _ = marklens_core::parse_schema(src);
    }
});
