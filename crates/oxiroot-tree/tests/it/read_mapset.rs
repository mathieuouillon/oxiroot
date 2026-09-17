//! `std::set<int>` and `std::map<int,double>` `TTree` branches. ROOT writes a
//! `set<int>` as an unsplit object-wise collection (byte-identical to a
//! `vector<int>`) and a `map<int,double>` split into `m.first` / `m.second`
//! sub-branches sharing one counter (identical to a split `vector<pair>`), so
//! both read straight through oxiroot's existing collection paths. Cross-checked
//! against uproot. The fixture needs a runtime collection-proxy dictionary to
//! write (see scripts/gen_tree_mapset.sh); the on-disk bytes are stable.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_mapset.root"))
        .expect("open fixture")
}

#[test]
fn reads_set_and_map_branches() {
    let f = fixture();
    let t = TTree::open(&f, "t").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    // A set<int> branch is a single jagged integer collection.
    assert_eq!(t.branch_names(), vec!["s", "m.first", "m.second"]);
    assert!(t.unsupported_branches().is_empty());

    assert_eq!(
        t.read_branch(&f, "s").unwrap(),
        BranchValues::VecI32(vec![vec![11, 22], vec![100], vec![7, 8, 9]])
    );
    // map<int,double> splits into parallel key / value collections.
    assert_eq!(
        t.read_branch(&f, "m.first").unwrap(),
        BranchValues::VecI32(vec![vec![11, 22], vec![100], vec![7, 8, 9]])
    );
    assert_eq!(
        t.read_branch(&f, "m.second").unwrap(),
        BranchValues::VecF64(vec![vec![11.5, 22.5], vec![100.5], vec![7.5, 8.5, 9.5]])
    );
}
