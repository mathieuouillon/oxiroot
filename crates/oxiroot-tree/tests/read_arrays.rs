//! Read fixed-size arrays, variable/jagged arrays, and string branches.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_arrays_and_strings() {
    let f = RFile::open(fixture("tree_arrays.root")).expect("open");
    let t = TTree::open(&f, "T").expect("open tree");
    let read = |n| t.read_branch(&f, n).expect("read branch");

    // Fixed-size array x[3].
    assert_eq!(
        read("x"),
        BranchValues::VecF64(vec![
            vec![0.0, 1.0, 2.0],
            vec![10.0, 11.0, 12.0],
            vec![20.0, 21.0, 22.0],
        ])
    );
    // The auto-generated count branch is a plain scalar.
    assert_eq!(read("ny"), BranchValues::I32(vec![2, 0, 3]));
    // Variable-length (jagged) branch.
    assert_eq!(
        read("y"),
        BranchValues::VecF64(vec![vec![1.0, 2.0], vec![], vec![3.0, 4.0, 5.0]])
    );
    // String branch.
    assert_eq!(
        read("s"),
        BranchValues::Str(vec!["a".into(), "bb".into(), "ccc".into()])
    );
}
