#![no_main]
//! Fuzz the TFile container parser: arbitrary bytes must never panic.
use libfuzzer_sys::fuzz_target;
use oxiroot_io_core::RFile;

fuzz_target!(|data: &[u8]| {
    if let Ok(f) = RFile::from_bytes(data.to_vec()) {
        let names: Vec<String> = f.keys().iter().map(|k| k.name.clone()).collect();
        for k in f.keys() {
            let _ = k.payload(f.data());
            let _ = k.payload_start(f.data().len());
        }
        // Directory navigation: subdir() / object_in() locate a key body via
        // an attacker-controlled fSeekKey — must reject, never panic.
        for n in &names {
            let _ = f.subdir(n);
            let _ = f.object_in(n, n);
        }
        let _ = f.streamer_registry();
        let _ = f.streamer_info_object();
        let _ = f.free_segments();
    }
});
