//! `TNtuple` / `TNtupleD` — the all-`float` / all-`double` `TTree` subclasses.
//! Their key class is `TNtuple`/`TNtupleD` (not `TTree`) and the streamed object
//! is a `TTree` base wrapped in one extra header plus a trailing `Int_t fNvar`;
//! the branch/leaf substructure is an ordinary flat tree. oxiroot reads them by
//! peeling the wrapper and reusing the plain-tree path. Cross-checked against
//! ROOT C++ and uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_ntuple.root"))
        .expect("open fixture")
}

#[test]
fn reads_tntuple_float() {
    let f = fixture();
    let t = TTree::open(&f, "nt").expect("open TNtuple");
    assert_eq!(t.num_entries(), 4);
    assert_eq!(t.branch_names(), vec!["x", "y", "z"]);
    assert_eq!(
        t.read_branch(&f, "x").unwrap(),
        BranchValues::F32(vec![1.5, 11.5, 21.5, 31.5])
    );
    assert_eq!(
        t.read_branch(&f, "z").unwrap(),
        BranchValues::F32(vec![3.5, 13.5, 23.5, 33.5])
    );
}

#[test]
fn reads_tntupled_double() {
    let f = fixture();
    let t = TTree::open(&f, "ntd").expect("open TNtupleD");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_names(), vec!["a", "b"]);
    assert_eq!(
        t.read_branch(&f, "a").unwrap(),
        BranchValues::F64(vec![100.25, 300.25, 500.25])
    );
    assert_eq!(
        t.read_branch(&f, "b").unwrap(),
        BranchValues::F64(vec![200.25, 400.25, 600.25])
    );
}
