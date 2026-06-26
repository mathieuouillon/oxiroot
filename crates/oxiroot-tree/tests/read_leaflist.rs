//! Read a multi-leaf (leaflist) branch written by ROOT C++: one TBranch "s" with
//! leaves a/F, b/I, c/D packed at their fOffsets. Each leaf is exposed as an
//! "s.<leaf>" sub-branch sliced from the per-entry stride.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

#[test]
fn reads_leaflist_branch() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join("tree_leaflist.root");
    let f = RFile::open(path).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 4);
    assert_eq!(t.branch_names(), ["s.a", "s.b", "s.c"]);
    assert!(t.unsupported_branches().is_empty());

    let read = |n| t.read_branch(&f, n).expect("read branch");
    assert_eq!(read("s.a"), BranchValues::F32(vec![0.5, 1.5, 2.5, 3.5]));
    assert_eq!(read("s.b"), BranchValues::I32(vec![0, 10, 20, 30]));
    assert_eq!(read("s.c"), BranchValues::F64(vec![0.0, 1.25, 2.5, 3.75]));
}
