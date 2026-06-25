//! Read a **zlib-compressed** (`ZL`) ROOT file. oxiroot writes only zstd/none,
//! so this is the one test that exercises the zlib *decode* path against a real
//! ROOT-written file (`fixtures/th1d_zlib.root`, from `scripts/gen_zlib_fixture.cpp`
//! — a 500-bin TH1D large enough that ROOT actually zlib-compresses the payload).

use std::path::PathBuf;

use oxiroot_hist::read_th1d;
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_zlib_compressed_th1d() {
    let f = RFile::open(fixture("th1d_zlib.root")).expect("open zlib fixture");

    // Guard: the file really is zlib (algorithm 1, level 5 → setting 105). If a
    // regeneration stored it uncompressed this would change and the test would
    // no longer be exercising the zlib decoder — fail loudly rather than silently.
    assert_eq!(
        f.header().compress,
        105,
        "fixture must be zlib(5)-compressed to exercise the decode path"
    );

    let h = read_th1d(&f, "h").expect("read zlib TH1D");
    assert_eq!(h.xaxis.nbins, 500);

    // Contents written by gen_zlib_fixture.cpp: bin i (1-based) = (i%7) + 0.5*(i%3).
    let values = h.values();
    assert_eq!(values.len(), 500);
    for (idx, &got) in values.iter().enumerate() {
        let i = idx + 1;
        let want = (i % 7) as f64 + 0.5 * (i % 3) as f64;
        assert!((got - want).abs() < 1e-9, "bin {i}: got {got}, want {want}");
    }
}
