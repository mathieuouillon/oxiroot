//! Hypothesis tests returning `(statistic, p_value)`, matching `scipy.stats`.

use crate::descriptive::{kurtosis, mean, skew, std_dev};
use crate::distributions::{ChiSquared, StudentT};

/// One-sample t-test of the sample mean against `popmean` —
/// `scipy.stats.ttest_1samp`. Two-sided p-value.
#[must_use]
pub fn ttest_1samp(data: &[f64], popmean: f64) -> (f64, f64) {
    let n = data.len() as f64;
    let se = std_dev(data, 1) / n.sqrt();
    let t = (mean(data) - popmean) / se;
    (t, 2.0 * StudentT::new(n - 1.0).sf(t.abs()))
}

/// Independent two-sample t-test assuming equal population variance (Student's) —
/// `scipy.stats.ttest_ind`. Two-sided p-value.
#[must_use]
pub fn ttest_ind(a: &[f64], b: &[f64]) -> (f64, f64) {
    let (n1, n2) = (a.len() as f64, b.len() as f64);
    let (v1, v2) = (std_dev(a, 1).powi(2), std_dev(b, 1).powi(2));
    let sp2 = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
    let se = (sp2 * (1.0 / n1 + 1.0 / n2)).sqrt();
    let t = (mean(a) - mean(b)) / se;
    (t, 2.0 * StudentT::new(n1 + n2 - 2.0).sf(t.abs()))
}

/// D'Agostino–Pearson omnibus test of normality — `scipy.stats.normaltest`.
/// Returns `(K², p)` where `K² = Z_skew² + Z_kurtosis²` and `p` is the χ²(2)
/// survival function of `K²`.
#[must_use]
pub fn normaltest(data: &[f64]) -> (f64, f64) {
    let zs = skewtest_z(data);
    let zk = kurtosistest_z(data);
    let k2 = zs * zs + zk * zk;
    (k2, ChiSquared::new(2.0).sf(k2))
}

/// The z-statistic of D'Agostino's skewness test (`scipy.stats.skewtest`).
fn skewtest_z(data: &[f64]) -> f64 {
    let n = data.len() as f64;
    let b1 = skew(data, true);
    let mut y = b1 * ((n + 1.0) * (n + 3.0) / (6.0 * (n - 2.0))).sqrt();
    let beta2 = 3.0 * (n * n + 27.0 * n - 70.0) * (n + 1.0) * (n + 3.0)
        / ((n - 2.0) * (n + 5.0) * (n + 7.0) * (n + 9.0));
    let w2 = -1.0 + (2.0 * (beta2 - 1.0)).sqrt();
    let delta = 1.0 / (0.5 * w2.ln()).sqrt();
    let alpha = (2.0 / (w2 - 1.0)).sqrt();
    if y == 0.0 {
        y = 1.0;
    }
    delta * (y / alpha + ((y / alpha).powi(2) + 1.0).sqrt()).ln()
}

/// The z-statistic of the Anscombe–Glynn kurtosis test (`scipy.stats.kurtosistest`).
fn kurtosistest_z(data: &[f64]) -> f64 {
    let n = data.len() as f64;
    let b2 = kurtosis(data, false, true); // Pearson, biased
    let e = 3.0 * (n - 1.0) / (n + 1.0);
    let varb2 = 24.0 * n * (n - 2.0) * (n - 3.0) / ((n + 1.0).powi(2) * (n + 3.0) * (n + 5.0));
    let x = (b2 - e) / varb2.sqrt();
    let sqrtbeta1 = 6.0 * (n * n - 5.0 * n + 2.0) / ((n + 7.0) * (n + 9.0))
        * (6.0 * (n + 3.0) * (n + 5.0) / (n * (n - 2.0) * (n - 3.0))).sqrt();
    let a =
        6.0 + 8.0 / sqrtbeta1 * (2.0 / sqrtbeta1 + (1.0 + 4.0 / (sqrtbeta1 * sqrtbeta1)).sqrt());
    let term1 = 1.0 - 2.0 / (9.0 * a);
    let denom = 1.0 + x * (2.0 / (a - 4.0)).sqrt();
    let term2 = denom.signum() * ((1.0 - 2.0 / a) / denom.abs()).cbrt();
    (term1 - term2) / (2.0 / (9.0 * a)).sqrt()
}
