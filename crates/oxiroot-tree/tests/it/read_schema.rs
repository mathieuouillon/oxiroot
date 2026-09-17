//! Streamer-info-driven schema validation + exposure (B10): TTree::open reads the
//! file's TStreamerInfo, validates the classes it parses, and exposes them.
use oxiroot_io_core::RFile;
use oxiroot_tree::TTree;
use std::path::PathBuf;

#[test]
fn exposes_and_validates_the_file_schema() {
    let f = RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join("tree_flat.root"),
    )
    .expect("open");
    let t = TTree::open(&f, "Events").expect("open tree");

    let classes: std::collections::HashMap<&str, i32> = t.streamer_classes().into_iter().collect();
    // The core classes the reader parses, at the versions it targets.
    assert_eq!(classes.get("TTree"), Some(&20));
    assert_eq!(classes.get("TBranch"), Some(&13));
    assert_eq!(classes.get("TLeaf"), Some(&2));
    // A leaf subclass is declared too.
    assert_eq!(classes.get("TLeafI"), Some(&1));
}
