//! Branch introspection (B1): type/len/title accessors, BranchValues helpers,
//! and the unsupported-branch diagnostics, on a flat primitive tree.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{LeafType, TTree};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn introspects_branches_without_reading() {
    let f = RFile::open(fixture("tree_flat.root")).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");

    // Element type + shape are available without reading the data.
    assert_eq!(t.branch_type("i4"), Some(LeafType::I32));
    assert_eq!(t.branch_type("f8"), Some(LeafType::F64));
    assert_eq!(t.branch_type("b1"), Some(LeafType::Bool));
    assert_eq!(t.branch_type("missing"), None);
    assert_eq!(t.branch_len("i4"), Some(1)); // scalar
    assert!(t.branch_title("i4").is_some());

    // A clean tree reports no unsupported branches.
    assert!(t.unsupported_branches().is_empty());
}

#[test]
fn branch_values_helpers() {
    let f = RFile::open(fixture("tree_flat.root")).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");

    let i4 = t.read_branch(&f, "i4").expect("read i4");
    assert_eq!(i4.len(), 5);
    assert!(!i4.is_empty());
    assert_eq!(i4.leaf_type(), LeafType::I32);
    assert_eq!(i4.as_i32(), Some([0, 1, 2, 3, 4].as_slice()));
    assert_eq!(i4.as_f64(), None); // wrong-type accessor returns None

    let f8 = t.read_branch(&f, "f8").expect("read f8");
    assert_eq!(f8.as_f64(), Some([0.25, 1.25, 2.25, 3.25, 4.25].as_slice()));
}
