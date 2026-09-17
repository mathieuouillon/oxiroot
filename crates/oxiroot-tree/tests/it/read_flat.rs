//! Read a flat primitive TTree written by uproot; values must match.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_flat_primitive_tree() {
    let f = RFile::open(fixture("tree_flat.root")).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 5);
    assert_eq!(t.branch_names(), ["i4", "i8", "f4", "f8", "b1", "u4"]);
    check_values(&f, &t);
}

/// A Zstd-compressed tree exercises the basket decompression path.
#[test]
fn reads_compressed_tree() {
    let f = RFile::open(fixture("tree_zstd.root")).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");
    check_values(&f, &t);
}

/// Two baskets per branch exercises cross-basket concatenation.
#[test]
fn reads_multi_basket_tree() {
    let f = RFile::open(fixture("tree_multibasket.root")).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 5);
    check_values(&f, &t);
}

fn check_values(f: &RFile, t: &TTree) {
    let read = |n| t.read_branch(f, n).expect("read branch");
    assert_eq!(read("i4"), BranchValues::I32(vec![0, 1, 2, 3, 4]));
    assert_eq!(read("i8"), BranchValues::I64(vec![10, 11, 12, 13, 14]));
    assert_eq!(read("f4"), BranchValues::F32(vec![0.5, 1.5, 2.5, 3.5, 4.5]));
    assert_eq!(
        read("f8"),
        BranchValues::F64(vec![0.25, 1.25, 2.25, 3.25, 4.25])
    );
    assert_eq!(
        read("b1"),
        BranchValues::Bool(vec![true, false, true, false, true])
    );
    assert_eq!(
        read("u4"),
        BranchValues::U32(vec![100, 200, 300, 400, 4_000_000_000])
    );
}
