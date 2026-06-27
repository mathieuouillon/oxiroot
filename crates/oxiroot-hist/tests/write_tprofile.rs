//! #3: a `TProfile` survives write→read against real ROOT-produced data, and a
//! create→fill→save→read cycle round-trips through our own reader.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TProfile, WriteRoot};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn round_trips_real_root_tprofile() {
    let f = RFile::open(fixture("tprofile_uncompressed.root")).expect("open fixture");
    let h = TProfile::read_root(&f, "p").expect("read TProfile");

    let out = PathBuf::from("/tmp/rootrs_roundtrip_tprofile.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f2 = RFile::open(&out).expect("reopen");
    let h2 = TProfile::read_root(&f2, "p").expect("read back");
    assert_eq!(h2, h, "real ROOT TProfile must survive write→read");
}

#[test]
fn create_fill_save_round_trips() {
    // Reproduce the fixture's fills: 4 bins over [0,4], profiling y vs x.
    let mut h = TProfile::new(4, 0.0, 4.0).named("p").titled("profile");
    h.fill(0.5, 20.0);
    h.fill(0.5, 10.0);
    h.fill(1.5, 10.0);
    h.fill(1.5, 5.0);
    h.fill(2.5, 30.0);

    assert_eq!(h.entries, 5.0);
    assert_eq!(h.tsumw, 5.0);
    assert_eq!(h.tsumwy, 75.0, "sum of y");
    assert_eq!(h.tsumwy2, 1525.0, "sum of y^2");
    // values() = mean y per bin: bin1 = (20+10)/2 = 15, bin2 = (10+5)/2 = 7.5, bin3 = 30.
    assert_eq!(h.values(), [15.0, 7.5, 30.0, 0.0]);
    assert_eq!(h.bin_entries[1..4], [2.0, 2.0, 1.0]);

    let out = PathBuf::from("/tmp/rootrs_filled_tprofile.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    let h2 = TProfile::read_root(&f, "p").expect("read back");
    assert_eq!(h2, h, "filled profile must round-trip");
}

#[test]
fn writes_a_zstd_compressed_tprofile() {
    let f = RFile::open(fixture("tprofile_uncompressed.root")).expect("open fixture");
    let h = TProfile::read_root(&f, "p").expect("read TProfile");

    let out = PathBuf::from("/tmp/rootrs_written_tprofile_zstd.root");
    h.write_root(&out, oxiroot_io_core::Compression::Zstd(5))
        .expect("write compressed file");

    let f2 = RFile::open(&out).expect("reopen");
    let key = f2.key("p").expect("p key");
    assert!(!key.is_uncompressed(), "object should be stored compressed");
    let h2 = TProfile::read_root(&f2, "p").expect("read back compressed TProfile");
    assert_eq!(h2, h, "compressed profile must round-trip");
}
