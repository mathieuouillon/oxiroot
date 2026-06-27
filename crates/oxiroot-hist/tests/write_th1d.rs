//! M4 (chunk 1): serializing a `TH1D` reproduces ROOT's object bytes exactly.
//!
//! Since the histogram is obtained by reading the fixture and then written
//! back, equality with the fixture's object bytes proves a full byte-level
//! round-trip (write ∘ read == identity) against real ROOT output.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, WriteRoot, TH1};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn serializes_th1d_byte_identical_to_root() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let key = f.key("h1").expect("h1 key");
    assert!(key.is_uncompressed());
    let expected: &[u8] = &f.data()[key.payload_range()];

    let h = TH1::read_root(&f, "h1").expect("read TH1D");
    let written = h.to_root_bytes();

    assert_eq!(written.len(), expected.len(), "serialized length differs");
    assert_eq!(
        written, expected,
        "serialized TH1D must match ROOT byte-for-byte"
    );
}

#[test]
fn writes_a_root_file_that_round_trips() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let h = TH1::read_root(&f, "h1").expect("read TH1D");

    // Write a complete .root file, then read it back with our own reader.
    let out = std::path::PathBuf::from("/tmp/rootrs_written_th1d.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write file");

    let f2 = RFile::open(&out).expect("reopen written file");
    let keys: Vec<(&str, &str)> = f2
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert_eq!(keys, vec![("h1", "TH1D")]);

    let h2 = TH1::read_root(&f2, "h1").expect("read back TH1D");
    assert_eq!(h2, h, "histogram must survive the write→read round-trip");
}

#[test]
fn create_fill_save_round_trips() {
    // Build a histogram from scratch and fill it, as in an analysis loop.
    let mut h = TH1::new(5, 0.0, 5.0).named("h").titled("filled");
    h.fill(0.5);
    h.fill(1.5);
    h.fill(1.5);
    h.fill(2.5);
    h.fill(2.5);
    h.fill(2.5);
    h.fill(-1.0); // underflow
    h.fill(10.0); // overflow

    assert_eq!(h.entries, 8.0, "all fills counted");
    assert_eq!(
        h.values(),
        [1.0, 2.0, 3.0, 0.0, 0.0],
        "in-range bin contents"
    );
    assert_eq!(h.contents[0], 1.0, "underflow");
    assert_eq!(*h.contents.last().unwrap(), 1.0, "overflow");
    assert_eq!(h.tsumw, 6.0, "in-range weight sum");
    assert!((h.mean() - 11.0 / 6.0).abs() < 1e-12, "mean = tsumwx/tsumw");

    // Save and read back through our own reader.
    let out = std::path::PathBuf::from("/tmp/rootrs_filled_th1d.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    let h2 = TH1::read_root(&f, "h").expect("read back");
    assert_eq!(h2, h, "filled histogram must round-trip");
}

#[test]
fn written_file_embeds_self_describing_streamer_info() {
    // A written file carries a TStreamerInfo list covering the histogram
    // hierarchy at the exact versions we emit, so any ROOT reader can read it.
    let mut h = TH1::new(5, 0.0, 5.0).named("h");
    h.fill(2.5);
    let out = std::path::PathBuf::from("/tmp/rootrs_streamerinfo_th1d.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");
    let reg = f.streamer_registry().expect("parse embedded streamer info");
    let classes = reg.class_names();
    for expected in ["TH1D", "TH2D", "TH3D", "TProfile", "TH1", "TAxis", "TNamed"] {
        assert!(
            classes.contains(&expected),
            "missing streamer for {expected}"
        );
    }
    // Versions must match what our serializer writes.
    assert_eq!(reg.get("TH1").unwrap().class_version, 8);
    assert_eq!(reg.get("TH1D").unwrap().class_version, 3);
    assert_eq!(reg.get("TAxis").unwrap().class_version, 10);
    assert_eq!(reg.get("TProfile").unwrap().class_version, 7);
}

#[test]
fn writes_a_zstd_compressed_th1d() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let h = TH1::read_root(&f, "h1").expect("read TH1D");

    // Write the same histogram Zstd-compressed (505 = Zstd level 5).
    let out = std::path::PathBuf::from("/tmp/rootrs_written_th1d_zstd.root");
    h.write_root(&out, oxiroot_io_core::Compression::Zstd(5))
        .expect("write compressed file");

    let f2 = RFile::open(&out).expect("reopen");
    let key = f2.key("h1").expect("h1 key");
    assert!(!key.is_uncompressed(), "object should be stored compressed");
    let h2 = TH1::read_root(&f2, "h1").expect("read back compressed TH1D");
    assert_eq!(h2, h, "compressed histogram must round-trip");
}

/// A never-filled histogram (all-zero contents, no entries) must still write a
/// valid file and round-trip — the empty-histogram boundary.
#[test]
fn empty_th1d_round_trips() {
    let h = TH1::new(10, 0.0, 1.0).named("empty").titled("never filled");
    let out = std::path::PathBuf::from("/tmp/oxiroot_empty_th1d.root");
    h.write_root(&out, Compression::None).expect("write");
    let back = TH1::read_root(&RFile::open(&out).unwrap(), "empty").expect("read");
    assert_eq!(back.contents.len(), 12); // nbins + 2 flow cells
    assert!(back.contents.iter().all(|&c| c == 0.0));
    assert_eq!(back.entries, 0.0);
}
