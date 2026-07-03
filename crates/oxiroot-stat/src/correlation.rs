//! Correlation coefficients with two-sided p-values, matching `scipy.stats`.

use crate::descriptive::{mean, rankdata};
use crate::distributions::StudentT;

/// Pearson correlation `r` and its two-sided p-value — `scipy.stats.pearsonr`.
/// The p-value comes from Student's t with `n − 2` degrees of freedom.
#[must_use]
pub fn pearsonr(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len().min(y.len());
    let (mx, my) = (mean(&x[..n]), mean(&y[..n]));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (dx, dy) = (x[i] - mx, y[i] - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    let r = (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0);
    (r, r_pvalue(r, n))
}

/// Spearman rank correlation and its two-sided p-value — `scipy.stats.spearmanr`
/// (Pearson correlation of the ranks).
#[must_use]
pub fn spearmanr(x: &[f64], y: &[f64]) -> (f64, f64) {
    pearsonr(&rankdata(x), &rankdata(y))
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
