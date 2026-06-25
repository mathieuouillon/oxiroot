//! Read a split (`fSplitLevel > 0`) `std::vector<MyStruct>` `TBranchElement`.
//!
//! ROOT splits `std::vector<Hit>` (Hit = {float x; float y; int id;}) into
//! per-member sub-branches `hits.x`/`hits.y`/`hits.id`, each a jagged array of
//! the member type. Our reader exposes those sub-branches.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_split_vector_of_struct() {
    let f = RFile::open(fixture("tree_split.root")).expect("open");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    // The split parent `hits` contributes its member sub-branches.
    assert_eq!(t.branch_names(), ["hits.x", "hits.y", "hits.id"]);

    assert_eq!(
        t.read_branch(&f, "hits.x").unwrap(),
        BranchValues::VecF32(vec![vec![0.0], vec![0.0, 1.0], vec![0.0, 1.0, 2.0]])
    );
    assert_eq!(
        t.read_branch(&f, "hits.y").unwrap(),
        BranchValues::VecF32(vec![vec![0.5], vec![0.5, 1.5], vec![0.5, 1.5, 2.5]])
    );
    assert_eq!(
        t.read_branch(&f, "hits.id").unwrap(),
        BranchValues::VecI32(vec![vec![0], vec![0, 10], vec![0, 10, 20]])
    );
}
