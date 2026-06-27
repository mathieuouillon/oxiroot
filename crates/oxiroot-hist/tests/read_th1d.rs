//! M2 integration test: reconstruct a `TH1D` from real ROOT fixtures (both
//! uncompressed and Zstd) and check it against the committed golden values.

use std::path::PathBuf;

use oxiroot_hist::read_th1d;
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// Bin contents (no flow) from fixtures/golden/th1d_*.json.
const GOLDEN_VALUES: [f64; 17] = [
    2.0, 5.0, 11.0, 18.0, 30.0, 44.0, 60.0, 70.0, 72.0, 68.0, 55.0, 40.0, 28.0, 16.0, 9.0, 4.0, 1.0,
];
const GOLDEN_ENTRIES: f64 = 533.0;

fn check_h1(file: &str) {
    let f = RFile::open(fixture(file)).expect("open fixture");
    let h = read_th1d(&f, "h1").expect("read TH1D");

    assert_eq!(h.name, "h1");
    assert_eq!(h.title, "");

    // Axis: 17 uniform bins over [-4, 4].
    assert_eq!(h.xaxis.nbins, 17);
    assert_eq!(h.xaxis.xmin, -4.0);
    assert_eq!(h.xaxis.xmax, 4.0);
    assert!(h.xaxis.xbins.is_empty(), "axis should be uniform");
    assert_eq!(h.ncells(), 19); // 17 + under/overflow

    // Bin contents (excluding flow) match golden exactly (integers as f64).
    assert_eq!(h.values(), GOLDEN_VALUES, "{file}: bin contents");

    // Under/overflow are empty for this histogram.
    assert_eq!(h.contents.first(), Some(&0.0));
    assert_eq!(h.contents.last(), Some(&0.0));

    assert_eq!(h.entries, GOLDEN_ENTRIES, "{file}: entries");
    assert_eq!(h.tsumw, GOLDEN_ENTRIES);

    // Edges: 18 values, uniform from -4 to 4.
    let edges = h.edges();
    assert_eq!(edges.len(), 18);
    for (i, e) in edges.iter().enumerate() {
        let expected = -4.0 + 8.0 * i as f64 / 17.0;
        assert!(
            (e - expected).abs() < 1e-12,
            "{file}: edge {i} = {e}, expected {expected}"
        );
    }
}

#[test]
fn reads_th1d_uncompressed() {
    check_h1("th1d_uncompressed.root");
}

#[test]
fn reads_th1d_zstd() {
    check_h1("th1d_zstd.root");
}

#[test]
fn zstd_and_uncompressed_agree() {
    let a = read_th1d(
        &RFile::open(fixture("th1d_uncompressed.root")).unwrap(),
        "h1",
    )
    .unwrap();
    let b = read_th1d(&RFile::open(fixture("th1d_zstd.root")).unwrap(), "h1").unwrap();
    assert_eq!(
        a, b,
        "compressed and uncompressed histograms must be identical"
    );
}
