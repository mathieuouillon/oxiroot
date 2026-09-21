//! A weighted fill must record `Σw²`, or every bin error assumes unit weights.
//!
//! ROOT's `Fill` switches on per-bin `Σw²` tracking by itself the first time it
//! sees a weight other than 1. oxiroot used to track it only when the caller had
//! asked up front (`Hist::…weight()` / `sumw2()`), so a weighted histogram or
//! profile built without that call silently reported unit-weight errors.

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, TProfile, WriteRoot};
use oxiroot_io_core::{Compression, FileReader};

// ---------------------------------------------------------------- histograms

#[test]
fn th1_weighted_fill_tracks_squared_weights() {
    let mut h = Hist::reg(4, 0.0, 4.0).double(); // no `.weight()`
    assert!(h.sumw2.is_empty());
    h.fill_weight(0.5, 2.0);
    // One entry of weight 2: error = √Σw² = 2, not √Σw = √2.
    assert!(!h.sumw2.is_empty(), "a weighted fill must turn tracking on");
    assert_eq!(h.bin_error(1), 2.0);
}

#[test]
fn th1_tracking_seeds_from_earlier_unit_fills() {
    let mut h = Hist::reg(4, 0.0, 4.0).double();
    h.fill(0.5); // unit weight, no tracking yet
    h.fill_weight(0.5, 3.0); // turns tracking on; the earlier fill contributes 1²
    assert_eq!(h.sumw2[1], 1.0 + 9.0);
    assert_eq!(h.bin_error(1), 10.0_f64.sqrt());
}

#[test]
fn unit_weight_fills_still_do_not_allocate() {
    let mut h = Hist::reg(4, 0.0, 4.0).double();
    h.fill(0.5);
    h.fill_weight(1.5, 1.0);
    assert!(h.sumw2.is_empty(), "unit weights need no Σw² array");
    assert_eq!(h.bin_error(1), 1.0);
}

#[test]
fn th2_and_th3_weighted_fills_track_squared_weights() {
    let mut h2 = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).double();
    h2.fill_weight(0.5, 0.5, 2.0);
    let bin2 = 1 + (2 + 2); // bx + (nx + 2)·by
    assert_eq!(h2.bin_error(bin2), 2.0);

    let mut h3 = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double();
    h3.fill_weight(0.5, 0.5, 0.5, 2.0);
    let bin3 = 1 + 4 * (1 + 4); // bx + (nx + 2)·(by + (ny + 2)·bz)
    assert_eq!(h3.bin_error(bin3), 2.0);
}

// ------------------------------------------------------------------ profiles

#[test]
fn tprofile_weighted_fill_counts_effective_entries() {
    // The original report: one fill of weight 2 has Σw = 2 and Σw² = 4, so
    // (Σw)²/Σw² = 1 effective entry. Without Σw² tracking it read 2.
    let mut p = Hist::reg(4, 0.0, 4.0).profile();
    p.fill_weight(0.5, 1.0, 2.0);
    assert_eq!(p.bin_sumw2[1], 4.0);
    assert_eq!(p.effective_entries(1), 1.0);
}

#[test]
fn tprofile_weighted_errors_use_squared_weights() {
    // Two weight-2 fills at y = 1 and y = 3 in one bin:
    //   mean = 2, spread² = 1, Σw = 4, Σw² = 8, neff = 16/8 = 2
    //   error on the mean = √(1/2); assuming unit weights gave √(1/4) = 0.5.
    let mut p = Hist::reg(4, 0.0, 4.0).profile();
    p.fill_weight(0.5, 1.0, 2.0);
    p.fill_weight(0.5, 3.0, 2.0);
    let err = p.bin_error(1);
    assert!((err - 0.5_f64.sqrt()).abs() < 1e-12, "got {err}");
}

#[test]
fn tprofile_tracking_seeds_from_earlier_unit_fills() {
    let mut p = Hist::reg(4, 0.0, 4.0).profile();
    p.fill(0.5, 1.0); // Σw = 1, Σw² = 1, not yet tracked
    p.fill_weight(0.5, 1.0, 2.0); // turns tracking on
    assert_eq!(p.bin_entries[1], 3.0);
    assert_eq!(p.bin_sumw2[1], 1.0 + 4.0);
    assert_eq!(p.effective_entries(1), 9.0 / 5.0);
    // Later unit fills keep accumulating into the tracked array.
    p.fill(0.5, 1.0);
    assert_eq!(p.bin_sumw2[1], 6.0);
}

#[test]
fn tprofile_rejected_fill_does_not_turn_tracking_on() {
    // A y-range rejects the point before anything is counted, as in ROOT.
    let mut p = Hist::reg(4, 0.0, 4.0).profile();
    p.ymin = 0.0;
    p.ymax = 10.0;
    p.fill_weight(0.5, 50.0, 2.0);
    assert!(p.bin_sumw2.is_empty());
    assert_eq!(p.entries, 0.0);
}

#[test]
fn tprofile_unit_fills_do_not_allocate() {
    let mut p = Hist::reg(4, 0.0, 4.0).profile();
    p.fill(0.5, 1.0);
    p.fill_weight(0.5, 2.0, 1.0);
    assert!(p.bin_sumw2.is_empty());
    assert_eq!(p.effective_entries(1), 2.0);
}

#[test]
fn tprofile2d_weighted_fill_tracks_squared_weights() {
    let mut p = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).profile();
    p.fill_weight(0.5, 0.5, 1.0, 2.0);
    p.fill_weight(0.5, 0.5, 3.0, 2.0);
    let cell = 1 + 4; // bx + (nx + 2)·by
    assert_eq!(p.bin_sumw2[cell], 8.0);
    // Same numbers as the 1-D case: error on the mean = √(1/2).
    let err = p.bin_error(cell);
    assert!((err - 0.5_f64.sqrt()).abs() < 1e-12, "got {err}");
}

#[test]
fn tprofile3d_weighted_fill_tracks_squared_weights() {
    let mut p = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .profile();
    p.fill(0.5, 0.5, 0.5, 1.0); // unit, untracked
    p.fill_weight(0.5, 0.5, 0.5, 1.0, 2.0); // turns tracking on
    let cell = 1 + 4 * (1 + 4); // bx + sx·(by + sy·bz)
    assert_eq!(p.bin_entries[cell], 3.0);
    assert_eq!(p.bin_sumw2[cell], 1.0 + 4.0);
}

#[test]
fn weighted_tprofile_round_trips_its_squared_weights() {
    let mut p = Hist::reg(4, 0.0, 4.0)
        .profile()
        .named("wp")
        .titled("weighted profile");
    p.fill_weight(0.5, 1.0, 2.0);
    p.fill_weight(1.5, 2.0, 0.5);

    let out = std::env::temp_dir().join(format!(
        "oxiroot_weighted_tprofile_{}.root",
        std::process::id()
    ));
    p.write_root(&out, Compression::None).expect("write");
    let back = TProfile::read_root(&FileReader::open(&out).expect("open"), "wp").expect("read");
    let _ = std::fs::remove_file(&out);

    assert_eq!(back.bin_sumw2, p.bin_sumw2);
    assert_eq!(back.effective_entries(1), 1.0);
    assert_eq!(back, p);
}

/// The committed ROOT-written fixture carries no `fBinSumw2` (it was filled with
/// unit weights), and reading it must not invent one.
#[test]
fn unit_weight_root_fixture_still_has_no_squared_weights() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tprofile2d.root");
    let f = FileReader::open(path).expect("open fixture");
    let p = oxiroot_hist::TProfile2D::read_root(&f, "p2").expect("read");
    assert!(p.bin_sumw2.is_empty());
}

// ------------------------------------------ derived histograms keep Σw² right
//
// Weighted fills now always track Σw², so every operation that derives a new
// histogram must carry the variances along with the contents, and every
// operation after which Σw² can no longer be derived from the contents must
// turn tracking on. These mirror ROOT's `Rebin`, `Scale`, `GetCumulative` and
// `DoProfile`.

#[test]
fn rebin_folds_variances_into_the_flow_bins() {
    let mut h = Hist::reg(4, 0.0, 4.0).double();
    h.fill_weight(-1.0, 2.0); // underflow
    h.fill_weight(0.5, 2.0);
    h.fill_weight(3.5, 3.0); // bin 4: left over by rebin(3), folds into overflow
    h.fill_weight(9.0, 3.0); // overflow
    let r = h.rebin(3);
    assert_eq!(r.contents, [2.0, 2.0, 6.0]);
    assert_eq!(r.sumw2, [4.0, 4.0, 18.0]);
    assert_eq!(r.bin_error(0), 2.0);
    assert_eq!(r.bin_error(2), 18.0_f64.sqrt());
}

#[test]
fn slice_folds_variances_into_the_flow_bins() {
    let mut h = Hist::reg(4, 0.0, 4.0).double();
    h.fill_weight(0.5, 2.0); // dropped below the slice -> underflow
    h.fill_weight(1.5, 2.0);
    h.fill_weight(2.5, 3.0);
    h.fill_weight(3.5, 3.0); // dropped above the slice -> overflow
    let s = h.slice(1.5, 2.5);
    assert_eq!(s.contents, [2.0, 2.0, 3.0, 3.0]);
    assert_eq!(s.sumw2, [4.0, 4.0, 9.0, 9.0]);
    assert_eq!(s.bin_error(0), 2.0);
    assert_eq!(s.bin_error(3), 3.0);
}

#[test]
fn scale_turns_tracking_on_so_later_fills_seed_correctly() {
    let mut h = Hist::reg(4, 0.0, 4.0).double();
    for _ in 0..4 {
        h.fill(0.5);
    }
    h.scale(0.5);
    // Four counts scaled by ½: content 2, variance 4·¼ = 1 (not √2 from the content).
    assert_eq!(h.bin_error(1), 1.0);
    h.fill_weight(0.5, 2.0);
    assert_eq!(h.sumw2[1], 1.0 + 4.0);
    assert_eq!(h.bin_error(1), 5.0_f64.sqrt());

    // The same through the `*=` operator, and in two dimensions.
    let mut h2 = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).double();
    h2.fill(0.5, 0.5);
    h2 *= 3.0;
    assert_eq!(h2.bin_error(1 + 4), 3.0);
}

#[test]
fn cumulative_accumulates_variances() {
    let mut h = Hist::reg(2, 0.0, 2.0).double();
    h.fill_weight(0.5, 2.0);
    h.fill_weight(1.5, 2.0);

    let fwd = h.cumulative(true);
    assert_eq!(fwd.contents, [0.0, 2.0, 4.0, 0.0]);
    assert_eq!(fwd.sumw2, [0.0, 4.0, 8.0, 0.0]);
    let bwd = h.cumulative(false);
    assert_eq!(bwd.contents, [0.0, 4.0, 2.0, 0.0]);
    assert_eq!(bwd.sumw2, [0.0, 8.0, 4.0, 0.0]);

    // A later weighted fill adds to the running variance instead of re-seeding
    // from the cumulative contents.
    let mut fwd = fwd;
    fwd.fill_weight(1.5, 3.0);
    assert_eq!(fwd.sumw2[2], 8.0 + 9.0);

    // Counts stay untracked, with √content errors.
    let mut counts = Hist::reg(2, 0.0, 2.0).double();
    counts.fill(0.5);
    counts.fill(1.5);
    let c = counts.cumulative(true);
    assert!(c.sumw2.is_empty());
    assert_eq!(c.bin_error(2), 2.0_f64.sqrt());
}

#[test]
fn profile_of_a_weighted_th2_matches_a_direct_weighted_profile() {
    let mut h2 = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).double();
    h2.fill_weight(0.5, 0.5, 2.0);
    h2.fill_weight(0.5, 1.5, 2.0);
    let from_th2 = h2.profile_x("px");

    let mut direct = Hist::reg(2, 0.0, 2.0).profile();
    direct.fill_weight(0.5, 0.5, 2.0);
    direct.fill_weight(0.5, 1.5, 2.0);

    assert_eq!(from_th2.bin_entries, direct.bin_entries);
    assert_eq!(from_th2.sums, direct.sums);
    assert_eq!(from_th2.sumy2, direct.sumy2);
    assert_eq!(from_th2.bin_sumw2, direct.bin_sumw2);
    // Σw = 4 and Σw² = 8, so 2 effective entries, not 4.
    assert_eq!(from_th2.effective_entries(1), 2.0);
    assert_eq!(from_th2.bin_error(1), direct.bin_error(1));

    // Merging keeps the bookkeeping exact: Σw² = 16, neff = 64/16.
    let mut merged = direct.clone();
    merged.add(&from_th2, 1.0).unwrap();
    assert_eq!(merged.bin_sumw2[1], 16.0);
    assert_eq!(merged.effective_entries(1), 4.0);

    // An unweighted TH2 still profiles without tracking.
    let mut counts = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).double();
    counts.fill(0.5, 0.5);
    assert!(counts.profile_x("p").bin_sumw2.is_empty());
}
