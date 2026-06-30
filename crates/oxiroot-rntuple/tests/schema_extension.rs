//! Reading a schema-extended RNTuple: a field added after the header, via the
//! footer's schema-extension record. oxiroot merges the extension fields/columns
//! into the schema and back-fills the deferred column's leading entries (the
//! ones written before the field existed), which ROOT defaults to 0. The fixture
//! is ROOT-C++-written (scripts/gen_rntuple_ext.cpp); cross-checked with uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rntuple_ext.root"))
        .expect("open fixture")
}

#[test]
fn reads_late_added_field() {
    let f = fixture();
    let nt = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    // The late field "y" (in the footer schema-extension record) is visible.
    assert_eq!(nt.field_names(), vec!["x", "y"]);
    assert_eq!(nt.num_entries(), 4);

    // "x" is in the header and present for every entry.
    assert_eq!(
        nt.read_field(&f, "x").unwrap(),
        FieldValues::I32(vec![1, 2, 3, 4])
    );
    // "y" was added after entries 0-1, which ROOT defaults to 0.0.
    assert_eq!(
        nt.read_field(&f, "y").unwrap(),
        FieldValues::F32(vec![0.0, 0.0, 3.5, 4.5])
    );
}
