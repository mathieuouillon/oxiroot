//! Friend trees (`TTree::AddFriend`). The friend list is persisted in the main
//! tree's `fFriends` (a `TList<TFriendElement>`); oxiroot reads it back and the
//! friend's branches are read *positionally* — entry *i* of the main tree pairs
//! with entry *i* of the friend. Cross-checked against ROOT C++.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_friend.root"))
        .expect("open fixture")
}

#[test]
fn reads_persisted_friend_list() {
    let f = fixture();
    let t = TTree::open(&f, "t").expect("open main tree");
    let friends = t.friends();
    assert_eq!(friends.len(), 1);
    let fr = &friends[0];
    assert_eq!(fr.tree_name(), "tf");
    assert_eq!(fr.alias(), "tf");
    // AddFriend with no explicit file name ⇒ same-file friend (empty file name).
    assert_eq!(fr.file_name(), "");
    assert!(fr.is_same_file());
}

#[test]
fn friend_branch_aligns_by_entry() {
    let f = fixture();
    let t = TTree::open(&f, "t").expect("open main tree");
    let fr = &t.friends()[0];
    // The friend lives in the same file here, so reuse `f`.
    let friend = TTree::open(&f, fr.tree_name()).expect("open friend tree");

    let x = t.read_branch(&f, "x").unwrap();
    let y = friend.read_branch(&f, "y").unwrap();
    // Same number of entries: the columns line up 1:1.
    assert_eq!(x.len(), y.len());
    assert_eq!(x, BranchValues::F32(vec![1.5, 2.5, 3.5, 4.5, 5.5]));
    // y = run * 100 + evt for each entry, in main-tree order.
    assert_eq!(
        y,
        BranchValues::F64(vec![120.0, 110.0, 205.0, 207.0, 130.0])
    );
}

#[test]
fn a_tree_without_friends_has_an_empty_list() {
    // A plain tree's fFriends pointer is null; it reads back as no friends.
    let f = RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_flat.root"),
    )
    .expect("open flat fixture");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert!(t.friends().is_empty());
}
