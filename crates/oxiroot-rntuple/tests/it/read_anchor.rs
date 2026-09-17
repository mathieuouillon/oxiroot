//! M3 (chunk 1): parse + checksum-verify the RNTuple anchor and load the
//! header/footer envelopes from a real ROOT fixture.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{ColumnType, RNTuple, StructRole};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn parses_anchor_and_envelopes() {
    let f = RFile::open(fixture("rntuple_scalars_uncompressed.root")).expect("open fixture");

    // The key is the RNTuple anchor.
    assert_eq!(f.key("ntpl").unwrap().class_name, "ROOT::RNTuple");

    // open() parses + checksum-verifies the anchor and both envelopes.
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    let a = ntpl.anchor();

    // Anchor field values decoded earlier from the raw bytes.
    assert_eq!(a.seek_header, 312);
    assert_eq!(a.nbytes_header, 617);
    assert_eq!(a.len_header, 617); // uncompressed: nbytes == len
    assert_eq!(a.seek_footer, 1744);
    assert_eq!(a.nbytes_footer, 160);
    assert_eq!(a.len_footer, 160);
    assert_eq!(a.max_key_size, 0x4000_0000); // 1 GiB default

    // The header envelope is the whole on-disk blob (its type and checksum are
    // checked by `open`, and parsed in the envelope module's unit tests).
    assert_eq!(ntpl.header_envelope().len(), 617);
}

#[test]
fn parses_schema() {
    let f = RFile::open(fixture("rntuple_scalars_uncompressed.root")).expect("open fixture");
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    let h = ntpl.header();

    // Seven fields, including the `_0` float child of the `vf` collection.
    let fields: Vec<(&str, &str, StructRole)> = h
        .fields
        .iter()
        .map(|fd| (fd.name.as_str(), fd.type_name.as_str(), fd.struct_role))
        .collect();
    assert_eq!(
        fields,
        vec![
            ("i32", "std::int32_t", StructRole::Leaf),
            ("f32", "float", StructRole::Leaf),
            ("f64", "double", StructRole::Leaf),
            ("b", "bool", StructRole::Leaf),
            ("s", "std::string", StructRole::Leaf),
            ("vf", "std::vector<float>", StructRole::Collection),
            ("_0", "float", StructRole::Leaf),
        ]
    );

    // Eight columns: types and the field each belongs to.
    let columns: Vec<(ColumnType, u32)> = h
        .columns
        .iter()
        .map(|c| (c.column_type, c.field_id))
        .collect();
    assert_eq!(
        columns,
        vec![
            (ColumnType::Int32, 0),
            (ColumnType::Real32, 1),
            (ColumnType::Real64, 2),
            (ColumnType::Bit, 3),
            (ColumnType::Index64, 4), // string offsets
            (ColumnType::Char, 4),    // string bytes
            (ColumnType::Index64, 5), // vector offsets
            (ColumnType::Real32, 6),  // vector<float> data
        ]
    );
}
