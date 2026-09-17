//! TGraph2D: read a ROOT fixture, self round-trip, and build from scratch.
//! (Cross-checked against compiled ROOT C++ and uproot, which read the
//! oxiroot-written TGraph2D with the right class and values.)

use std::path::PathBuf;

use oxiroot_hist::{Compression, ReadRoot, TGraph2D, WriteRoot};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_root_written_graph2d() {
    let f = RFile::open(fixture("graph2d.root")).expect("open");
    let g = TGraph2D::read_root(&f, "g2d").expect("read g2d");
    assert_eq!(g.name, "g2d");
    assert_eq!(g.title, "surface fixture");
    assert_eq!(g.x, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(g.y, vec![10.0, 20.0, 30.0, 40.0]);
    assert_eq!(g.z, vec![100.0, 200.0, 300.0, 400.0]);
}

#[test]
fn graph2d_round_trip_from_fixture() {
    let f = RFile::open(fixture("graph2d.root")).expect("open");
    let g = TGraph2D::read_root(&f, "g2d").unwrap();
    let out = std::env::temp_dir().join("oxiroot_g2d_rt.root");
    g.write_root(&out, Compression::None).expect("write");
    let back = TGraph2D::read_root(&RFile::open(&out).unwrap(), "g2d").unwrap();
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn graph2d_build_from_scratch() {
    let g = TGraph2D::new(
        vec![0.5, 1.5, 2.5],
        vec![-1.0, 0.0, 1.0],
        vec![3.0, 6.0, 9.0],
    )
    .named("scratch")
    .titled("built in Rust");
    let out = std::env::temp_dir().join("oxiroot_g2d_scratch.root");
    g.write_root(&out, Compression::Zstd(3)).expect("write");
    let back = TGraph2D::read_root(&RFile::open(&out).unwrap(), "scratch").unwrap();
    assert_eq!(back, g);
    assert_eq!(back.len(), 3);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn empty_graph2d_round_trip() {
    let g = TGraph2D::new(vec![], vec![], vec![]).named("empty");
    assert!(g.is_empty());
    let out = std::env::temp_dir().join("oxiroot_g2d_empty.root");
    g.write_root(&out, Compression::None).expect("write");
    let back = TGraph2D::read_root(&RFile::open(&out).unwrap(), "empty").unwrap();
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}
