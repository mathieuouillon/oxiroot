//! M5/#2: write an RNTuple with scalar, string, and vector fields, then read it
//! back through our own reader.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{Column, Field, FieldValues, RNTuple};

#[test]
fn writes_a_rich_rntuple_that_round_trips() {
    let fields = vec![
        Field {
            name: "i32".into(),
            data: Column::I32(vec![0, 10, 20, 30, 40]),
        },
        Field {
            name: "i64".into(),
            data: Column::I64(vec![-1, -2, -3, -4, -5]),
        },
        Field {
            name: "f32".into(),
            data: Column::F32(vec![0.5, 1.5, 2.5, 3.5, 4.5]),
        },
        Field {
            name: "f64".into(),
            data: Column::F64(vec![0.0, 1.25, 2.5, 3.75, 5.0]),
        },
        Field {
            name: "b".into(),
            data: Column::Bool(vec![true, false, true, false, true]),
        },
        Field {
            name: "s".into(),
            data: Column::Str(
                ["row0", "row1", "row2", "row3", "row4"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
        },
        Field {
            name: "vf".into(),
            data: Column::VecF32(vec![
                vec![],
                vec![1.0],
                vec![2.0, 2.0],
                vec![3.0, 3.0, 3.0],
                vec![4.0, 4.0, 4.0, 4.0],
            ]),
        },
    ];

    let out = PathBuf::from("/tmp/rootrs_written_rich_ntuple.root");
    oxiroot_rntuple::write_rntuple_file(&out, "ntpl", &fields, oxiroot_io_core::Compression::None)
        .expect("write rntuple");

    let f = RFile::open(&out).expect("reopen");
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5);

    let field = |n| ntpl.read_field(&f, n).expect("read field");
    assert_eq!(field("i32"), FieldValues::I32(vec![0, 10, 20, 30, 40]));
    assert_eq!(field("i64"), FieldValues::I64(vec![-1, -2, -3, -4, -5]));
    assert_eq!(
        field("f32"),
        FieldValues::F32(vec![0.5, 1.5, 2.5, 3.5, 4.5])
    );
    assert_eq!(
        field("f64"),
        FieldValues::F64(vec![0.0, 1.25, 2.5, 3.75, 5.0])
    );
    assert_eq!(
        field("b"),
        FieldValues::Bool(vec![true, false, true, false, true])
    );
    assert_eq!(
        field("s"),
        FieldValues::Str(
            ["row0", "row1", "row2", "row3", "row4"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        )
    );
    assert_eq!(
        field("vf"),
        FieldValues::VecF32(vec![
            vec![],
            vec![1.0],
            vec![2.0, 2.0],
            vec![3.0, 3.0, 3.0],
            vec![4.0, 4.0, 4.0, 4.0],
        ])
    );
}

#[test]
fn writes_unsigned_and_more_vector_types() {
    let fields = vec![
        Field {
            name: "u32".into(),
            data: Column::U32(vec![1, 2, 3_000_000_000]), // > i32::MAX, exercises unsigned
        },
        Field {
            name: "u64".into(),
            data: Column::U64(vec![1, 2, 10_000_000_000]),
        },
        Field {
            name: "vi64".into(),
            data: Column::VecI64(vec![vec![], vec![-1], vec![2, 3]]),
        },
        Field {
            name: "vb".into(),
            data: Column::VecBool(vec![vec![true], vec![], vec![false, true]]),
        },
    ];

    let out = PathBuf::from("/tmp/rootrs_more_types.root");
    oxiroot_rntuple::write_rntuple_file(&out, "ntpl", &fields, oxiroot_io_core::Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    let field = |n| ntpl.read_field(&f, n).expect("read field");
    // u32 round-trips with its 32-bit identity intact (not widened to u64).
    assert_eq!(field("u32"), FieldValues::U32(vec![1, 2, 3_000_000_000]));
    assert_eq!(field("u64"), FieldValues::U64(vec![1, 2, 10_000_000_000]));
    assert_eq!(
        field("vi64"),
        FieldValues::VecI64(vec![vec![], vec![-1], vec![2, 3]])
    );
    assert_eq!(
        field("vb"),
        FieldValues::VecBool(vec![vec![true], vec![], vec![false, true]])
    );
}

#[test]
fn writes_a_zstd_compressed_rntuple_that_round_trips() {
    // Highly compressible columns so the pages are actually stored compressed.
    let n = 1000usize;
    let x: Vec<i32> = (0..n as i32).map(|i| i % 4).collect();
    let y: Vec<f64> = vec![2.5; n];
    let fields = vec![
        Field {
            name: "x".into(),
            data: Column::I32(x.clone()),
        },
        Field {
            name: "y".into(),
            data: Column::F64(y.clone()),
        },
    ];

    let out = PathBuf::from("/tmp/rootrs_written_ntuple_zstd.root");
    oxiroot_rntuple::write_rntuple_file(
        &out,
        "ntpl",
        &fields,
        oxiroot_io_core::Compression::Zstd(5),
    )
    .expect("write compressed");

    // The file must be much smaller than the raw column bytes (4000 + 8000).
    let file_len = std::fs::metadata(&out).unwrap().len();
    assert!(
        file_len < 6000,
        "expected compressed file, got {file_len} bytes"
    );

    let f = RFile::open(&out).expect("reopen");
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), n as u64);
    assert_eq!(ntpl.read_field(&f, "x").expect("x"), FieldValues::I32(x));
    assert_eq!(ntpl.read_field(&f, "y").expect("y"), FieldValues::F64(y));
}
