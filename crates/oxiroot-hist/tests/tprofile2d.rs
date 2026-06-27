//! TProfile2D: read a ROOT-written fixture, and self-round-trip a written one.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TProfile2D, WriteRoot};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_root_written_tprofile2d() {
    // fixtures/tprofile2d.root p2: bin(1,1) mean 15, (2,1) mean 5, (2,2) mean 30.
    let f = RFile::open(fixture("tprofile2d.root")).expect("open");
    assert_eq!(f.key("p2").unwrap().class_name, "TProfile2D");
    let p = TProfile2D::read_root(&f, "p2").expect("read TProfile2D");
    assert_eq!(p.values(), vec![vec![15.0, 0.0], vec![5.0, 30.0]]);
    assert_eq!(p.entries, 4.0);
}

#[test]
fn tprofile2d_round_trips() {
    let mut p = TProfile2D::new(2, 0.0, 2.0, 2, 0.0, 2.0)
        .named("p2")
        .titled("prof");
    p.fill(0.5, 0.5, 10.0);
    p.fill(0.5, 0.5, 20.0); // bin(1,1) mean 15
    p.fill(1.5, 0.5, 5.0); // bin(2,1) mean 5
    p.fill(1.5, 1.5, 30.0); // bin(2,2) mean 30
    assert_eq!(p.values(), vec![vec![15.0, 0.0], vec![5.0, 30.0]]);

    let out = PathBuf::from("/tmp/oxiroot_tprofile2d.root");
    p.write_root(&out, Compression::None).expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(TProfile2D::read_root(&f, "p2").unwrap(), p, "round-trips");
}
