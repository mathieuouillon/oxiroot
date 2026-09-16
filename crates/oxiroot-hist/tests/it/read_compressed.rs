//! Decode every compressed ROOT block format against real ROOT output.
//!
//! oxiroot encodes zstd/zlib/lz4 and decodes those plus LZMA. The committed
//! fixtures `fixtures/th1d_{zlib,lz4,lzma}.root` (from
//! `scripts/gen_compressed_fixtures.cpp`) are the same 500-bin `TH1D` stored
//! with each algorithm, so they pin the zlib, LZ4, and LZMA *decode* paths to
//! genuine ROOT bytes. (zstd decode is exercised throughout the other tests.)

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TH1};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// `(fixture, ROOT compression setting algorithm*100+level)`.
const CASES: &[(&str, u32)] = &[
    ("th1d_zlib.root", 105), // zlib level 5
    ("th1d_lz4.root", 404),  // LZ4 level 4
    ("th1d_lzma.root", 205), // LZMA level 5
];

#[test]
fn decodes_every_compressed_block_format() {
    for &(name, setting) in CASES {
        let f = RFile::open(fixture(name)).unwrap_or_else(|e| panic!("open {name}: {e}"));

        // Guard: confirm the fixture really uses this algorithm, so a regeneration
        // that stored it differently fails loudly instead of silently skipping
        // the decoder under test.
        assert_eq!(
            f.header().compress,
            setting,
            "{name} must be compressed with setting {setting}"
        );

        let h = TH1::read_root(&f, "h").unwrap_or_else(|e| panic!("read {name}: {e}"));
        assert_eq!(h.xaxis.nbins, 500);

        // Content written by gen_compressed_fixtures.cpp: bin i = (i%7) + 0.5*(i%3).
        let values = h.values();
        assert_eq!(values.len(), 500);
        for (idx, &got) in values.iter().enumerate() {
            let i = idx + 1;
            let want = (i % 7) as f64 + 0.5 * (i % 3) as f64;
            assert!(
                (got - want).abs() < 1e-9,
                "{name} bin {i}: got {got}, want {want}"
            );
        }
    }
}
