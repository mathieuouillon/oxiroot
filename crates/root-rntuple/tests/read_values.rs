//! M3 end-to-end: decode every column of the RNTuple fixture and reconstruct
//! the field values, validating against what uproot reports.

use std::path::PathBuf;

use root_io_core::RFile;
use root_rntuple::{ColumnValues, RNTuple};

fn open() -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join("rntuple_scalars_uncompressed.root");
    RFile::open(path).expect("open fixture")
}

#[test]
fn decodes_all_columns_and_fields() {
    let file = open();
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5);

    let col = |i| ntpl.read_column(&file, i).expect("read column");

    // Scalar leaf columns.
    assert_eq!(col(0), ColumnValues::I32(vec![0, 10, 20, 30, 40]));
    assert_eq!(col(1), ColumnValues::F32(vec![0.5, 1.5, 2.5, 3.5, 4.5]));
    assert_eq!(col(2), ColumnValues::F64(vec![0.0, 1.25, 2.5, 3.75, 5.0]));
    assert_eq!(
        col(3),
        ColumnValues::Bits(vec![true, false, true, false, true])
    );

    // String field `s`: an Index64 offset column + a Char data column.
    let s_offsets = match col(4) {
        ColumnValues::U64(v) => v,
        other => panic!("expected U64 offsets, got {other:?}"),
    };
    assert_eq!(s_offsets, vec![4, 8, 12, 16, 20]);
    let s_bytes = match col(5) {
        ColumnValues::Bytes(v) => v,
        other => panic!("expected Bytes, got {other:?}"),
    };
    assert_eq!(&s_bytes, b"row0row1row2row3row4");
    assert_eq!(
        reconstruct_strings(&s_offsets, &s_bytes),
        ["row0", "row1", "row2", "row3", "row4"]
    );

    // Vector field `vf`: an Index64 offset column + a Real32 data column.
    let vf_offsets = match col(6) {
        ColumnValues::U64(v) => v,
        other => panic!("expected U64 offsets, got {other:?}"),
    };
    assert_eq!(vf_offsets, vec![0, 1, 3, 6, 10]);
    let vf_data = match col(7) {
        ColumnValues::F32(v) => v,
        other => panic!("expected F32, got {other:?}"),
    };
    assert_eq!(
        vf_data,
        vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0]
    );
    let expected_vf: Vec<Vec<f32>> = vec![
        vec![],
        vec![1.0],
        vec![2.0, 2.0],
        vec![3.0, 3.0, 3.0],
        vec![4.0, 4.0, 4.0, 4.0],
    ];
    assert_eq!(reconstruct_collections(&vf_offsets, &vf_data), expected_vf);
}

/// Reconstruct per-entry strings from cumulative offsets + the byte data.
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

/// Reconstruct per-entry collections from cumulative offsets + the flat data.
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
