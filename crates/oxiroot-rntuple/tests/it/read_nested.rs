//! Nested collection fields written by official ROOT: `std::vector<std::string>`,
//! `std::vector<std::vector<int32_t>>`, and `std::vector<std::pair<int32,double>>`
//! (a vector of records). Read on both the uncompressed (non-split) and Zstd
//! (split index/leaf) fixtures.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

fn check_nested(name: &str) {
    let file = open(name);
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5, "{name} entries");
    assert_eq!(
        ntpl.field_names(),
        ["vs", "vvi", "vp"],
        "{name} field names"
    );
    let field = |n| ntpl.read_field(&file, n).expect("read field");

    // std::vector<std::string>: one Vec<String> per entry.
    assert_eq!(
        field("vs"),
        FieldValues::VecStr(vec![
            vec![],
            vec!["r1_0".into()],
            vec!["r2_0".into(), "r2_1".into()],
            vec!["r3_0".into(), "r3_1".into(), "r3_2".into()],
            vec!["r4_0".into(), "r4_1".into(), "r4_2".into(), "r4_3".into()],
        ]),
        "{name} vs"
    );

    // std::vector<std::vector<int32_t>>: outer offsets partition the flattened
    // inner vectors (a VecI32). Inner vector j of entry i has length j+1, all =
    // i*10+j.
    assert_eq!(
        field("vvi"),
        FieldValues::Nested {
            offsets: vec![0, 1, 3, 6, 10],
            items: Box::new(FieldValues::VecI32(vec![
                vec![10], // entry 1
                vec![20], // entry 2
                vec![21, 21],
                vec![30], // entry 3
                vec![31, 31],
                vec![32, 32, 32],
                vec![40], // entry 4
                vec![41, 41],
                vec![42, 42, 42],
                vec![43, 43, 43, 43],
            ])),
        },
        "{name} vvi"
    );

    // std::vector<std::pair<int32,double>>: outer offsets partition a record of
    // two struct-of-arrays leaves (_0: int32 id, _1: double). Pair j of entry i
    // is (i*100+j, i + 0.5*j).
    assert_eq!(
        field("vp"),
        FieldValues::Nested {
            offsets: vec![0, 1, 3, 6, 10],
            items: Box::new(FieldValues::Record(vec![
                (
                    "_0".into(),
                    FieldValues::I32(vec![100, 200, 201, 300, 301, 302, 400, 401, 402, 403]),
                ),
                (
                    "_1".into(),
                    FieldValues::F64(vec![1.0, 2.0, 2.5, 3.0, 3.5, 4.0, 4.0, 4.5, 5.0, 5.5]),
                ),
            ])),
        },
        "{name} vp"
    );
}

#[test]
fn reads_nested_uncompressed() {
    check_nested("rntuple_nested_uncompressed.root");
}

#[test]
fn reads_nested_zstd_split() {
    check_nested("rntuple_nested_zstd.root");
}
