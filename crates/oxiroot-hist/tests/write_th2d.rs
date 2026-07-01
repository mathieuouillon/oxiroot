//! #3: serializing a `TH2D` reproduces ROOT's object bytes exactly, and a
//! create→fill→save→read cycle round-trips through our own reader.

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, WriteRoot, TH2};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn serializes_th2d_byte_identical_to_root() {
    let f = RFile::open(fixture("th2d_uncompressed.root")).expect("open fixture");
    let key = f.key("h2").expect("h2 key");
    assert!(key.is_uncompressed());
    let expected: &[u8] = &f.data()[key.payload_range()];

    let h = TH2::read_root(&f, "h2").expect("read TH2D");
    let written = h.to_root_bytes();

    assert_eq!(written.len(), expected.len(), "serialized length differs");
    assert_eq!(
        written, expected,
        "serialized TH2D must match ROOT byte-for-byte"
    );
}

#[test]
fn create_fill_save_round_trips() {
    // Build a 3x2 histogram and fill it, as in a 2-D analysis loop.
    let mut h = Hist::reg(3, 0.0, 3.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("h2")
        .titled("filled");
    h.fill(0.5, 0.5); // (binx=1, biny=1)
    h.fill(0.5, 0.5);
    h.fill(2.5, 1.5); // (binx=3, biny=2)
    h.fill(-1.0, 0.5); // x underflow -> excluded from stats
    h.fill(0.5, 9.0); // y overflow -> excluded from stats

    assert_eq!(h.entries, 5.0, "all fills counted");
    assert_eq!(h.tsumw, 3.0, "only in-range fills in stats");
    assert_eq!(h.values(), [[2.0, 0.0], [0.0, 0.0], [0.0, 1.0]], "values");
    // mean_x = (0.5+0.5+2.5)/3, mean_y = (0.5+0.5+1.5)/3
    assert!((h.mean_x() - 3.5 / 3.0).abs() < 1e-12, "mean x");
    assert!((h.mean_y() - 2.5 / 3.0).abs() < 1e-12, "mean y");

    let out = PathBuf::from("/tmp/rootrs_filled_th2d.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    let h2 = TH2::read_root(&f, "h2").expect("read back");
    assert_eq!(h2, h, "filled 2-D histogram must round-trip");
}

#[test]
fn writes_a_zstd_compressed_th2d() {
    let f = RFile::open(fixture("th2d_uncompressed.root")).expect("open fixture");
    let h = TH2::read_root(&f, "h2").expect("read TH2D");

    let out = PathBuf::from("/tmp/rootrs_written_th2d_zstd.root");
    h.write_root(&out, oxiroot_io_core::Compression::Zstd(5))
        .expect("write compressed file");

    let f2 = RFile::open(&out).expect("reopen");
    let key = f2.key("h2").expect("h2 key");
    assert!(!key.is_uncompressed(), "object should be stored compressed");
    let h2 = TH2::read_root(&f2, "h2").expect("read back compressed TH2D");
    assert_eq!(h2, h, "compressed 2-D histogram must round-trip");
}
