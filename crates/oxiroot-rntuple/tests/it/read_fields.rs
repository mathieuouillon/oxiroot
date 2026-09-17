//! M3+ typed field API: read fields by name and get reconstructed per-entry
//! values, on both the uncompressed and Zstd (split) fixtures.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

fn check_fields(name: &str) {
    let file = open(name);
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");

    assert_eq!(
        ntpl.field_names(),
        ["i32", "f32", "f64", "b", "s", "vf"],
        "{name} field names"
    );

    let field = |n| ntpl.read_field(&file, n).expect("read field");

    assert_eq!(
        field("i32"),
        FieldValues::I32(vec![0, 10, 20, 30, 40]),
        "{name}"
    );
    assert_eq!(
        field("f32"),
        FieldValues::F32(vec![0.5, 1.5, 2.5, 3.5, 4.5]),
        "{name}"
    );
    assert_eq!(
        field("f64"),
        FieldValues::F64(vec![0.0, 1.25, 2.5, 3.75, 5.0]),
        "{name}"
    );
    assert_eq!(
        field("b"),
        FieldValues::Bool(vec![true, false, true, false, true]),
        "{name}"
    );
    assert_eq!(
        field("s"),
        FieldValues::Str(vec![
            "row0".into(),
            "row1".into(),
            "row2".into(),
            "row3".into(),
            "row4".into()
        ]),
        "{name}"
    );
    assert_eq!(
        field("vf"),
        FieldValues::VecF32(vec![
            vec![],
            vec![1.0],
            vec![2.0, 2.0],
            vec![3.0, 3.0, 3.0],
            vec![4.0, 4.0, 4.0, 4.0],
        ]),
        "{name}"
    );
}

#[test]
fn reads_fields_uncompressed() {
    check_fields("rntuple_scalars_uncompressed.root");
}

#[test]
fn reads_fields_zstd_split() {
    check_fields("rntuple_scalars_zstd.root");
}

/// A `std::vector<float>` field written by official ROOT across three clusters
/// (with split/delta index encoding). The reader must re-base each cluster's
/// index offsets to reconstruct the per-entry collections correctly.
#[test]
fn reads_real_root_multicluster_collection() {
    let file = open("rntuple_multicluster_vec.root");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 9);

    assert_eq!(
        ntpl.read_field(&file, "n").expect("n"),
        FieldValues::I32((0..9).collect())
    );
    assert_eq!(
        ntpl.read_field(&file, "v").expect("v"),
        FieldValues::VecF32(vec![
            vec![],
            vec![10.0],
            vec![20.0, 21.0],
            vec![30.0, 31.0, 32.0],
            vec![],
            vec![50.0],
            vec![60.0, 61.0],
            vec![70.0, 71.0, 72.0],
            vec![],
        ])
    );
}
