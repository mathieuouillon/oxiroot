//! The small/medium integer physical column encodings written by official ROOT:
//! `Int8`/`UInt8`/`Int16`/`UInt16` (uncompressed) and `SplitInt16`/`SplitUInt16`
//! (Zstd-split). 8-bit columns have no split form. Includes a `vector<int16_t>`.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

fn check(name: &str) {
    let file = open(name);
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5, "{name}");
    let field = |n| ntpl.read_field(&file, n).expect("read field");

    assert_eq!(
        field("i8"),
        FieldValues::I8(vec![-2, -1, 0, 1, 2]),
        "{name} i8"
    );
    assert_eq!(
        field("u8"),
        FieldValues::U8(vec![250, 251, 252, 253, 254]),
        "{name} u8"
    );
    assert_eq!(
        field("i16"),
        FieldValues::I16(vec![-2000, -1000, 0, 1000, 2000]),
        "{name} i16"
    );
    assert_eq!(
        field("u16"),
        FieldValues::U16(vec![5, 10005, 20005, 30005, 40005]),
        "{name} u16"
    );
    // Float16_t / Double32_t without precision annotations are stored as full
    // Real32 / Real64, so they read back as ordinary f32 / f64.
    assert_eq!(
        field("f16"),
        FieldValues::F32(vec![0.25, 1.25, 2.25, 3.25, 4.25]),
        "{name} f16"
    );
    assert_eq!(
        field("d32"),
        FieldValues::F64(vec![0.0, 1.5, 3.0, 4.5, 6.0]),
        "{name} d32"
    );
    // std::vector<int16_t>: Index column + an Int16/SplitInt16 leaf.
    assert_eq!(
        field("vi16"),
        FieldValues::VecI16(vec![
            vec![],
            vec![101],
            vec![102, 102],
            vec![103, 103, 103],
            vec![104, 104, 104, 104],
        ]),
        "{name} vi16"
    );
}

#[test]
fn reads_integer_coltypes_uncompressed() {
    check("rntuple_coltypes_uncompressed.root");
}

#[test]
fn reads_integer_coltypes_zstd_split() {
    check("rntuple_coltypes_zstd.root");
}
