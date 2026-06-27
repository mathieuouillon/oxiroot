//! TProfile3D: read a ROOT-written fixture (`p3` in tprofile2d.root) + round-trip.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TProfile3D, WriteRoot};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_root_written_tprofile3d() {
    let f = RFile::open(fixture("tprofile2d.root")).expect("open");
    assert_eq!(f.key("p3").unwrap().class_name, "TProfile3D");
    let p = TProfile3D::read_root(&f, "p3").expect("read TProfile3D");
    // cell(1,1,1) mean t = 15; cell(2,2,2) mean t = 7.
    assert_eq!(p.values()[0][0][0], 15.0);
    assert_eq!(p.values()[1][1][1], 7.0);
    assert_eq!(p.entries, 3.0);
}

#[test]
fn tprofile3d_round_trips() {
    let mut p = TProfile3D::new("p3", "", 2, 0.0, 2.0, 2, 0.0, 2.0, 2, 0.0, 2.0);
    p.fill(0.5, 0.5, 0.5, 10.0);
    p.fill(0.5, 0.5, 0.5, 20.0); // cell(1,1,1) mean 15
    p.fill(1.5, 1.5, 1.5, 7.0); // cell(2,2,2) mean 7
    assert_eq!(p.values()[0][0][0], 15.0);
    assert_eq!(p.values()[1][1][1], 7.0);

    let out = PathBuf::from("/tmp/oxiroot_tprofile3d.root");
    p.write_root(&out, Compression::None).expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(TProfile3D::read_root(&f, "p3").unwrap(), p, "round-trips");
}
