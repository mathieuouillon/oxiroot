//! M4 (chunk 1): serializing a `TH1D` reproduces ROOT's object bytes exactly.
//!
//! Since the histogram is obtained by reading the fixture and then written
//! back, equality with the fixture's object bytes proves a full byte-level
//! round-trip (write ∘ read == identity) against real ROOT output.

use std::path::PathBuf;

use root_hist::{read_th1d, th1d_to_bytes};
use root_io_core::RFile;

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

    let h = read_th1d(&f, "h1").expect("read TH1D");
    let written = th1d_to_bytes(&h);

    assert_eq!(written.len(), expected.len(), "serialized length differs");
    assert_eq!(
        written, expected,
        "serialized TH1D must match ROOT byte-for-byte"
    );
}

#[test]
fn writes_a_root_file_that_round_trips() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let h = read_th1d(&f, "h1").expect("read TH1D");

    // Write a complete .root file, then read it back with our own reader.
    let out = std::path::PathBuf::from("/tmp/rootrs_written_th1d.root");
    root_hist::write_th1d_file(&out, &h, 0).expect("write file");

    let f2 = RFile::open(&out).expect("reopen written file");
    let keys: Vec<(&str, &str)> = f2
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert_eq!(keys, vec![("h1", "TH1D")]);

    let h2 = read_th1d(&f2, "h1").expect("read back TH1D");
    assert_eq!(h2, h, "histogram must survive the write→read round-trip");
}

#[test]
fn writes_a_zstd_compressed_th1d() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let h = read_th1d(&f, "h1").expect("read TH1D");

    // Write the same histogram Zstd-compressed (505 = Zstd level 5).
    let out = std::path::PathBuf::from("/tmp/rootrs_written_th1d_zstd.root");
    root_hist::write_th1d_file(&out, &h, 505).expect("write compressed file");

    let f2 = RFile::open(&out).expect("reopen");
    let key = f2.key("h1").expect("h1 key");
    assert!(!key.is_uncompressed(), "object should be stored compressed");
    let h2 = read_th1d(&f2, "h1").expect("read back compressed TH1D");
    assert_eq!(h2, h, "compressed histogram must round-trip");
}
