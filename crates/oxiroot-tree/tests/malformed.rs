//! Hardening: corrupt/truncated TTree files must yield `Err`, never panic.
//!
//! Covers a flat tree, fixed/variable arrays, a split `std::vector<MyStruct>`
//! tree, and a `std::vector<double>` tree. For every mutated file we open the
//! tree and read every branch; the test passes as long as nothing panics.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::TTree;

/// Fixtures spanning the supported branch layouts, with their tree name.
const FIXTURES: &[(&str, &str)] = &[
    ("tree_flat.root", "Events"),
    ("tree_arrays.root", "T"),
    ("tree_split.root", "T"),
    ("tree_vector.root", "T"),
];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("read fixture")
}

/// Open the tree (if it parses) and read every branch. The point is that none
/// of this panics regardless of the bytes.
fn poke_tree(f: &RFile, tree: &str) {
    if let Ok(t) = TTree::open(f, tree) {
        let names: Vec<String> = t.branch_names().iter().map(|s| s.to_string()).collect();
        for b in &names {
            let _ = t.read_branch(f, b);
        }
    }
}

/// Stride that keeps each fixture to roughly `samples` probes regardless of size.
fn stride(len: usize, samples: usize) -> usize {
    (len / samples).max(1)
}

#[test]
fn tree_byte_flips_never_panic() {
    for (fix, tree) in FIXTURES {
        let data = fixture(fix);
        let step = stride(data.len(), 3000);
        for i in (0..data.len()).step_by(step) {
            for v in [0x00u8, 0xff] {
                let mut c = data.clone();
                c[i] = v;
                if let Ok(f) = RFile::from_bytes(c) {
                    poke_tree(&f, tree);
                }
            }
        }
    }
}

#[test]
fn tree_truncations_never_panic() {
    for (fix, tree) in FIXTURES {
        let data = fixture(fix);
        let step = stride(data.len(), 2000);
        for len in (0..=data.len()).step_by(step) {
            if let Ok(f) = RFile::from_bytes(data[..len].to_vec()) {
                poke_tree(&f, tree);
            }
        }
    }
}
