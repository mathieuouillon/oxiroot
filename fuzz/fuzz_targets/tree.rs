#![no_main]
//! Fuzz the TTree read path (tree object → branches → baskets → values):
//! arbitrary bytes must never panic.
use libfuzzer_sys::fuzz_target;
use oxiroot_io_core::FileReader;
use oxiroot_tree::TreeReader;

fuzz_target!(|data: &[u8]| {
    let Ok(f) = FileReader::from_bytes(data.to_vec()) else {
        return;
    };
    let names: Vec<String> = f.keys().iter().take(4).map(|k| k.name.clone()).collect();
    for name in &names {
        let Ok(t) = TreeReader::open(&f, name) else {
            continue;
        };
        let _ = t.num_entries();
        let branches: Vec<String> = t
            .branch_names()
            .into_iter()
            .take(8)
            .map(str::to_string)
            .collect();
        for b in &branches {
            let _ = t.read_branch(&f, b);
            let _ = t.read_branch_flat(&f, b);
            let _ = t.read_branch_range(&f, b, 1, 3);
        }
    }
});
