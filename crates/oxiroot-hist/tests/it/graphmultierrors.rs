//! TGraphMultiErrors: read a ROOT fixture, self round-trip, and build from
//! scratch. (Cross-checked against compiled ROOT C++ — uproot cannot decode the
//! memberwise-streamed attribute vectors, so ROOT C++ is the oracle here.)

use std::path::PathBuf;

use oxiroot_hist::{Compression, ReadRoot, TGraphMultiErrors, WriteRoot};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_root_written_graphmultierrors() {
    let f = RFile::open(fixture("graphmultierrors.root")).expect("open");
    let g = TGraphMultiErrors::read_root(&f, "gme").expect("read gme");
    assert_eq!(g.name, "gme");
    assert_eq!(g.title, "multi");
    assert_eq!(g.x, vec![1.0, 2.0, 3.0]);
    assert_eq!(g.y, vec![10.0, 20.0, 30.0]);
    assert_eq!(g.ex_low, vec![0.1, 0.1, 0.1]);
    assert_eq!(g.ex_high, vec![0.2, 0.2, 0.2]);
    assert_eq!(g.n_y_errors(), 2);
    assert_eq!(g.ey_low, vec![vec![1.0, 1.0, 1.0], vec![3.0, 3.0, 3.0]]);
    assert_eq!(g.ey_high, vec![vec![2.0, 2.0, 2.0], vec![4.0, 4.0, 4.0]]);
}

#[test]
fn graphmultierrors_round_trip_from_fixture() {
    let f = RFile::open(fixture("graphmultierrors.root")).expect("open");
    let g = TGraphMultiErrors::read_root(&f, "gme").unwrap();
    let out = std::env::temp_dir().join("oxiroot_gme_rt.root");
    g.write_root(&out, Compression::None).expect("write");
    let back = TGraphMultiErrors::read_root(&RFile::open(&out).unwrap(), "gme").unwrap();
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn graphmultierrors_build_from_scratch() {
    let g = TGraphMultiErrors::new(
        vec![1.0, 2.0, 3.0],
        vec![10.0, 20.0, 30.0],
        vec![0.5, 0.5, 0.5],
        vec![0.5, 0.5, 0.5],
        vec![1.0, 2.0, 3.0], // statistical y error
        vec![1.0, 2.0, 3.0],
    )
    .add_y_error(vec![0.5, 0.5, 0.5], vec![0.5, 0.5, 0.5]) // systematic layer
    .named("multi")
    .titled("two error sources");
    assert_eq!(g.n_y_errors(), 2);

    let out = std::env::temp_dir().join("oxiroot_gme_scratch.root");
    g.write_root(&out, Compression::Zstd(3)).expect("write");
    let back = TGraphMultiErrors::read_root(&RFile::open(&out).unwrap(), "multi").unwrap();
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn empty_graphmultierrors_round_trip() {
    let g = TGraphMultiErrors::new(vec![], vec![], vec![], vec![], vec![], vec![]).named("empty");
    assert!(g.is_empty());
    let out = std::env::temp_dir().join("oxiroot_gme_empty.root");
    g.write_root(&out, Compression::None).expect("write");
    let back = TGraphMultiErrors::read_root(&RFile::open(&out).unwrap(), "empty").unwrap();
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}
