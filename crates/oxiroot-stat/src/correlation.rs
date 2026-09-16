//! Correlation coefficients with two-sided p-values, matching `scipy.stats`.

use crate::descriptive::{mean, rankdata};
use crate::distributions::StudentT;
use crate::error::{check_paired, StatError};

/// Fewest pairs a correlation coefficient is defined for.
const MIN_PAIRS: usize = 2;

/// Reject samples a correlation cannot be computed from.
fn check_pairs(x: &[f64], y: &[f64]) -> Result<usize, StatError> {
    check_paired(x.len(), y.len())?;
    let n = x.len();
    if n < MIN_PAIRS {
        return Err(StatError::TooFewObservations {
            needed: MIN_PAIRS,
            got: n,
        });
    }
    Ok(n)
}

/// The Pearson coefficient of two equal-length samples, clamped to `[-1, 1]`.
fn pearson_r(x: &[f64], y: &[f64]) -> f64 {
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (&xi, &yi) in x.iter().zip(y) {
        let (dx, dy) = (xi - mx, yi - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0)
}

/// Pearson correlation `r` and its two-sided p-value — `scipy.stats.pearsonr`.
/// The p-value comes from Student's t with `n − 2` degrees of freedom. With
/// exactly two pairs the points always lie on a line, so, as in `scipy`, `r` is
/// ±1 and the p-value is 1.
///
/// # Errors
///
/// [`StatError::LengthMismatch`] if `x` and `y` differ in length, and
/// [`StatError::TooFewObservations`] for fewer than two pairs. `scipy` raises on
/// both.
pub fn pearsonr(x: &[f64], y: &[f64]) -> Result<(f64, f64), StatError> {
    let n = check_pairs(x, y)?;
    let r = pearson_r(x, y);
    if n == 2 {
        let p = if r.is_nan() { f64::NAN } else { 1.0 };
        return Ok((r.round(), p));
    }
    Ok((r, r_pvalue(r, n)))
}

/// Spearman rank correlation and its two-sided p-value — `scipy.stats.spearmanr`
/// (Pearson correlation of the ranks). With exactly two pairs the p-value is
/// `NaN`, as in `scipy`, and a `NaN` in either sample gives `(NaN, NaN)`.
///
/// # Errors
///
/// [`StatError::LengthMismatch`] if `x` and `y` differ in length, as `scipy`
/// raises, and [`StatError::TooFewObservations`] for fewer than two pairs, where
/// `scipy` instead returns `(NaN, NaN)`.
pub fn spearmanr(x: &[f64], y: &[f64]) -> Result<(f64, f64), StatError> {
    let n = check_pairs(x, y)?;
    // Ranking would give each NaN its own finite rank and hide it.
    if x.iter().chain(y).any(|v| v.is_nan()) {
        return Ok((f64::NAN, f64::NAN));
    }
    let r = pearson_r(&rankdata(x), &rankdata(y));
    Ok((r, r_pvalue(r, n)))
}

/// Two-sided p-value for correlation `r` over `n` pairs, via Student's t.
fn r_pvalue(r: f64, n: usize) -> f64 {
    if n <= 2 {
        return f64::NAN;
    }
    if r.abs() >= 1.0 {
        return 0.0;
    }
    let df = n as f64 - 2.0;
    let t = r * (df / (1.0 - r * r)).sqrt();
    2.0 * StudentT::new(df).sf(t.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_lengths_are_rejected_not_truncated() {
        // Regression: these used to be zipped to the shorter length, so a
        // 5-point and a 3-point sample reported a perfect correlation
        // (r = 1.0, p = 0.0) from data that was never paired.
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [1.0, 2.0, 3.0];
        let want = StatError::LengthMismatch { left: 5, right: 3 };
        assert_eq!(pearsonr(&x, &y), Err(want.clone()));
        assert_eq!(spearmanr(&x, &y), Err(want));
    }

    #[test]
    fn fewer_than_two_pairs_are_rejected() {
        for n in 0..MIN_PAIRS {
            let v = vec![1.0; n];
            let want = StatError::TooFewObservations {
                needed: MIN_PAIRS,
                got: n,
            };
            assert_eq!(pearsonr(&v, &v), Err(want.clone()), "n = {n}");
            assert_eq!(spearmanr(&v, &v), Err(want), "n = {n}");
        }
    }

    #[test]
    fn equal_lengths_still_correlate() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [2.0, 4.0, 6.0, 8.0, 10.0];
        let (r, _) = pearsonr(&x, &y).unwrap();
        assert!((r - 1.0).abs() < 1e-12, "got {r}");
        let (r, _) = spearmanr(&x, &y).unwrap();
        assert!((r - 1.0).abs() < 1e-12, "got {r}");
    }

    #[test]
    fn two_pairs_follow_scipy() {
        // scipy.stats.pearsonr([1, 2], [3, 5]) -> (1.0, 1.0); a decreasing pair
        // gives (-1.0, 1.0). spearmanr on two pairs gives p = NaN.
        assert_eq!(pearsonr(&[1.0, 2.0], &[3.0, 5.0]), Ok((1.0, 1.0)));
        assert_eq!(pearsonr(&[0.1, 0.7], &[0.3, 0.2]), Ok((-1.0, 1.0)));
        let (rho, p) = spearmanr(&[1.0, 2.0], &[3.0, 5.0]).unwrap();
        assert!((rho - 1.0).abs() < 1e-12 && p.is_nan(), "got ({rho}, {p})");
        // A constant sample has no correlation at all.
        let (r, p) = pearsonr(&[1.0, 1.0], &[3.0, 5.0]).unwrap();
        assert!(r.is_nan() && p.is_nan(), "got ({r}, {p})");
    }
}
