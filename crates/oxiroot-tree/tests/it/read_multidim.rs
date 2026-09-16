//! Read a multidimensional fixed-array branch float m[2][3]. The data is stored
//! row-major flat (6 per entry); the [2,3] shape is exposed via branch_shape.
use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};
use std::path::PathBuf;

#[test]
fn reads_multidim_array() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join("tree_multidim.root");
    let f = RFile::open(path).expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_shape("m"), Some([2usize, 3].as_slice()));
    assert_eq!(
        t.read_branch(&f, "m").expect("read m"),
        BranchValues::VecF32(vec![
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0],
            vec![20.0, 21.0, 22.0, 23.0, 24.0, 25.0],
        ])
    );
}
