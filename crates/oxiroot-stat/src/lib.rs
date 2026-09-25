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
//! - **Special functions** — [`erf`]/[`erfc`], [`gammaln`], the regularized
//!   incomplete gamma ([`gammainc`]/[`gammaincc`]) and beta ([`betainc`])
//!   functions, and the inverse normal CDF [`ndtri`]. The rest of the crate is
//!   built on them; names follow `scipy.special` where there is a direct
//!   counterpart.
//! - **Distributions** — [`Normal`], [`StudentT`], [`ChiSquared`], and
//!   [`FisherF`] with `pdf`/`cdf`/`sf`/`ppf`, plus the Poisson and Binomial CDF
//!   and survival functions ([`poisson_cdf`], [`binom_sf`], …).
//! - **Descriptive statistics** — [`gmean`]/[`hmean`], [`skew`], [`kurtosis`],
//!   [`moment`], [`sem`], [`variation`], [`iqr`], [`median_abs_deviation`],
//!   [`entropy`], [`zscore`], [`rankdata`], … They follow `scipy.stats`
//!   conventions: population moments unless noted, and linear interpolation for
//!   [`iqr`] and [`median`], as in NumPy. An empty sample yields `NaN`.
//! - **Correlation** — [`pearsonr`], [`spearmanr`], with two-sided p-values.
//! - **Hypothesis tests** — [`ttest_1samp`]/[`ttest_ind`], [`normaltest`],
//!   [`chisquare`], [`ks_1samp`]/[`ks_2samp`], and the nonparametric
//!   [`mannwhitneyu`]/[`wilcoxon`], each returning `(statistic, p_value)`.
//! - **Lineshapes** — HEP fit shapes: the Crystal Ball (and double-sided),
//!   Breit–Wigner / relativistic Breit–Wigner, the Voigt profile, Novosibirsk,
//!   ARGUS, the bifurcated Gaussian, Moyal, and Landau (see
//!   [below](#lineshapes)).
//! - **Physics helpers** — significance ↔ p-value, weighted means and
//!   measurement combination, Clopper–Pearson / Garwood / Wilson / Agresti–Coull
//!   confidence intervals, and the [`feldman_cousins`] unified interval.
//! - **Resampling** — a seeded percentile [`bootstrap_ci`].
//!
//! Everything is exported at the crate root.
//!
//! # Lineshapes
//!
//! Two conventions, chosen to be the ones physicists fit:
//!
//! - The **peaked shapes** — [`gaussian`], [`crystal_ball`],
//!   [`double_crystal_ball`], [`novosibirsk`], [`bifurcated_gaussian`] — are
//!   normalized to **unit height at the peak**, so you multiply by an amplitude
//!   (a yield or peak height) to fit a spectrum.
//! - The **densities** — [`breit_wigner`], [`relativistic_breit_wigner`],
//!   [`voigtian`], [`moyal`], [`landau`] — are normalized **probability
//!   densities** (unit area). [`argus`] is the conventional (unnormalized)
//!   endpoint background shape.
//!
//! Conventions match RooFit and ROOT and, where they exist, `scipy.stats`
//! (`crystalball`/`cauchy`/`moyal`) and `scipy.special.voigt_profile`.
//! `oxiroot-fit` wraps these as fittable models.
//!
//! # Invalid input
//!
//! Functions that pair two samples element by element return
//! `Result<_, `[`StatError`]`>`. They fail if the samples differ in length,
//! rather than silently pairing up to the shorter one, and some also fail when
//! there are too few observations for the statistic to exist. A `NaN` value in
//! the data is not an error; it propagates to a `NaN` result.
//!
//! Everything else returns plain floats. The incomplete-gamma functions,
//! `erf`/`erfc` and the probabilities built on them, and the
//! Kolmogorov–Smirnov, Mann–Whitney and Wilcoxon tests return `NaN` for `NaN`
//! input rather than looping forever or reporting a spurious p-value. Other
//! functions do not yet treat `NaN` consistently.

// The modules are private: every public item is re-exported at the crate root
// below, so each has one path.
mod correlation;
mod descriptive;
mod distributions;
mod error;
mod hypothesis;
mod lineshapes;
mod physics;
mod resample;
mod special;

pub use correlation::{pearsonr, spearmanr};
pub use descriptive::{
    describe, entropy, gmean, hmean, iqr, kl_divergence, kurtosis, median, median_abs_deviation,
    moment, quantile, rankdata, sem, skew, variation, zscore, Describe,
};
pub use distributions::{
    binom_cdf, binom_sf, poisson_cdf, poisson_sf, ChiSquared, FisherF, Normal, StudentT,
};
pub use error::StatError;
pub use hypothesis::{
    chisquare, ks_1samp, ks_2samp, mannwhitneyu, normaltest, ttest_1samp, ttest_ind, wilcoxon,
};
pub use lineshapes::{
    argus, bifurcated_gaussian, breit_wigner, crystal_ball, double_crystal_ball, gaussian, landau,
    moyal, novosibirsk, relativistic_breit_wigner, voigtian,
};
pub use physics::{
    agresti_coull_interval, clopper_pearson, combine_measurements, feldman_cousins,
    poisson_conf_interval, pvalue_from_significance, significance_from_pvalue, weighted_mean,
    weighted_std, wilson_interval,
};
pub use resample::bootstrap_ci;
pub use special::{
    beta, betainc, betaincinv, betaln, erf, erfc, gammainc, gammaincc, gammaln, ndtri,
};

/// Chi-square survival function `P(X > chi2)` for `X ~ χ²(ndf)` — ROOT's
/// `TMath::Prob`, i.e. the complemented regularized incomplete gamma
/// `Q(ndf/2, chi2/2)`. The goodness-of-fit p-value (a good fit is near 1, a poor
/// one near 0). `ndf == 0` yields 0; `chi2 <= 0` yields 1; a `NaN` `chi2` yields
/// `NaN` and an infinite one yields 0.
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
    fn non_finite_input_terminates_and_propagates() {
        // Regression: both incomplete-gamma kernels exit on a `<= MACHEP` test,
        // which is false for NaN, so a non-finite argument used to spin forever
        // here and in every caller — including `Hist1D::chi2_test`.
        assert!(chi_square_prob(f64::NAN, 3).is_nan());
        assert_eq!(chi_square_prob(f64::INFINITY, 3), 0.0);

        assert!(special::gammainc(f64::NAN, 1.0).is_nan());
        assert!(special::gammainc(1.0, f64::NAN).is_nan());
        assert!(special::gammaincc(f64::NAN, 1.0).is_nan());
        assert!(special::gammaincc(1.0, f64::NAN).is_nan());

        // P(a, +inf) = 1 and Q(a, +inf) = 0, as scipy gives.
        assert_eq!(special::gammainc(2.0, f64::INFINITY), 1.0);
        assert_eq!(special::gammaincc(2.0, f64::INFINITY), 0.0);

        // Reached through erf/erfc, which square their argument.
        assert!(special::erf(f64::NAN).is_nan());
        assert!(special::erfc(f64::NAN).is_nan());
        assert_eq!(special::erf(f64::INFINITY), 1.0);
        assert_eq!(special::erfc(f64::INFINITY), 0.0);
    }

    #[test]
    fn kolmogorov_prob_is_monotone_and_bounded() {
        assert_eq!(kolmogorov_prob(0.0), 1.0);
        assert_eq!(kolmogorov_prob(100.0), 0.0);
        let (a, b) = (kolmogorov_prob(0.5), kolmogorov_prob(1.5));
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b) && a > b);
    }
}
