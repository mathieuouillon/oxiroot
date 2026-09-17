//! Aliases & selections. `TTree::SetAlias` stores `(name, expression)` pairs in
//! the tree's `fAliases` (a `TList<TNamed>`); a `TEntryList` is a standalone key
//! holding a bit array of selected entry numbers. Both cross-checked against
//! ROOT C++ / uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{TEntryList, TTree};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_alias.root"))
        .expect("open fixture")
}

#[test]
fn reads_tree_aliases() {
    let f = fixture();
    let t = TTree::open(&f, "t").expect("open tree");
    assert_eq!(
        t.aliases(),
        &[
            ("z".to_string(), "x+y".to_string()),
            ("twice".to_string(), "2*x".to_string()),
        ]
    );
    assert_eq!(t.alias("z"), Some("x+y"));
    assert_eq!(t.alias("twice"), Some("2*x"));
    assert_eq!(t.alias("missing"), None);
}

#[test]
fn a_tree_without_aliases_has_none() {
    let f = RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree_flat.root"),
    )
    .expect("open flat fixture");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert!(t.aliases().is_empty());
    assert_eq!(t.alias("anything"), None);
}

#[test]
fn reads_entry_list() {
    let f = fixture();
    let el = TEntryList::open(&f, "elist").expect("open entry list");
    assert_eq!(el.name(), "elist");
    assert_eq!(el.tree_name(), "t");
    assert_eq!(el.entries(), &[0, 2, 4]);
    assert_eq!(el.len(), 3);
    assert!(!el.is_empty());
    assert!(el.contains(2));
    assert!(!el.contains(1));
    assert!(!el.contains(3));
}

#[test]
fn opening_a_non_entry_list_key_errors() {
    let f = fixture();
    assert!(TEntryList::open(&f, "t").is_err()); // "t" is a TTree
}
