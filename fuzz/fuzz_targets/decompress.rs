#![no_main]
//! Fuzz the ROOT compression block decoder against arbitrary bytes and declared
//! uncompressed lengths — must never panic or allocate unboundedly.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for len in [data.len(), data.len().saturating_mul(4), 1 << 20] {
        let _ = oxiroot_compress::decompress(data, len);
    }
});
