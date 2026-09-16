//! A user-defined class written by official ROOT (with a rootcling dictionary).
//! ROOT splits a class with a dictionary into a Record of named sub-fields, so
//! it reads back through the existing recursive Record/Collection path.
use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};
use std::path::PathBuf;
fn open(name: &str) -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open")
}
#[test]
fn reads_user_class() {
    let file = open("rntuple_user_uncompressed.root");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open");
    // Top-level struct -> Record of named sub-fields.
    assert_eq!(
        ntpl.read_field(&file, "hit").expect("hit"),
        FieldValues::Record(vec![
            ("id".into(), FieldValues::I32(vec![0, 1, 2])),
            ("energy".into(), FieldValues::F64(vec![0.5, 1.5, 2.5])),
        ])
    );
    // vector<struct> -> Nested over a Record.
    assert_eq!(
        ntpl.read_field(&file, "vhit").expect("vhit"),
        FieldValues::Nested {
            offsets: vec![1, 3, 6],
            items: Box::new(FieldValues::Record(vec![
                ("id".into(), FieldValues::I32(vec![0, 0, 2, 0, 2, 4])),
                (
                    "energy".into(),
                    FieldValues::F64(vec![0.0, 0.0, 1.0, 0.0, 1.0, 2.0])
                ),
            ])),
        }
    );
}
