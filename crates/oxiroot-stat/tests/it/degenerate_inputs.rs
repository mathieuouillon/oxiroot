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
        ((got - want) / want).abs() < 1e-13,
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

/// Near `x ≈ a` the series and continued fraction need O(√a) steps: a fixed
/// iteration cap once truncated them here and returned wrong values. Temme's
/// expansion now handles these points. The references are mpmath's, at 50
/// digits.
#[test]
fn large_shape_parameters_converge() {
    let cases: [Case; 5] = [
        (
            "P(1e6, 1e6)",
            || gammainc(1e6, 1e6),
            0.500_132_980_760_872_5,
        ),
        (
            "P(5e4, 5e4)",
            || gammainc(5e4, 5e4),
            0.500_594_708_104_793_3,
        ),
        (
            "Q(5e6, 5e6 + 3√5e6)",
            || gammaincc(5e6, 5e6 + 3.0 * 5e6_f64.sqrt()),
            0.001_355_188_192_241_146_4,
        ),
        // χ² one standard deviation below its mean: about Φ(1).
        (
            "chi_square_prob(1.998e6, 2e6)",
            || chi_square_prob(1_998_000.0, 2_000_000),
            0.841_344_786_425_696_3,
        ),
        (
            "poisson_cdf(1e6, 1e6)",
            || poisson_cdf(1e6, 1e6),
            0.500_265_961_486_283_7,
        ),
    ];
    for (what, f, want) in cases {
        assert_close(what, within_deadline(what, f), want);
    }
}

/// Beyond 2⁵³, where `a + 1` is no longer exact, the series cannot step; near
/// `x ≈ a` Temme's expansion takes over and stays exact to rounding. At the
/// peak it reduces to `P(a, a) = 1/2 + 1/(3√(2πa)) + O(a^(−3/2))`.
#[test]
fn huge_shape_parameters_are_evaluated_near_the_peak() {
    for a in [9.1e15, 1e16, 1e20, 1e308] {
        let (p, q) = within_deadline("gammainc/gammaincc(huge a)", move || {
            (gammainc(a, a), gammaincc(a, a))
        });
        let excess = 1.0 / (3.0 * (2.0 * std::f64::consts::PI * a).sqrt());
        assert!(
            (p - (0.5 + excess)).abs() <= 1e-16 && (q - (0.5 - excess)).abs() <= 1e-16,
            "P/Q({a:e}, a) = {p}/{q}"
        );
    }
    // Far from x ≈ a the answer is exact, whatever the size of a.
    assert_eq!(gammainc(1e300, 1.0), 0.0);
    assert_eq!(gammaincc(1e308, f64::MAX), 0.0);
}

/// scipy's values where an argument is zero or negative.
#[test]
fn zero_and_negative_arguments_follow_scipy() {
    for (a, x) in [
        (-1.0, 1.0),
        (1.0, -1.0),
        (-1.0, -1.0),
        (0.0, 0.0),
        (0.0, -1.0),
    ] {
        assert!(gammainc(a, x).is_nan(), "P({a}, {x})");
        assert!(gammaincc(a, x).is_nan(), "Q({a}, {x})");
    }
    // Γ(0) is infinite, so all the mass of Gamma(0) sits at 0.
    assert_eq!((gammainc(0.0, 0.5), gammaincc(0.0, 0.5)), (1.0, 0.0));
    assert_eq!((gammainc(2.5, 0.0), gammaincc(2.5, 0.0)), (0.0, 1.0));
    // The distributions keep their support: a chi-squared is 0 below 0.
    let chi2 = oxiroot_stat::ChiSquared::new(3.0);
    assert_eq!((chi2.cdf(-1.0), chi2.sf(-1.0)), (0.0, 1.0));
    assert!(poisson_cdf(2.0, -1.0).is_nan()); // a negative mean
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
