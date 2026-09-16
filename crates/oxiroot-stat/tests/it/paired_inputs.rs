//! Every function that pairs two samples element by element rejects samples of
//! different lengths with `StatError::LengthMismatch`, instead of silently
//! pairing up to the shorter one (which used to report, for example, a perfect
//! correlation from data that was never paired).

use oxiroot_stat::{
    chisquare, combine_measurements, kl_divergence, pearsonr, spearmanr, weighted_mean,
    weighted_std, wilcoxon, StatError,
};

const LONG: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
const SHORT: [f64; 3] = [1.0, 2.0, 3.0];

fn mismatch(left: usize, right: usize) -> StatError {
    StatError::LengthMismatch { left, right }
}

#[test]
fn every_paired_function_rejects_mismatched_lengths() {
    let want = || Err(mismatch(5, 3));
    assert_eq!(pearsonr(&LONG, &SHORT), want());
    assert_eq!(spearmanr(&LONG, &SHORT), want());
    assert_eq!(chisquare(&LONG, &SHORT), want());
    assert_eq!(wilcoxon(&LONG, &SHORT), want());
    assert_eq!(kl_divergence(&LONG, &SHORT).map(|_| (0.0, 0.0)), want());
    assert_eq!(weighted_mean(&LONG, &SHORT).map(|_| (0.0, 0.0)), want());
    assert_eq!(weighted_std(&LONG, &SHORT).map(|_| (0.0, 0.0)), want());
    assert_eq!(combine_measurements(&LONG, &SHORT), want());
}

#[test]
fn the_error_reports_both_lengths_in_argument_order() {
    // Reversing the arguments reverses the report, so a caller can tell which
    // array was short.
    assert_eq!(weighted_mean(&SHORT, &LONG), Err(mismatch(3, 5)));
    assert_eq!(
        mismatch(5, 3).to_string(),
        "paired samples must have the same length, got 5 and 3"
    );
    assert_eq!(
        StatError::TooFewObservations { needed: 2, got: 1 }.to_string(),
        "need at least 2 observations, got 1"
    );
}

#[test]
fn stat_error_is_a_std_error() {
    fn assert_error<E: std::error::Error + Send + Sync + 'static>(_: &E) {}
    assert_error(&mismatch(1, 2));
    // So it converts into the usual boxed error with `?`.
    let boxed: Box<dyn std::error::Error> = mismatch(1, 2).into();
    assert!(boxed.to_string().contains("got 1 and 2"));
}

#[test]
fn weighted_std_matches_a_hand_computation() {
    // mean = (1·1 + 2·1 + 3·2) / 4 = 2.25
    // var  = (1·1.25² + 1·0.25² + 2·0.75²) / 4 = 2.75 / 4 = 0.6875
    let s = weighted_std(&[1.0, 2.0, 3.0], &[1.0, 1.0, 2.0]).unwrap();
    assert!((s - 0.6875_f64.sqrt()).abs() < 1e-12, "got {s}");
}

#[test]
fn kl_divergence_matches_a_hand_computation() {
    // 0.5·ln(0.5/0.9) + 0.5·ln(0.5/0.1) = ½·ln(25/9) = ln(5/3); this is also
    // scipy.stats.entropy([.5, .5], [.9, .1]).
    let d = kl_divergence(&[0.5, 0.5], &[0.9, 0.1]).unwrap();
    assert!((d - (5.0_f64 / 3.0).ln()).abs() < 1e-12, "got {d}");
}

#[test]
fn tests_without_a_statistic_are_rejected() {
    // No pairs at all.
    assert_eq!(
        wilcoxon(&[], &[]),
        Err(StatError::TooFewObservations { needed: 1, got: 0 })
    );
    // Pairs that never differ: the normal approximation is undefined, and
    // scipy.stats.wilcoxon(x, x, method="approx") gives (0.0, nan). This used to
    // report p = 1 through `f64::min`, and before that never returned.
    let (w, p) = wilcoxon(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]).unwrap();
    assert!(w == 0.0 && p.is_nan(), "got ({w}, {p})");
    // scipy.stats.wilcoxon([1, 2.5, 3, 7], [1.2, 2, 3, 4], method="approx").
    let (w, p) = wilcoxon(&[1.0, 2.5, 3.0, 7.0], &[1.2, 2.0, 3.0, 4.0]).unwrap();
    assert!(
        w == 1.0 && (p - 0.285_049_407_402_612_75).abs() < 1e-12,
        "got ({w}, {p})"
    );
    // One category has no degrees of freedom; a perfect one-bin "fit" used to
    // report p = 0 (scipy gives p = nan).
    assert_eq!(
        chisquare(&[5.0], &[5.0]),
        Err(StatError::TooFewObservations { needed: 2, got: 1 })
    );
    assert_eq!(
        chisquare(&[], &[]),
        Err(StatError::TooFewObservations { needed: 2, got: 0 })
    );
    // Two categories are enough.
    assert_eq!(chisquare(&[5.0, 5.0], &[5.0, 5.0]), Ok((0.0, 1.0)));
}

#[test]
fn nan_values_propagate_instead_of_erroring() {
    fn both_nan(r: Result<(f64, f64), StatError>) -> bool {
        matches!(r, Ok((a, b)) if a.is_nan() && b.is_nan())
    }
    let with_nan = [1.0, f64::NAN, 3.0, 4.0];
    let clean = [2.0, 1.0, 4.0, 3.0];
    assert!(both_nan(pearsonr(&with_nan, &clean)));
    assert!(both_nan(spearmanr(&with_nan, &clean)));
    assert!(both_nan(wilcoxon(&with_nan, &clean)));
    assert!(both_nan(chisquare(&with_nan, &clean)));
    assert!(kl_divergence(&with_nan, &clean).unwrap().is_nan());
    assert!(weighted_mean(&with_nan, &clean).unwrap().is_nan());
    // Before, wilcoxon turned the NaN into a confident p = 1 via `f64::min`.
    assert!(both_nan(wilcoxon(&[f64::NAN, 2.0, 3.0], &[1.0, 1.0, 1.0])));
}
