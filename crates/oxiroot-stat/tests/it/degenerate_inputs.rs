//! Non-finite, degenerate and very large inputs: every call must return in
//! bounded time with a documented value.
//!
//! Several of these used to loop forever (a `NaN` makes an `x <= eps` exit test
//! permanently false), so each call runs on its own thread with a deadline: a
//! regression fails the test instead of hanging the test run.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use oxiroot_stat::{
    chi_square_prob, erf, erfc, gammainc, gammaincc, ks_1samp, ks_2samp, mannwhitneyu, poisson_cdf,
    Normal,
};

/// Run `f` on a worker thread and return its result, failing the test if it
/// does not finish within ten seconds.
fn within_deadline<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("{what} did not return within 10 s"))
}

fn assert_close(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: got {got:?}, want {want:?}"
    );
}

#[test]
fn nan_arguments_return_nan() {
    for (a, x) in [(f64::NAN, 1.0), (1.0, f64::NAN), (f64::NAN, f64::NAN)] {
        let (p, q) = within_deadline("gammainc/gammaincc(NaN)", move || {
            (gammainc(a, x), gammaincc(a, x))
        });
        assert!(p.is_nan() && q.is_nan(), "P/Q({a}, {x}) = {p}/{q}");
    }
    assert!(within_deadline("chi_square_prob(NaN)", || chi_square_prob(f64::NAN, 3)).is_nan());
    assert!(within_deadline("erf(NaN)", || erf(f64::NAN)).is_nan());
    assert!(within_deadline("erfc(NaN)", || erfc(f64::NAN)).is_nan());
    assert!(within_deadline("Normal::cdf(NaN)", || Normal::standard().cdf(f64::NAN)).is_nan());
}

#[test]
fn infinite_arguments_give_their_limits() {
    let inf = f64::INFINITY;
    assert_eq!(gammainc(2.0, inf), 1.0);
    assert_eq!(gammaincc(2.0, inf), 0.0);
    assert_eq!(gammainc(inf, 2.0), 0.0);
    assert_eq!(gammaincc(inf, 2.0), 1.0);
    // Both infinite has no limit; scipy.special.gammainc(inf, inf) is NaN too.
    assert!(gammainc(inf, inf).is_nan());
    assert!(gammaincc(inf, inf).is_nan());
    assert_eq!(chi_square_prob(inf, 3), 0.0);
    assert_eq!(erf(inf), 1.0);
    assert_eq!(erfc(inf), 0.0);
}

/// A named computation and the value it must produce.
type Case = (&'static str, fn() -> f64, f64);

/// The series and continued fraction need O(√a) steps when `x` is near `a`. A
/// fixed iteration cap once truncated them here and returned wrong values; these
/// references come from the uncapped implementation.
#[test]
fn large_shape_parameters_converge() {
    let cases: [Case; 5] = [
        (
            "P(1e6, 1e6)",
            || gammainc(1e6, 1e6),
            0.500_132_980_418_815_8,
        ),
        (
            "P(5e4, 5e4)",
            || gammainc(5e4, 5e4),
            0.500_594_708_066_390_4,
        ),
        (
            "Q(5e6, 5e6 + 3√5e6)",
            || gammaincc(5e6, 5e6 + 3.0 * 5e6_f64.sqrt()),
            0.001_355_188_195_199_019,
        ),
        // χ² one standard deviation below its mean: about Φ(1).
        (
            "chi_square_prob(1.998e6, 2e6)",
            || chi_square_prob(1_998_000.0, 2_000_000),
            0.841_344_786_446_773_1,
        ),
        (
            "poisson_cdf(1e6, 1e6)",
            || poisson_cdf(1e6, 1e6),
            0.500_265_961_828_067_2,
        ),
    ];
    for (what, f, want) in cases {
        assert_close(what, within_deadline(what, f), want);
    }
}

/// Finite arguments whose intermediates overflow, or whose counter can no
/// longer step by one (a ≥ 2⁵³), return NaN instead of spinning.
#[test]
fn finite_arguments_the_series_cannot_evaluate_terminate() {
    for (a, x) in [(1e308, 1e308), (1e16, 1e16), (9.1e15, 9.1e15)] {
        let (p, q) = within_deadline("gammainc/gammaincc(huge a)", move || {
            (gammainc(a, x), gammaincc(a, x))
        });
        assert!(p.is_nan() && q.is_nan(), "P/Q({a:e}, {x:e}) = {p}/{q}");
    }
    // Far from x ≈ a the answer is still exact, whatever the size of a.
    assert_eq!(gammainc(1e300, 1.0), 0.0);
}

#[test]
fn rank_and_ks_tests_propagate_nan_and_reject_empty_samples() {
    let both_nan = |(d, p): (f64, f64)| d.is_nan() && p.is_nan();
    // The two-sample KS merge never advanced past a NaN.
    assert!(both_nan(within_deadline("ks_2samp(NaN)", || {
        ks_2samp(&[1.0, 2.0, f64::NAN], &[1.0, 2.0, 3.0])
    })));
    assert!(both_nan(within_deadline("ks_2samp(NaN only)", || {
        ks_2samp(&[1.0, 2.0], &[f64::NAN])
    })));
    assert!(both_nan(ks_2samp(&[], &[1.0])));
    // `f64::max` used to drop a NaN distance, and `f64::min` a NaN p-value.
    assert!(both_nan(ks_1samp(&[0.1, f64::NAN], |x| x)));
    assert!(both_nan(ks_1samp(&[0.1, 0.2], |_| f64::NAN)));
    assert!(both_nan(ks_1samp(&[], |x| x)));
    assert!(both_nan(mannwhitneyu(&[f64::NAN, 1.0], &[2.0, 3.0])));
    assert!(both_nan(mannwhitneyu(&[], &[])));
    // Ordinary input is untouched.
    let (d, p) = ks_2samp(&[1.0, 2.0], &[1.5, 2.5]);
    assert!(d == 0.5 && p > 0.0 && p <= 1.0, "got ({d}, {p})");
}
