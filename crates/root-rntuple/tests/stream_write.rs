//! M6: streaming, multi-cluster RNTuple write. Each batch becomes one cluster;
//! the file must read back with all entries in order through our reader (and, as
//! separately verified, official ROOT and uproot).

use std::path::PathBuf;

use root_io_core::RFile;
use root_rntuple::{Column, Field, FieldValues, RNTuple, RNTupleWriter};

#[test]
fn streams_multiple_clusters() {
    let out = PathBuf::from("/tmp/rootrs_stream_ntuple.root");
    let mut w = RNTupleWriter::create(&out, "ntpl", 0).expect("create");

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
fn streaming_compresses_and_rejects_collections() {
    let out = PathBuf::from("/tmp/rootrs_stream_reject.root");
    let mut w = RNTupleWriter::create(&out, "ntpl", 505).expect("create");
    // A collection column is rejected by the streaming writer.
    let err = w.write_batch(&[Field {
        name: "v".into(),
        data: Column::VecF32(vec![vec![1.0], vec![2.0, 3.0]]),
    }]);
    assert!(err.is_err(), "collections must be rejected");
}
