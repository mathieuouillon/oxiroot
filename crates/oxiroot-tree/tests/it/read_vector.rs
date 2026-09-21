//! Read `std::vector<T>` `TBranchElement` branches (written by ROOT C++).

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_std_vector_branches() {
    let f = RFile::open(fixture("tree_vector.root")).expect("open");
    let t = TTree::open(&f, "T").expect("open tree");
    let read = |n| t.read_branch(&f, n).expect("read branch");

    assert_eq!(t.num_entries(), 4);
    // The scalar branch and the three vector branches are all visible.
    assert_eq!(t.branch_names(), ["n", "vf", "vd", "vi"]);

    assert_eq!(read("n"), BranchValues::I32(vec![0, 1, 2, 3]));
    assert_eq!(
        read("vf"),
        BranchValues::VecF32(vec![vec![1.0, 2.0, 3.0], vec![], vec![4.0], vec![5.0, 6.0]])
    );
    assert_eq!(
        read("vd"),
        BranchValues::VecF64(vec![vec![1.5], vec![2.5, 3.5], vec![], vec![]])
    );
    assert_eq!(
        read("vi"),
        BranchValues::VecI32(vec![vec![10, 20], vec![30], vec![], vec![40, 50, 60]])
    );
}
