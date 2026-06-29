//! A split *single* struct branch (`t.Branch("o", &outer)`), as opposed to a
//! split collection. ROOT splits the one object into scalar member sub-branches
//! (`id`, `inner.a`, `inner.b`, `w`) — one value per entry, not jagged. The
//! nested struct `Inner` flattens into `inner.a`/`inner.b`. Cross-checked against
//! ROOT C++ and uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

#[test]
fn reads_split_single_object_scalars() {
    let f = RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_object.root"),
    )
    .expect("open fixture");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_names(), vec!["id", "inner.a", "inner.b", "w"]);
    assert!(t.unsupported_branches().is_empty());

    // Each member is a scalar (one value per entry), not a jagged array.
    assert_eq!(
        t.read_branch(&f, "id").unwrap(),
        BranchValues::I32(vec![0, 1, 2])
    );
    assert_eq!(
        t.read_branch(&f, "inner.a").unwrap(),
        BranchValues::F32(vec![0.5, 1.5, 2.5])
    );
    assert_eq!(
        t.read_branch(&f, "inner.b").unwrap(),
        BranchValues::I32(vec![0, 2, 4])
    );
    assert_eq!(
        t.read_branch(&f, "w").unwrap(),
        BranchValues::F32(vec![0.0, 1.5, 3.0])
    );
}
