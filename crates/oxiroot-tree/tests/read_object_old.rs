//! Old-style unsplit object branches (`TBranchObject`, leaf `TLeafObject`): a
//! whole object stored per entry, the pre-`TBranchElement` mechanism. oxiroot
//! synthesizes one `branch.member` column per (basic / string) member of the
//! object class, decoding it out of every entry's object. Cross-checked against
//! ROOT C++ and uproot. The classes here cover string, double, and int members:
//! `TNamed` (fName/fTitle), `TParameter<double>`, and `TParameter<int>`.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture() -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_object_old.root"),
    )
    .expect("open fixture")
}

#[test]
fn reads_tbranchobject_members() {
    let f = fixture();
    let t = TTree::open(&f, "t").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    // Each TBranchObject expands into one column per (basic/string) member.
    assert_eq!(
        t.branch_names(),
        vec![
            "nm.fName",
            "nm.fTitle",
            "pd.fName",
            "pd.fVal",
            "pi.fName",
            "pi.fVal"
        ]
    );
    assert!(t.unsupported_branches().is_empty());

    // TNamed: two string members.
    assert_eq!(
        t.read_branch(&f, "nm.fName").unwrap(),
        BranchValues::Str(vec!["name0".into(), "name1".into(), "name2".into()])
    );
    assert_eq!(
        t.read_branch(&f, "nm.fTitle").unwrap(),
        BranchValues::Str(vec!["ttl0".into(), "ttl1".into(), "ttl2".into()])
    );
    // TParameter<double>: a string name and a double value.
    assert_eq!(
        t.read_branch(&f, "pd.fName").unwrap(),
        BranchValues::Str(vec!["pd".into(), "pd".into(), "pd".into()])
    );
    assert_eq!(
        t.read_branch(&f, "pd.fVal").unwrap(),
        BranchValues::F64(vec![0.5, 1.5, 2.5])
    );
    // TParameter<int>: a string name and an int value.
    assert_eq!(
        t.read_branch(&f, "pi.fVal").unwrap(),
        BranchValues::I32(vec![0, 10, 20])
    );
}

#[test]
fn object_member_introspection_and_range() {
    let f = fixture();
    let t = TTree::open(&f, "t").expect("open tree");
    // Introspection works on the synthesized columns.
    assert_eq!(t.branch_type("pd.fVal"), Some(oxiroot_tree::LeafType::F64));
    assert_eq!(t.branch_type("nm.fName"), Some(oxiroot_tree::LeafType::Str));
    // A range read slices the decoded column.
    assert_eq!(
        t.read_branch_range(&f, "pi.fVal", 1, 3).unwrap(),
        BranchValues::I32(vec![10, 20])
    );
}
