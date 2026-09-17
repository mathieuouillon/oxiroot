//! Physics-flavoured helpers (HEP conventions): Gaussian significance ↔ p-value,
//! weighted means and measurement combination, and frequentist confidence
//! intervals for efficiencies (Clopper–Pearson) and counts (Garwood/Poisson).

use std::f64::consts::SQRT_2;

use crate::distributions::ChiSquared;
use crate::error::{check_paired, StatError};
use crate::special::{betaincinv, erfc, gammaln, ndtri};

/// One-sided p-value → Gaussian significance `Z` (number of σ): `Z = Φ⁻¹(1 − p)`.
/// Uses `−Φ⁻¹(p)` so the deep tails (e.g. the `p = 2.87e-7` of a 5σ discovery)
/// stay accurate.
#[must_use]
pub fn significance_from_pvalue(p: f64) -> f64 {
    -ndtri(p)
}

/// Gaussian significance `Z` (σ) → one-sided p-value `p = Φ(−Z)`.
#[must_use]
pub fn pvalue_from_significance(z: f64) -> f64 {
    0.5 * erfc(z / SQRT_2)
}

/// Weighted arithmetic mean `Σwᵢxᵢ / Σwᵢ`.
///
/// # Errors
///
/// [`StatError::LengthMismatch`] if `values` and `weights` differ in length.
pub fn weighted_mean(values: &[f64], weights: &[f64]) -> Result<f64, StatError> {
    check_paired(values.len(), weights.len())?;
    let sw: f64 = weights.iter().sum();
    Ok(values
        .iter()
        .zip(weights)
        .map(|(&x, &w)| w * x)
        .sum::<f64>()
        / sw)
}

/// Weighted (population) standard deviation about the weighted mean.
///
/// # Errors
///
/// [`StatError::LengthMismatch`] if `values` and `weights` differ in length.
pub fn weighted_std(values: &[f64], weights: &[f64]) -> Result<f64, StatError> {
    let m = weighted_mean(values, weights)?;
    let sw: f64 = weights.iter().sum();
    Ok((values
        .iter()
        .zip(weights)
        .map(|(&x, &w)| w * (x - m) * (x - m))
        .sum::<f64>()
        / sw)
        .sqrt())
}

/// Combine independent measurements `valuesᵢ ± errorsᵢ` by inverse-variance
/// weighting. Returns the combined `(mean, error)`, where the weights are
/// `1/errorᵢ²` and the error on the mean is `1/√Σwᵢ`.
///
/// # Errors
///
/// [`StatError::LengthMismatch`] if `values` and `errors` differ in length.
pub fn combine_measurements(values: &[f64], errors: &[f64]) -> Result<(f64, f64), StatError> {
    check_paired(values.len(), errors.len())?;
    let weights: Vec<f64> = errors.iter().map(|&e| 1.0 / (e * e)).collect();
    let sw: f64 = weights.iter().sum();
    let mean = values
        .iter()
        .zip(&weights)
        .map(|(&x, &w)| w * x)
        .sum::<f64>()
        / sw;
    Ok((mean, (1.0 / sw).sqrt()))
}

/// Clopper–Pearson exact confidence interval for a binomial proportion (`k`
/// successes in `n` trials) at confidence level `cl` — e.g. an efficiency and
/// its asymmetric errors (ROOT's `TEfficiency` `kBUniform`/Clopper–Pearson).
/// Returns `(lower, upper)`.
#[must_use]
pub fn clopper_pearson(k: f64, n: f64, cl: f64) -> (f64, f64) {
    let half = (1.0 - cl) / 2.0;
    let lower = if k <= 0.0 {
        0.0
    } else {
        betaincinv(k, n - k + 1.0, half)
    };
    let upper = if k >= n {
        1.0
    } else {
        betaincinv(k + 1.0, n - k, 1.0 - half)
    };
    (lower, upper)
}

/// Garwood exact confidence interval for a Poisson mean given `k` observed
/// counts, at confidence level `cl`. Returns `(lower, upper)` (the lower limit is
/// 0 when `k = 0`).
#[must_use]
pub fn poisson_conf_interval(k: f64, cl: f64) -> (f64, f64) {
    let half = (1.0 - cl) / 2.0;
    let lower = if k <= 0.0 {
        0.0
    } else {
        0.5 * ChiSquared::new(2.0 * k).ppf(half)
    };
    let upper = 0.5 * ChiSquared::new(2.0 * (k + 1.0)).ppf(1.0 - half);
    (lower, upper)
}

/// Wilson score confidence interval for a binomial proportion (`k` of `n`) at
/// confidence level `cl` — scipy's `method='wilson'`. Returns `(lower, upper)`.
#[must_use]
pub fn wilson_interval(k: f64, n: f64, cl: f64) -> (f64, f64) {
    let z = ndtri(1.0 - (1.0 - cl) / 2.0);
    let z2 = z * z;
    let p_hat = k / n;
    let denom = 1.0 + z2 / n;
    let center = (p_hat + z2 / (2.0 * n)) / denom;
    let half = (z / denom) * (p_hat * (1.0 - p_hat) / n + z2 / (4.0 * n * n)).sqrt();
    ((center - half).max(0.0), (center + half).min(1.0))
}

/// Agresti–Coull confidence interval for a binomial proportion (`k` of `n`) at
/// confidence level `cl` — the "add `z²` trials" adjusted Wald interval. Returns
/// `(lower, upper)`.
#[must_use]
pub fn agresti_coull_interval(k: f64, n: f64, cl: f64) -> (f64, f64) {
    let z = ndtri(1.0 - (1.0 - cl) / 2.0);
    let z2 = z * z;
    let nt = n + z2;
    let pt = (k + z2 / 2.0) / nt;
    let half = z * (pt * (1.0 - pt) / nt).sqrt();
    ((pt - half).max(0.0), (pt + half).min(1.0))
}

/// Feldman–Cousins unified frequentist confidence interval for a Poisson signal
/// mean given `n_obs` observed counts over a known mean `background`, at
/// confidence level `cl` (Feldman & Cousins, 1998). Returns `(lower, upper)`; the
/// belt is built by the likelihood-ratio ordering on a fine μ grid.
#[must_use]
pub fn feldman_cousins(n_obs: usize, background: f64, cl: f64) -> (f64, f64) {
    let step = 0.005;
    let center = n_obs as f64 + background;
    let mu_max = center + 15.0 * (center + 1.0).sqrt() + 30.0;
    let (mut lower, mut upper) = (f64::INFINITY, 0.0f64);
    let mut mu = 0.0;
    while mu <= mu_max {
        if fc_accepts(mu, background, cl, n_obs) {
            lower = lower.min(mu);
            upper = upper.max(mu);
        }
        mu += step;
    }
    (if lower.is_finite() { lower } else { 0.0 }, upper)
}

/// Whether `n_obs` lies in the Feldman–Cousins acceptance region for signal `mu`.
fn fc_accepts(mu: f64, b: f64, cl: f64, n_obs: usize) -> bool {
    let s = mu + b;
    let nmax = (s + 12.0 * (s + 1.0).sqrt() + 40.0).ceil() as usize;
    // (n, P(n | s), likelihood ratio R = P(n | s) / P(n | best-fit signal)).
    let mut rows: Vec<(usize, f64, f64)> = (0..=nmax)
        .map(|n| {
            let pn = poisson_pmf(n, s);
            let mu_best = (n as f64 - b).max(0.0);
            let p_best = poisson_pmf(n, mu_best + b);
            let r = if p_best > 0.0 { pn / p_best } else { 0.0 };
            (n, pn, r)
        })
        .collect();
    // Add n's in decreasing likelihood ratio until the coverage reaches `cl`.
    rows.sort_by(|x, y| y.2.total_cmp(&x.2));
    let mut acc = 0.0;
    for (n, pn, _) in rows {
        acc += pn;
        if n == n_obs {
            return true;
        }
        if acc >= cl {
            return false;
        }
    }
    false
}

/// Poisson pmf `e^{−λ} λⁿ / n!`.
fn poisson_pmf(n: usize, lambda: f64) -> f64 {
    if lambda <= 0.0 {
        return f64::from(u8::from(n == 0));
    }
    (n as f64 * lambda.ln() - lambda - gammaln((n + 1) as f64)).exp()
}
