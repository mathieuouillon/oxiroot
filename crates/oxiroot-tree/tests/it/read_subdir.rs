//! Reading a `TTree` from a nested subdirectory (`TTree::open_in`).
use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn opens_a_tree_in_a_nested_subdirectory() {
    // fixtures/tree_subdir.root holds the tree at `cal/run2/Events` (ROOT-written).
    let f = RFile::open(fixture("tree_subdir.root")).expect("open");
    let t = TTree::open_in(&f, "cal/run2", "Events").expect("open nested tree");
    assert_eq!(t.num_entries(), 5);
    assert_eq!(
        t.read_branch(&f, "i").unwrap(),
        BranchValues::I32(vec![0, 1, 2, 3, 4])
    );
    assert_eq!(
        t.read_branch(&f, "x").unwrap(),
        BranchValues::F64(vec![0.0, 1.5, 3.0, 4.5, 6.0])
    );

    // A missing level / tree is a clean error, not a panic.
    assert!(TTree::open_in(&f, "cal/nope", "Events").is_err());
    assert!(TTree::open_in(&f, "cal/run2", "Missing").is_err());
}
