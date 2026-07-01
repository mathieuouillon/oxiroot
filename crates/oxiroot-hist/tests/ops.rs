//! Item 3: histogram arithmetic — merge (add), scale, multiply, divide,
//! integral — with Sumw2 error propagation. Merge + scale are cross-checked in
//! official ROOT.

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, WriteRoot, TH1};
use oxiroot_io_core::RFile;

#[test]
fn merge_then_scale_matches_root() {
    // Two weighted histograms, as if from two parallel jobs.
    let mut a = Hist::reg(4, 0.0, 4.0).double().named("h").titled("merged");
    a.sumw2();
    a.fill_weight(0.5, 2.0);
    a.fill_weight(1.5, 1.0);
    let mut b = Hist::reg(4, 0.0, 4.0).double().named("h").titled("merged");
    b.sumw2();
    b.fill_weight(0.5, 3.0);
    b.fill_weight(2.5, 1.0);

    a.add(&b, 1.0).expect("compatible binnings merge");
    // bin1: 2+3 = 5, sumw2 = 4+9 = 13; bin2 = 1; bin3 = 1.
    assert_eq!(a.contents[1], 5.0);
    assert_eq!(a.sumw2[1], 13.0);
    assert_eq!(a.entries, 4.0, "entries summed");

    a.scale(2.0);
    assert_eq!(a.contents[1], 10.0);
    assert_eq!(a.sumw2[1], 52.0); // 13 * 2^2
    assert!((a.bin_error(1) - 52.0_f64.sqrt()).abs() < 1e-9);

    let out = PathBuf::from("/tmp/rootrs_merged_scaled.root");
    a.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(TH1::read_root(&f, "h").unwrap(), a, "round-trips");
}

#[test]
fn add_rejects_mismatched_binning() {
    // Different bin counts.
    let mut a = Hist::reg(4, 0.0, 4.0).double().named("a");
    let b = Hist::reg(5, 0.0, 5.0).double().named("b");
    assert!(a.add(&b, 1.0).is_err(), "different bin counts -> error");

    // Same bin count but different range — must also be rejected (cell-count
    // equality alone would have missed this).
    let mut c = Hist::reg(4, 0.0, 4.0).double().named("c");
    let d = Hist::reg(4, 0.0, 8.0).double().named("d");
    assert!(c.add(&d, 1.0).is_err(), "different edges -> error");
    assert_eq!(c.contents[1], 0.0, "rejected add makes no change");
}

#[test]
fn integral_multiply_divide() {
    let mut num = Hist::reg(2, 0.0, 2.0).double().named("n");
    num.sumw2();
    num.fill(0.5);
    num.fill(0.5);
    num.fill(1.5);
    assert_eq!(num.integral(), 3.0, "sum of in-range bins");

    let mut den = Hist::reg(2, 0.0, 2.0).double().named("d");
    den.sumw2();
    for _ in 0..4 {
        den.fill(0.5);
    }
    for _ in 0..4 {
        den.fill(1.5);
    }

    // Efficiency num/den: bin1 = 2/4 = 0.5, bin2 = 1/4 = 0.25.
    let mut eff = num.clone();
    eff.divide(&den).expect("compatible binnings");
    assert!((eff.contents[1] - 0.5).abs() < 1e-12);
    assert!((eff.contents[2] - 0.25).abs() < 1e-12);
    // Binomial-ish error from ROOT's default formula stays finite and positive.
    assert!(eff.bin_error(1) > 0.0 && eff.bin_error(1) < 1.0);

    // Multiply back by the denominator recovers the numerator contents.
    eff.multiply(&den).expect("compatible binnings");
    assert!((eff.contents[1] - 2.0).abs() < 1e-12);
    assert!((eff.contents[2] - 1.0).abs() < 1e-12);
}

#[test]
fn find_bin_routes_nan_and_top_edge() {
    let h = Hist::reg(4, 0.0, 4.0).double().named("h");
    assert_eq!(h.xaxis.find_bin(-0.1), 0, "below range -> underflow");
    assert_eq!(h.xaxis.find_bin(0.0), 1, "low edge -> first bin");
    assert_eq!(h.xaxis.find_bin(3.999), 4, "just under top -> last bin");
    assert_eq!(h.xaxis.find_bin(4.0), 5, "top edge -> overflow");
    assert_eq!(h.xaxis.find_bin(f64::NAN), 5, "NaN -> overflow (ROOT)");
    // A NaN fill must not corrupt an in-range bin.
    let mut h2 = h.clone();
    h2.fill(f64::NAN);
    assert_eq!(
        &h2.values(),
        &[0.0, 0.0, 0.0, 0.0],
        "NaN stays out of range"
    );
}

#[test]
fn tprofile_merge_and_bin_error_match_root() {
    // Two profiles of the same (x,y) stream, split across "jobs", then merged.
    let mut a = Hist::reg(2, 0.0, 2.0).profile().named("p");
    a.fill(0.5, 1.0);
    a.fill(0.5, 3.0);
    let mut b = Hist::reg(2, 0.0, 2.0).profile().named("p");
    b.fill(0.5, 5.0);
    b.fill(1.5, 10.0);

    a.add(&b, 1.0).expect("compatible binnings merge");
    // Bin 1 saw y = {1,3,5}: mean = 3, entries = 3.
    assert_eq!(a.bin_entries[1], 3.0);
    assert!((a.values()[0] - 3.0).abs() < 1e-12, "profiled mean = 3");
    // ROOT default (kERRORMEAN) error on the mean: spread² = <y²>-<y>² =
    // 35/3 - 9 = 8/3; neff = 3; error = sqrt((8/3)/3) = sqrt(8)/3.
    let expected = (8.0_f64 / 3.0 / 3.0).sqrt();
    assert!(
        (a.bin_error(1) - expected).abs() < 1e-12,
        "got {}, want {expected}",
        a.bin_error(1)
    );
    // Bin 2 saw a single y = 10: zero spread, zero error on the mean.
    assert!((a.values()[1] - 10.0).abs() < 1e-12);
    assert_eq!(a.bin_error(2), 0.0, "single entry -> no spread");

    let mut wrong = Hist::reg(3, 0.0, 3.0).profile().named("p");
    assert!(wrong.add(&a, 1.0).is_err(), "different binning rejected");
}
