#![no_main]
//! Fuzz the TFile container parser, including the directory-navigation parse
//! paths: arbitrary bytes must never panic.
use libfuzzer_sys::fuzz_target;
use oxiroot_io_core::FileReader;

fuzz_target!(|data: &[u8]| {
    if let Ok(f) = FileReader::from_bytes(data.to_vec()) {
        let size = usize::try_from(f.size()).unwrap_or(usize::MAX);
        for k in f.keys() {
            let _ = f.key_payload(k);
            let _ = k.payload_start(size);
            // The generic, streamer-info-driven object reader walks attacker
            // controlled TStreamerInfo and object bytes.
            let _ = f.get_value(&k.name);
        }

        // Directory navigation: subdir() / object_in() resolve a record body
        // from an attacker-controlled fSeekKey — must reject, never panic or
        // wrap. Drive subdir() through every directory key, and object_in()
        // (which goes through subdir() too) through the directory names plus a
        // couple of arbitrary names.
        for k in f.keys() {
            if k.class_name == "TDirectory" || k.class_name == "TDirectoryFile" {
                let _ = f.subdir(&k.name);
                let _ = f.object_in(&k.name, "x");
            }
        }
        let names: Vec<String> = f.keys().iter().take(2).map(|k| k.name.clone()).collect();
        for name in &names {
            let _ = f.object_in(name, "x");
        }

        let _ = f.streamer_registry();
        let _ = f.streamer_info_object();
        let _ = f.free_segments();
    }
});
