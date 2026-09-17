//! M3 end-to-end: decode every column and reconstruct field values. Runs on
//! both the uncompressed (non-split) and Zstd (split-encoded) fixtures, which
//! must decode to identical values.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{ColumnValues, RNTuple};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

fn check_fixture(name: &str) {
    let file = open(name);
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5, "{name}");

    let col = |i| ntpl.read_column(&file, i).expect("read column");

    // Scalars. Same values whether stored plain (uncompressed) or split (Zstd).
    assert_eq!(
        col(0),
        ColumnValues::I32(vec![0, 10, 20, 30, 40]),
        "{name} i32"
    );
    assert_eq!(
        col(1),
        ColumnValues::F32(vec![0.5, 1.5, 2.5, 3.5, 4.5]),
        "{name} f32"
    );
    assert_eq!(
        col(2),
        ColumnValues::F64(vec![0.0, 1.25, 2.5, 3.75, 5.0]),
        "{name} f64"
    );
    assert_eq!(
        col(3),
        ColumnValues::Bits(vec![true, false, true, false, true]),
        "{name} b"
    );

    // String field: Index64/SplitIndex64 offsets + Char bytes.
    let s_offsets = as_u64(col(4));
    assert_eq!(s_offsets, vec![4, 8, 12, 16, 20], "{name} s offsets");
    let s_bytes = match col(5) {
        ColumnValues::Bytes(v) => v,
        other => panic!("{name}: expected Bytes, got {other:?}"),
    };
    assert_eq!(&s_bytes, b"row0row1row2row3row4", "{name} s bytes");
    assert_eq!(
        reconstruct_strings(&s_offsets, &s_bytes),
        ["row0", "row1", "row2", "row3", "row4"]
    );

    // Vector<float> field: Index offsets + Real32 data.
    let vf_offsets = as_u64(col(6));
    assert_eq!(vf_offsets, vec![0, 1, 3, 6, 10], "{name} vf offsets");
    let vf_data = match col(7) {
        ColumnValues::F32(v) => v,
        other => panic!("{name}: expected F32, got {other:?}"),
    };
    assert_eq!(
        vf_data,
        vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0],
        "{name} vf data"
    );
    let expected_vf: Vec<Vec<f32>> = vec![
        vec![],
        vec![1.0],
        vec![2.0, 2.0],
        vec![3.0, 3.0, 3.0],
        vec![4.0, 4.0, 4.0, 4.0],
    ];
    assert_eq!(
        reconstruct_collections(&vf_offsets, &vf_data),
        expected_vf,
        "{name} vf"
    );
}

#[test]
fn decodes_uncompressed() {
    check_fixture("rntuple_scalars_uncompressed.root");
}

#[test]
fn decodes_zstd_split() {
    check_fixture("rntuple_scalars_zstd.root");
}

fn as_u64(c: ColumnValues) -> Vec<u64> {
    match c {
        ColumnValues::U64(v) => v,
        other => panic!("expected U64 offsets, got {other:?}"),
    }
}

fn reconstruct_strings(offsets: &[u64], bytes: &[u8]) -> Vec<String> {
    let mut start = 0usize;
    offsets
        .iter()
        .map(|&end| {
            let end = end as usize;
            let s = String::from_utf8(bytes[start..end].to_vec()).unwrap();
            start = end;
            s
        })
        .collect()
}

fn reconstruct_collections<T: Clone>(offsets: &[u64], data: &[T]) -> Vec<Vec<T>> {
    let mut start = 0usize;
    offsets
        .iter()
        .map(|&end| {
            let end = end as usize;
            let slice = data[start..end].to_vec();
            start = end;
            slice
        })
        .collect()
}
