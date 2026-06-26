//! Histogram-analysis accessors checked against compiled ROOT C++ output on
//! `fixtures/analysis.root` (the `h` parabola, `g` its perturbed copy).

use std::path::PathBuf;

use oxiroot_hist::{read_th1d, TH1};
use oxiroot_io_core::RFile;

fn h(name: &str) -> TH1 {
    let f =
        RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/analysis.root"))
            .expect("open");
    read_th1d(&f, name).expect("read")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * (1.0 + b.abs())
}

#[test]
fn interpolate_matches_root() {
    let h = h("h");
    // ROOT TH1::Interpolate values.
    assert!(close(h.interpolate(5.0), 85.0));
    assert!(close(h.interpolate(5.5), 90.0));
    assert!(close(h.interpolate(10.3), 110.0));
    assert!(close(h.interpolate(0.7), 23.6));
    // At/beyond the first/last bin centers, clamp to the edge bin content.
    assert!(close(h.interpolate(0.0), 20.0)); // <= center(1)=0.5 -> content(1)
    assert!(close(h.interpolate(100.0), 20.0)); // >= center(20)=19.5 -> content(20)
}

#[test]
fn quantiles_match_root() {
    let h = h("h");
    let q = h.quantiles(&[0.1, 0.25, 0.5, 0.75, 0.9]);
    let want = [
        3.617647059,
        6.357142857,
        9.5, // exact cumulative tie -> bin center
        13.64285714,
        16.38235294,
    ];
    for (got, want) in q.iter().zip(want) {
        assert!(close(*got, want), "quantile got {got}, want {want}");
    }
}
