//! The typed errors a caller can match on when reading a tree: a missing tree or
//! branch, and a key holding another class.

use oxiroot_io_core::{Compression, Error, FileReader, FileWriter, TObjString};
use oxiroot_tree::{tree_file_bytes, Branch, TreeReader};

#[test]
fn a_missing_tree_or_branch_is_not_found() {
    let bytes = tree_file_bytes(
        "f.root",
        "T",
        &[Branch::f64("x", vec![1.0, 2.0])],
        Compression::None,
    )
    .unwrap();
    let f = FileReader::from_bytes(bytes).unwrap();
    let err = TreeReader::open(&f, "nope").unwrap_err();
    assert!(
        matches!(&err, Error::NotFound { what: "key", name } if name == "nope"),
        "{err:?}"
    );

    let tree = TreeReader::open(&f, "T").unwrap();
    let err = tree.read_branch(&f, "y").unwrap_err();
    assert!(
        matches!(&err, Error::NotFound { what: "branch", name } if name == "y"),
        "{err:?}"
    );
    assert_eq!(err.to_string(), "no branch named \"y\"");
}

#[test]
fn a_key_that_is_not_a_tree_is_the_wrong_class() {
    let bytes = FileWriter::create("unused.root")
        .add(&TObjString::new("hi").named("s"))
        .to_bytes(Compression::None)
        .unwrap();
    let f = FileReader::from_bytes(bytes).unwrap();
    assert_eq!(
        TreeReader::open(&f, "s").unwrap_err(),
        Error::WrongClass {
            name: "s".into(),
            found: "TObjString".into(),
            expected: "TTree".into()
        }
    );
}
