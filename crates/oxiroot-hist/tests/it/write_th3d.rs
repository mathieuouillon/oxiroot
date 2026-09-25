//! #3: a `TH3D` survives write→read against real ROOT-produced data, and a
//! create→fill→save→read cycle round-trips through our own reader.
//!
//! Unlike the TH1D/TH2D fixtures (written by uproot, which we match
//! byte-for-byte), the TH3D fixture is written by C++ ROOT 6.38. The two
//! writers differ only in cosmetic/auxiliary fields — TObject `fBits` (ROOT
//! strips the memory-resident flags when writing to disk; uproot keeps them),
//! the `TAttMarker` class version, and a `TAttAxis` default — none of which
//! affect the data. So instead of byte-identity we assert that real
//! ROOT-produced data round-trips losslessly through our write→read.

use std::path::PathBuf;

use oxiroot_hist::{Hist, Hist3D, ReadRoot, WriteRoot};
use oxiroot_io_core::FileReader;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn round_trips_real_root_th3d() {
    // Read C++ ROOT-produced data, write it back out, and read it again: the
    // histogram must be identical, proving write∘read == identity on real data.
    let f = FileReader::open(fixture("th3d_uncompressed.root")).expect("open fixture");
    let h = Hist3D::read_root(&f, "h3").expect("read TH3D");

    let out = PathBuf::from("/tmp/rootrs_roundtrip_th3d.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f2 = FileReader::open(&out).expect("reopen");
    let h2 = Hist3D::read_root(&f2, "h3").expect("read back");
    assert_eq!(h2, h, "real ROOT TH3D must survive write→read");
}

#[test]
fn create_fill_save_round_trips() {
    let mut h = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("h3")
        .titled("filled");
    h.fill(0.5, 0.5, 0.5); // (1,1,1)
    h.fill(0.5, 0.5, 0.5);
    h.fill(1.5, 1.5, 1.5); // (2,2,2)
    h.fill(9.0, 0.5, 0.5); // x overflow -> excluded from stats

    assert_eq!(h.entries, 4.0, "all fills counted");
    assert_eq!(h.tsumw, 3.0, "only in-range fills in stats");
    assert_eq!(h.values()[0][0][0], 2.0, "cell (1,1,1)");
    assert_eq!(h.values()[1][1][1], 1.0, "cell (2,2,2)");
    assert!((h.mean_x() - 2.5 / 3.0).abs() < 1e-12, "mean x");
    assert!((h.mean_z() - 2.5 / 3.0).abs() < 1e-12, "mean z");

    let out = PathBuf::from("/tmp/rootrs_filled_th3d.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = FileReader::open(&out).expect("reopen");
    let h2 = Hist3D::read_root(&f, "h3").expect("read back");
    assert_eq!(h2, h, "filled 3-D histogram must round-trip");
}

#[test]
fn writes_a_zstd_compressed_th3d() {
    let f = FileReader::open(fixture("th3d_uncompressed.root")).expect("open fixture");
    let h = Hist3D::read_root(&f, "h3").expect("read TH3D");

    let out = PathBuf::from("/tmp/rootrs_written_th3d_zstd.root");
    h.write_root(&out, oxiroot_io_core::Compression::Zstd(5))
        .expect("write compressed file");

    let f2 = FileReader::open(&out).expect("reopen");
    let key = f2.key("h3").expect("h3 key");
    assert!(!key.is_uncompressed(), "object should be stored compressed");
    let h2 = Hist3D::read_root(&f2, "h3").expect("read back compressed TH3D");
    assert_eq!(h2, h, "compressed 3-D histogram must round-trip");
}
