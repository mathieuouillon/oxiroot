//! Item 3: histogram arithmetic — merge (add), scale, multiply, divide,
//! integral — with Sumw2 error propagation. Merge + scale are cross-checked in
//! official ROOT.

use std::path::PathBuf;

use oxiroot_hist::{Hist, Hist1D, ReadRoot, WriteRoot};
use oxiroot_io_core::FileReader;

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
    let f = FileReader::open(&out).expect("reopen");
    assert_eq!(Hist1D::read_root(&f, "h").unwrap(), a, "round-trips");
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

/// Regression: `Profile1D::add` scaled Σw·y² by `c²` while every other weighted
/// sum — and `Hist2D::add`'s identical `tsumwy2` line — scales by `c`. Σw·y² is
/// linear in the weight, so `c` is correct. Invisible at `c == 1` (the merge
/// path), which is why no existing test caught it.
#[test]
fn profile_add_scales_weighted_sums_linearly() {
    use oxiroot_hist::Hist;

    let build = || {
        let mut p = Hist::reg(4, 0.0, 4.0).profile();
        p.fill(0.5, 2.0);
        p.fill(1.5, 3.0);
        p
    };
    let (mut dst, src) = (build(), build());

    let (w0, w20, wx0, wx20, wy0, wy20) = (
        dst.tsumw,
        dst.tsumw2,
        dst.tsumwx,
        dst.tsumwx2,
        dst.tsumwy,
        dst.tsumwy2,
    );
    let c = 3.0;
    dst.add(&src, c).unwrap();

    // Σw² is quadratic in the scale factor; every Σw·(…) sum is linear.
    assert!((dst.tsumw - (w0 + c * src.tsumw)).abs() < 1e-12);
    assert!((dst.tsumw2 - (w20 + c * c * src.tsumw2)).abs() < 1e-12);
    assert!((dst.tsumwx - (wx0 + c * src.tsumwx)).abs() < 1e-12);
    assert!((dst.tsumwx2 - (wx20 + c * src.tsumwx2)).abs() < 1e-12);
    assert!((dst.tsumwy - (wy0 + c * src.tsumwy)).abs() < 1e-12);
    assert!(
        (dst.tsumwy2 - (wy20 + c * src.tsumwy2)).abs() < 1e-12,
        "tsumwy2 scaled by c^2 instead of c: got {}, want {}",
        dst.tsumwy2,
        wy20 + c * src.tsumwy2
    );
}

/// Regression: scaling one unweighted profile into another must track Σw²
/// separately from Σw, because Σ(c·w)² = c²·Σw² ≠ Σ(c·w). `add` used to leave
/// `bin_sumw2` empty for two unweighted inputs, so `effective_entries` fell back
/// to Σw and every bin error was wrong for `c != 1`.
#[test]
fn profile_add_tracks_squared_weights_when_scaled() {
    use oxiroot_hist::Hist;

    // Bin 1 ([0, 1)): two unweighted entries in `dst`, one in `src`.
    let mut dst = Hist::reg(4, 0.0, 4.0).profile();
    dst.fill(0.5, 1.0);
    dst.fill(0.5, 3.0);
    let mut src = Hist::reg(4, 0.0, 4.0).profile();
    src.fill(0.5, 2.0);

    let c = 3.0;
    dst.add(&src, c).unwrap();

    // Σw = 2 + 3·1 = 5; Σw² = 2 + 3²·1 = 11; neff = (Σw)² / Σw² = 25 / 11.
    let bin = 1;
    assert!((dst.bin_entries[bin] - 5.0).abs() < 1e-12);
    assert!(
        (dst.bin_sumw2[bin] - 11.0).abs() < 1e-12,
        "Σw² should be 2 + c²·1 = 11, got {:?}",
        dst.bin_sumw2.get(bin)
    );
    let neff = dst.effective_entries(bin);
    assert!(
        (neff - 25.0 / 11.0).abs() < 1e-12,
        "effective entries should be 25/11, got {neff} (5.0 means Σw² was taken as Σw)"
    );
}

/// The same law when both sides already carry `fBinSumw2` — the shape of a
/// weighted profile read back from a ROOT file.
#[test]
fn profile_add_scales_tracked_squared_weights_quadratically() {
    use oxiroot_hist::Hist;

    let mut dst = Hist::reg(4, 0.0, 4.0).profile();
    dst.fill(0.5, 1.0);
    let mut src = Hist::reg(4, 0.0, 4.0).profile();
    src.fill(0.5, 2.0);
    // As if both were filled with weights and read from disk.
    dst.bin_sumw2 = vec![0.0, 4.0, 0.0, 0.0, 0.0, 0.0];
    src.bin_sumw2 = vec![0.0, 9.0, 0.0, 0.0, 0.0, 0.0];

    let c = 2.0;
    dst.add(&src, c).unwrap();

    // 4 + c²·9 = 40, not 4 + c·9 = 22.
    assert!(
        (dst.bin_sumw2[1] - 40.0).abs() < 1e-12,
        "got {}",
        dst.bin_sumw2[1]
    );
}

/// `c == 1` is the merge path: nothing about the Σw² handling may change there,
/// and two unweighted profiles must still merge without allocating `bin_sumw2`.
#[test]
fn profile_add_at_unit_scale_is_unchanged() {
    use oxiroot_hist::Hist;

    let mut dst = Hist::reg(4, 0.0, 4.0).profile();
    dst.fill(0.5, 1.0);
    let mut src = Hist::reg(4, 0.0, 4.0).profile();
    src.fill(0.5, 2.0);

    dst.add(&src, 1.0).unwrap();
    assert!(dst.bin_sumw2.is_empty());
    assert!((dst.effective_entries(1) - 2.0).abs() < 1e-12);
}

/// The types ROOT's `hadd` merges beyond the histogram and profile families:
/// they merge the same way, and refuse an incompatible partner.
mod merges {
    use oxiroot_hist::{
        Efficiency, Graph, GraphFunction, Hist, HistStack, Mergeable, PolyHist, SparseHist,
    };
    use oxiroot_io_core::Error;

    fn poly() -> PolyHist {
        let mut p = PolyHist::new(0.0, 2.0, 0.0, 2.0);
        p.add_bin_rect(0.0, 0.0, 1.0, 1.0);
        p.add_bin_rect(1.0, 1.0, 2.0, 2.0);
        p
    }

    #[test]
    fn an_efficiency_sums_passed_and_total() {
        let mut a = Efficiency::new(2, 0.0, 2.0).named("e");
        a.fill(true, 0.5);
        let mut b = Efficiency::new(2, 0.0, 2.0).named("e");
        b.fill(false, 0.5);
        b.fill(true, 1.5);
        a.merge(&b).unwrap();
        assert_eq!(a.passed.integral(), 2.0);
        assert_eq!(a.total.integral(), 3.0);

        // A different binning changes nothing.
        let other = Efficiency::new(3, 0.0, 3.0).named("e");
        assert!(matches!(
            a.merge(&other),
            Err(Error::BinningMismatch { .. })
        ));
        assert_eq!(a.total.integral(), 3.0);
    }

    #[test]
    fn a_poly_histogram_sums_its_bins() {
        let mut a = poly();
        a.fill_weight(0.5, 0.5, 2.0);
        let mut b = poly();
        b.fill_weight(0.5, 0.5, 1.0);
        b.fill_weight(1.5, 1.5, 4.0);
        a.merge(&b).unwrap();
        assert_eq!(
            a.bins.iter().map(|x| x.content).collect::<Vec<_>>(),
            vec![3.0, 4.0]
        );
        assert_eq!(a.entries, 3.0);

        // Different polygons change nothing.
        let mut other = PolyHist::new(0.0, 2.0, 0.0, 2.0);
        other.add_bin_rect(0.0, 0.0, 2.0, 2.0);
        assert!(matches!(
            a.merge(&other),
            Err(Error::BinningMismatch { .. })
        ));
        assert_eq!(a.bins[0].content, 3.0);
    }

    #[test]
    fn a_sparse_histogram_sums_shared_bins_and_keeps_the_rest() {
        let mut a = SparseHist::new(&[(4, 0.0, 4.0)]).named("sp");
        a.fill(&[0.5]).unwrap();
        let mut b = SparseHist::new(&[(4, 0.0, 4.0)]).named("sp");
        b.fill(&[0.5]).unwrap();
        b.fill(&[2.5]).unwrap();
        a.merge(&b).unwrap();
        assert_eq!(
            a.bins.len(),
            2,
            "the shared bin is summed, the new one kept"
        );
        assert_eq!(a.bins.iter().map(|x| x.content).sum::<f64>(), 3.0);
        assert_eq!(a.entries, 3.0);

        let other = SparseHist::new(&[(2, 0.0, 2.0)]).named("sp");
        assert!(matches!(
            a.merge(&other),
            Err(Error::BinningMismatch { .. })
        ));
    }

    #[test]
    fn graphs_append_their_points_and_errors() {
        let mut a = Graph::with_errors(vec![1.0], vec![2.0], vec![0.1], vec![0.2])
            .unwrap()
            .named("g")
            .with_function(GraphFunction::new("f", "[0]*x", vec![1.0], 0.0, 1.0));
        let b = Graph::with_errors(
            vec![3.0, 4.0],
            vec![5.0, 6.0],
            vec![0.3, 0.4],
            vec![0.5, 0.6],
        )
        .unwrap();
        a.merge(&b).unwrap();
        assert_eq!(a.x, vec![1.0, 3.0, 4.0]);
        assert_eq!(a.y, vec![2.0, 5.0, 6.0]);
        assert_eq!(a.functions.len(), 1, "the attached function is kept");

        // Different error bars change nothing.
        let plain = Graph::new(vec![9.0], vec![9.0]).unwrap();
        assert!(matches!(a.merge(&plain), Err(Error::InvalidInput(_))));
        assert_eq!(a.x.len(), 3);
    }

    #[test]
    fn a_stack_merges_its_histograms_by_name() {
        let mut first = Hist::reg(2, 0.0, 2.0).double().named("one");
        first.fill(0.5);
        let mut again = Hist::reg(2, 0.0, 2.0).double().named("one");
        again.fill(0.5);
        let other = Hist::reg(2, 0.0, 2.0).double().named("two");

        let mut a = HistStack::new().named("st").add(first);
        let b = HistStack::new().named("st").add(again).add(other);
        a.merge(&b).unwrap();
        assert_eq!(a.hists().len(), 2, "the new histogram is appended");
        assert_eq!(a.hists()[0].integral(), 2.0, "the shared one is summed");
        assert_eq!(a.hists()[1].name, "two");
    }
}
