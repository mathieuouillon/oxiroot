//! Pure-Rust statistics: special functions, distributions, descriptive
//! statistics, correlation, and hypothesis tests — a dependency-free `f64` core
//! whose numerics track [`scipy.stats`](https://docs.scipy.org/doc/scipy/reference/stats.html)
//! (and `scipy.special`) function-for-function, verified against it.
//!
//! It began as the special functions behind oxiroot's histogram comparison tests
//! ([`oxiroot_hist`](https://crates.io/crates/oxiroot-hist)'s `chi2_test`/
//! `kolmogorov_test`) and the goodness-of-fit p-value of
//! [`oxiroot_fit`](https://crates.io/crates/oxiroot-fit), and now offers:
//!
//! - [`special`] — `erf`/`erfc`, `gammaln`, the regularized incomplete
//!   gamma (`gammainc`/`gammaincc`) and beta (`betainc`) functions, and the
//!   inverse normal CDF `ndtri`.
//! - [`distributions`] — [`Normal`], [`StudentT`], [`ChiSquared`], and [`FisherF`]
//!   with `pdf`/`cdf`/`sf`/`ppf`, plus Poisson/Binomial CDF & survival.
//! - [`descriptive`] — `gmean`/`hmean`, `skew`, `kurtosis`, `moment`, `sem`,
//!   `variation`, `iqr`, `median_abs_deviation`, `entropy`, `zscore`, `rankdata`, …
//! - [`correlation`] — `pearsonr`, `spearmanr`.
//! - [`hypothesis`] — `ttest_1samp`, `ttest_ind`, `normaltest`.
//!
//! The commonly-used items are also re-exported at the crate root.

pub mod correlation;
pub mod descriptive;
pub mod distributions;
pub mod hypothesis;
pub mod special;

pub use correlation::{pearsonr, spearmanr};
pub use descriptive::{
    entropy, gmean, hmean, iqr, kl_divergence, kurtosis, median, median_abs_deviation, moment,
    quantile, rankdata, sem, skew, variation, zscore,
};
pub use distributions::{
    binom_cdf, binom_sf, poisson_cdf, poisson_sf, ChiSquared, FisherF, Normal, StudentT,
};
pub use hypothesis::{normaltest, ttest_1samp, ttest_ind};
pub use special::{beta, betainc, betaln, erf, erfc, gammainc, gammaincc, gammaln, ndtri};

/// Chi-square survival function `P(X > chi2)` for `X ~ χ²(ndf)` — ROOT's
/// `TMath::Prob`, i.e. the complemented regularized incomplete gamma
/// `Q(ndf/2, chi2/2)`. The goodness-of-fit p-value (a good fit is near 1, a poor
/// one near 0). `ndf == 0` yields 0; `chi2 <= 0` yields 1.
#[must_use]
pub fn chi_square_prob(chi2: f64, ndf: usize) -> f64 {
    if ndf == 0 {
        return 0.0;
    }
    if chi2 <= 0.0 {
        return 1.0;
    }
    special::gammaincc(ndf as f64 / 2.0, chi2 / 2.0)
}

/// ROOT's `TMath::KolmogorovProb(z)` — the asymptotic Kolmogorov distribution,
/// the p-value of a two-sample Kolmogorov–Smirnov test statistic `z`.
#[must_use]
pub fn kolmogorov_prob(z: f64) -> f64 {
    const FJ: [f64; 4] = [-2.0, -8.0, -18.0, -32.0];
    const W: f64 = 2.506628274631;
    const C1: f64 = -1.2337005501361697;
    const C2: f64 = -11.103304951225528;
    const C3: f64 = -30.842513753404244;
    let u = z.abs();
    if u < 0.2 {
        1.0
    } else if u < 0.755 {
        let v = 1.0 / (u * u);
        1.0 - W * ((C1 * v).exp() + (C2 * v).exp() + (C3 * v).exp()) / u
    } else if u < 6.8116 {
        let v = u * u;
        let maxj = ((3.0 / u).round() as i64).clamp(1, 4) as usize;
        let mut r = [0.0; 4];
        for (j, rj) in r.iter_mut().enumerate().take(maxj) {
            *rj = (FJ[j] * v).exp();
        }
        (2.0 * (r[0] - r[1] + r[2] - r[3])).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chi_square_prob_edges_and_midpoint() {
        assert_eq!(chi_square_prob(10.0, 0), 0.0);
        assert_eq!(chi_square_prob(0.0, 5), 1.0);
        let p = chi_square_prob(1.0, 1);
        assert!((p - 0.3173).abs() < 1e-3, "got {p}");
    }

    #[test]
    fn kolmogorov_prob_is_monotone_and_bounded() {
        assert_eq!(kolmogorov_prob(0.0), 1.0);
        assert_eq!(kolmogorov_prob(100.0), 0.0);
        let (a, b) = (kolmogorov_prob(0.5), kolmogorov_prob(1.5));
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b) && a > b);
    }
}
