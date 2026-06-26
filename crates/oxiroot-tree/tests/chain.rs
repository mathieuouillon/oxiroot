//! Multi-branch read and TChain (B12).

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TChain, TTree};

fn open() -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join("tree_flat.root"),
    )
    .expect("open")
}

#[test]
fn reads_several_branches_at_once() {
    let f = open();
    let t = TTree::open(&f, "Events").expect("open tree");
    let cols = t.read_branches(&f, &["i4", "b1"]).expect("read");
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0], BranchValues::I32(vec![0, 1, 2, 3, 4]));
    assert_eq!(
        cols[1],
        BranchValues::Bool(vec![true, false, true, false, true])
    );
}

#[test]
fn chain_concatenates_across_files() {
    // Chain the same file twice — a stand-in for a dataset split across files.
    let f1 = open();
    let f2 = open();
    let mut chain = TChain::new();
    chain.add(&f1, "Events").expect("add 1");
    chain.add(&f2, "Events").expect("add 2");

    assert_eq!(chain.num_trees(), 2);
    assert_eq!(chain.num_entries(), 10);
    assert_eq!(chain.branch_names(), ["i4", "i8", "f4", "f8", "b1", "u4"]);

    // The branch spans both trees, in add order.
    assert_eq!(
        chain.read_branch("i4").expect("i4"),
        BranchValues::I32(vec![0, 1, 2, 3, 4, 0, 1, 2, 3, 4])
    );
    assert_eq!(
        chain.read_branch("u4").expect("u4"),
        BranchValues::U32(vec![
            100,
            200,
            300,
            400,
            4_000_000_000,
            100,
            200,
            300,
            400,
            4_000_000_000
        ])
    );
}
