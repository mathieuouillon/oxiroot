//! M6: streaming, multi-cluster RNTuple write. Each batch becomes one cluster;
//! the file must read back with all entries in order through our reader (and, as
//! separately verified, official ROOT and uproot).

use std::path::PathBuf;

use root_io_core::RFile;
use root_rntuple::{Column, Field, FieldValues, RNTuple, RNTupleWriter};

#[test]
fn streams_multiple_clusters() {
    let out = PathBuf::from("/tmp/rootrs_stream_ntuple.root");
    let mut w =
        RNTupleWriter::create(&out, "ntpl", root_io_core::Compression::None).expect("create");

    // Three clusters of 4 entries each (12 total), pushed one batch at a time.
    let mut expect_x = Vec::new();
    let mut expect_y = Vec::new();
    for cluster in 0..3i32 {
        let x: Vec<i32> = (0..4).map(|i| cluster * 100 + i).collect();
        let y: Vec<f64> = (0..4).map(|i| (cluster * 4 + i) as f64 * 0.5).collect();
        expect_x.extend_from_slice(&x);
        expect_y.extend_from_slice(&y);
        w.write_batch(&[
            Field {
                name: "x".into(),
                data: Column::I32(x),
            },
            Field {
                name: "y".into(),
                data: Column::F64(y),
            },
        ])
        .expect("write batch");
    }
    w.finish().expect("finish");

    let f = RFile::open(&out).expect("reopen");
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 12, "all entries across clusters");
    assert_eq!(
        ntpl.read_field(&f, "x").unwrap(),
        FieldValues::I32(expect_x)
    );
    assert_eq!(
        ntpl.read_field(&f, "y").unwrap(),
        FieldValues::F64(expect_y)
    );
}

#[test]
fn streams_collections_and_strings_across_clusters() {
    // vector<f32> and string fields spanning two clusters (compressed), with
    // empty collections/strings at cluster boundaries. The reader must re-base
    // each cluster's index offsets to reconstruct the values across clusters.
    let out = PathBuf::from("/tmp/rootrs_stream_coll.root");
    let mut w =
        RNTupleWriter::create(&out, "ntpl", root_io_core::Compression::Zstd(5)).expect("create");

    let v0 = vec![vec![], vec![1.0f32], vec![2.0, 2.0]];
    let s0 = vec!["a".to_string(), "bb".to_string(), "ccc".to_string()];
    w.write_batch(&[
        Field {
            name: "v".into(),
            data: Column::VecF32(v0.clone()),
        },
        Field {
            name: "s".into(),
            data: Column::Str(s0.clone()),
        },
    ])
    .expect("batch 0");

    let v1 = vec![vec![3.0f32, 3.0, 3.0], vec![]];
    let s1 = vec!["dddd".to_string(), String::new()];
    w.write_batch(&[
        Field {
            name: "v".into(),
            data: Column::VecF32(v1.clone()),
        },
        Field {
            name: "s".into(),
            data: Column::Str(s1.clone()),
        },
    ])
    .expect("batch 1");
    w.finish().expect("finish");

    let f = RFile::open(&out).expect("reopen");
    let ntpl = RNTuple::open(&f, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5);

    let mut expect_v = v0;
    expect_v.extend(v1);
    let mut expect_s = s0;
    expect_s.extend(s1);
    assert_eq!(
        ntpl.read_field(&f, "v").unwrap(),
        FieldValues::VecF32(expect_v)
    );
    assert_eq!(
        ntpl.read_field(&f, "s").unwrap(),
        FieldValues::Str(expect_s)
    );
}
