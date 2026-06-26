//! Read a std::vector<std::string> TTree branch (B11).
use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};
use std::path::PathBuf;
#[test]
fn reads_vector_of_strings() {
    let f = RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join("tree_vecstring.root"),
    )
    .expect("open");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(
        t.read_branch(&f, "vs").expect("vs"),
        BranchValues::VecStr(vec![
            vec!["s00".into()],
            vec!["s10".into(), "s11".into()],
            vec!["s20".into(), "s21".into(), "s22".into()],
        ])
    );
}
