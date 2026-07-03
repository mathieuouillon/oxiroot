//! Physics-flavoured helpers (HEP conventions): Gaussian significance ↔ p-value,
//! weighted means and measurement combination, and frequentist confidence
//! intervals for efficiencies (Clopper–Pearson) and counts (Garwood/Poisson).

use std::f64::consts::SQRT_2;

use crate::distributions::ChiSquared;
use crate::special::{betaincinv, erfc, ndtri};

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
#[must_use]
pub fn weighted_mean(values: &[f64], weights: &[f64]) -> f64 {
    let sw: f64 = weights.iter().sum();
    values
        .iter()
        .zip(weights)
        .map(|(&x, &w)| w * x)
        .sum::<f64>()
        / sw
}

/// Weighted (population) standard deviation about the weighted mean.
#[must_use]
pub fn weighted_std(values: &[f64], weights: &[f64]) -> f64 {
    let m = weighted_mean(values, weights);
    let sw: f64 = weights.iter().sum();
    (values
        .iter()
        .zip(weights)
        .map(|(&x, &w)| w * (x - m) * (x - m))
        .sum::<f64>()
        / sw)
        .sqrt()
}

/// Combine independent measurements `valuesᵢ ± errorsᵢ` by inverse-variance
/// weighting. Returns the combined `(mean, error)`, where the weights are
/// `1/errorᵢ²` and the error on the mean is `1/√Σwᵢ`.
#[must_use]
pub fn combine_measurements(values: &[f64], errors: &[f64]) -> (f64, f64) {
    let weights: Vec<f64> = errors.iter().map(|&e| 1.0 / (e * e)).collect();
    let sw: f64 = weights.iter().sum();
    let mean = values
        .iter()
        .zip(&weights)
        .map(|(&x, &w)| w * x)
        .sum::<f64>()
        / sw;
    (mean, (1.0 / sw).sqrt())
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
