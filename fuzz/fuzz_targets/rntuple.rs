#![no_main]
//! Fuzz the RNTuple read path (anchor → envelopes → page decode → fields).
use libfuzzer_sys::fuzz_target;
use oxiroot_io_core::RFile;
use oxiroot_rntuple::RNTuple;

fuzz_target!(|data: &[u8]| {
    if let Ok(f) = RFile::from_bytes(data.to_vec()) {
        if let Ok(ntpl) = RNTuple::open(&f, "ntpl") {
            let names: Vec<String> = ntpl.field_names().iter().map(|s| s.to_string()).collect();
            for n in &names {
                let _ = ntpl.read_field(&f, n);
            }
        }
    }
});
