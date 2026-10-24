//! `.vxtr` load: any bytes give a valid document or an error, never a
//! panic; a loaded document saves and loads back equal.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vextor_core::{from_bytes, to_bytes};

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = from_bytes(data) else {
        return;
    };
    let bytes = to_bytes(&doc).expect("a loaded document saves");
    let again = from_bytes(&bytes).expect("a saved document loads");
    assert_eq!(doc, again);
});
