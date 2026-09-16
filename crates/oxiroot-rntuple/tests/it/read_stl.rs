//! Fixed-size STL fields written by official ROOT: `std::array<int32_t, 3>` (an
//! array field with an element child) and `std::bitset<8>` (a repetition field
//! with its own Bit column). Both surface as one fixed-length chunk per entry.

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
    assert_eq!(ntpl.num_entries(), 4, "{name}");
    let field = |n| ntpl.read_field(&file, n).expect("read field");

    // std::array<int32_t, 3>: entry i = {i, i*10, i*100}.
    assert_eq!(
        field("arr"),
        FieldValues::VecI32(vec![
            vec![0, 0, 0],
            vec![1, 10, 100],
            vec![2, 20, 200],
            vec![3, 30, 300],
        ]),
        "{name} arr"
    );

    // std::bitset<8>: entry i = bits of (i*5), LSB-first. 0, 5(=101), 10(=1010),
    // 15(=1111).
    let t = true;
    let f = false;
    assert_eq!(
        field("bits"),
        FieldValues::VecBool(vec![
            vec![f, f, f, f, f, f, f, f],
            vec![t, f, t, f, f, f, f, f],
            vec![f, t, f, t, f, f, f, f],
            vec![t, t, t, t, f, f, f, f],
        ]),
        "{name} bits"
    );
}

#[test]
fn reads_stl_uncompressed() {
    check("rntuple_stl_uncompressed.root");
}

#[test]
fn reads_stl_zstd() {
    check("rntuple_stl_zstd.root");
}
