//! Entry-range reads (B5): read_branch_range fetches only the baskets covering
//! the window. Uses the two-basket fixture so basket selection is exercised.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn open(name: &str) -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open")
}

#[test]
fn reads_entry_ranges() {
    let f = open("tree_multibasket.root");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 5);

    // Full range equals read_branch.
    let full = t.read_branch(&f, "i4").expect("full");
    assert_eq!(
        t.read_branch_range(&f, "i4", 0, 5).expect("range full"),
        full
    );
    assert_eq!(full, BranchValues::I32(vec![0, 1, 2, 3, 4]));

    // A sub-window (may span the basket boundary).
    assert_eq!(
        t.read_branch_range(&f, "i4", 1, 4).expect("mid"),
        BranchValues::I32(vec![1, 2, 3])
    );

    // stop clamps to the entry count; start clamps to stop.
    assert_eq!(
        t.read_branch_range(&f, "i4", 3, 100).expect("tail"),
        BranchValues::I32(vec![3, 4])
    );
    assert_eq!(
        t.read_branch_range(&f, "i4", 2, 2).expect("empty"),
        BranchValues::I32(vec![])
    );
    assert_eq!(
        t.read_branch_range(&f, "i4", 10, 20).expect("past end"),
        BranchValues::I32(vec![])
    );
}
