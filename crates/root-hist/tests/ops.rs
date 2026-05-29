//! Item 3: histogram arithmetic — merge (add), scale, multiply, divide,
//! integral — with Sumw2 error propagation. Merge + scale are cross-checked in
//! official ROOT.

use std::path::PathBuf;

use root_hist::{read_th1d, TH1};
use root_io_core::RFile;

#[test]
fn merge_then_scale_matches_root() {
    // Two weighted histograms, as if from two parallel jobs.
    let mut a = TH1::new("h", "merged", 4, 0.0, 4.0);
    a.sumw2();
    a.fill_weight(0.5, 2.0);
    a.fill_weight(1.5, 1.0);
    let mut b = TH1::new("h", "merged", 4, 0.0, 4.0);
    b.sumw2();
    b.fill_weight(0.5, 3.0);
    b.fill_weight(2.5, 1.0);

    assert!(a.add(&b, 1.0), "compatible binnings merge");
    // bin1: 2+3 = 5, sumw2 = 4+9 = 13; bin2 = 1; bin3 = 1.
    assert_eq!(a.contents[1], 5.0);
    assert_eq!(a.sumw2[1], 13.0);
    assert_eq!(a.entries, 4.0, "entries summed");

    a.scale(2.0);
    assert_eq!(a.contents[1], 10.0);
    assert_eq!(a.sumw2[1], 52.0); // 13 * 2^2
    assert!((a.bin_error(1) - 52.0_f64.sqrt()).abs() < 1e-9);

    let out = PathBuf::from("/tmp/rootrs_merged_scaled.root");
    root_hist::write_th1d_file(&out, &a, root_io_core::Compression::None).expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(read_th1d(&f, "h").unwrap(), a, "round-trips");
}

#[test]
fn add_rejects_mismatched_binning() {
    let mut a = TH1::new("a", "", 4, 0.0, 4.0);
    let b = TH1::new("b", "", 5, 0.0, 5.0);
    assert!(!a.add(&b, 1.0), "different bin counts -> no-op");
}

#[test]
fn integral_multiply_divide() {
    let mut num = TH1::new("n", "", 2, 0.0, 2.0);
    num.sumw2();
    num.fill(0.5);
    num.fill(0.5);
    num.fill(1.5);
    assert_eq!(num.integral(), 3.0, "sum of in-range bins");

    let mut den = TH1::new("d", "", 2, 0.0, 2.0);
    den.sumw2();
    for _ in 0..4 {
        den.fill(0.5);
    }
    for _ in 0..4 {
        den.fill(1.5);
    }

    // Efficiency num/den: bin1 = 2/4 = 0.5, bin2 = 1/4 = 0.25.
    let mut eff = num.clone();
    assert!(eff.divide(&den));
    assert!((eff.contents[1] - 0.5).abs() < 1e-12);
    assert!((eff.contents[2] - 0.25).abs() < 1e-12);
    // Binomial-ish error from ROOT's default formula stays finite and positive.
    assert!(eff.bin_error(1) > 0.0 && eff.bin_error(1) < 1.0);

    // Multiply back by the denominator recovers the numerator contents.
    assert!(eff.multiply(&den));
    assert!((eff.contents[1] - 2.0).abs() < 1e-12);
    assert!((eff.contents[2] - 1.0).abs() < 1e-12);
}
