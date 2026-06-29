//! A split `std::vector<Outer>` where `Outer { int id; Inner inner; float w }`
//! nests another struct `Inner { float a; int b }`. ROOT flattens the nested
//! member into per-member jagged sub-branches (`v.id`, `v.inner.a`, `v.inner.b`,
//! `v.w`), which oxiroot reads. Cross-checked against ROOT C++ and uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open fixture")
}

#[test]
fn reads_nested_struct_subbranches() {
    let f = fixture("tree_nested.root");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    // The nested `Inner` member is flattened to `v.inner.a` / `v.inner.b`.
    assert_eq!(
        t.branch_names(),
        vec!["v.id", "v.inner.a", "v.inner.b", "v.w"]
    );
    assert!(t.unsupported_branches().is_empty());

    assert_eq!(
        t.read_branch(&f, "v.id").unwrap(),
        BranchValues::VecI32(vec![vec![0], vec![0, 1], vec![0, 1, 2]])
    );
    assert_eq!(
        t.read_branch(&f, "v.inner.a").unwrap(),
        BranchValues::VecF32(vec![vec![0.5], vec![0.5, 1.5], vec![0.5, 1.5, 2.5]])
    );
    assert_eq!(
        t.read_branch(&f, "v.inner.b").unwrap(),
        BranchValues::VecI32(vec![vec![0], vec![0, 2], vec![0, 2, 4]])
    );
    assert_eq!(
        t.read_branch(&f, "v.w").unwrap(),
        BranchValues::VecF32(vec![vec![0.0], vec![0.0, 1.5], vec![0.0, 1.5, 3.0]])
    );
}
