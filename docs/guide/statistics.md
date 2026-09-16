# Statistics

A dependency-free, `f64` statistics core whose numerics track
[`scipy.stats`](https://docs.scipy.org/doc/scipy/reference/stats.html) (and
`scipy.special`) **function-for-function**, verified against it. It began as the
special functions behind oxiroot's histogram comparison
([`chi2_test`](histograms.md)/`kolmogorov_test`) and the goodness-of-fit p-value
of the [fitter](fitting.md), and has grown into a small general-purpose stats
library covering special functions, distributions, descriptive statistics,
correlation, hypothesis tests, HEP confidence intervals, and the bootstrap.

Unlike the [`fit`](fitting.md) and [`plot`](plotting.md) features, statistics is
**always available** — it pulls in no third-party crates, so there is nothing to
enable:

```rust
use oxiroot::stat::*;

let z = Normal::standard().ppf(0.975); // 1.959963984540054  (scipy.stats.norm.ppf)
let (t, p) = ttest_1samp(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], 4.0);
```

It is also published on its own as the [`oxiroot-stat`](https://crates.io/crates/oxiroot-stat)
crate if you want the numerics without the rest of oxiroot.

The commonly-used items are re-exported at `oxiroot::stat` (shown above); the
full set lives in submodules you can also reach directly, e.g.
`oxiroot::stat::distributions::Normal`.

| Module | What it holds |
| --- | --- |
| `special` | `erf`/`erfc`, `gammaln`, incomplete gamma & beta, inverse normal CDF |
| `distributions` | `Normal`, `StudentT`, `ChiSquared`, `FisherF`; Poisson/Binomial |
| `descriptive` | `describe`, moments, `skew`/`kurtosis`, quantiles, `entropy`, … |
| `correlation` | `pearsonr`, `spearmanr` |
| `hypothesis` | t-tests, `normaltest`, `chisquare`, KS, Mann–Whitney, Wilcoxon |
| `physics` | significance ↔ p-value, weighted means, efficiency & Feldman–Cousins intervals |
| `resample` | seeded percentile `bootstrap_ci` |

Every value in the examples below is the number scipy returns for the same
input — the [test suite](https://github.com/mathieuouillon/oxiroot) pins them
against scipy 1.18.

## Special functions

The mathematical primitives everything else is built on — direct analogues of
`scipy.special`.

| Function | Meaning |
| --- | --- |
| `erf(x)` / `erfc(x)` | Error function and its complement |
| `gammaln(x)` | Natural log of the gamma function (Lanczos) |
| `gammainc(a, x)` / `gammaincc(a, x)` | Regularized lower / upper incomplete gamma `P`, `Q` |
| `beta(a, b)` / `betaln(a, b)` | Beta function and its log |
| `betainc(a, b, x)` | Regularized incomplete beta `Iₓ(a, b)` |
| `betaincinv(a, b, y)` | Its inverse (solves `Iₓ = y` for `x`) |
| `ndtri(p)` | Inverse standard-normal CDF `Φ⁻¹` (quantile) |

```rust
use oxiroot::stat::*;

assert!((erf(0.7)            - 0.6778011938374184).abs()  < 1e-9);
assert!((gammaincc(3.0, 5.0) - 0.12465201948308109).abs() < 1e-9);
assert!((ndtri(0.975)        - 1.959963984540054).abs()   < 1e-9);
```

`ndtri` uses Acklam's rational approximation with one Newton–Halley refinement,
so it stays accurate deep into the tails — that is what keeps a 5σ p-value
(`≈ 2.87e-7`) round-tripping (see [Physics helpers](#physics-helpers)).

## Distributions

Each continuous distribution is a small `Copy` struct with `pdf` / `cdf` / `sf`
(survival, `1 − cdf`) / `ppf` (quantile / inverse CDF). The CDFs are exact in
terms of the special functions above; the quantiles without a closed form are
found by bisection.

| Type | scipy | Constructor |
| --- | --- | --- |
| `Normal` | `norm` | `Normal::new(mean, std)` / `Normal::standard()` |
| `StudentT` | `t` | `StudentT::new(df)` |
| `ChiSquared` | `chi2` | `ChiSquared::new(df)` |
| `FisherF` | `f` | `FisherF::new(dfn, dfd)` |

```rust
use oxiroot::stat::*;

let n = Normal::standard();
assert!((n.cdf(1.5) - 0.9331927987311419).abs()  < 1e-9); // P(Z ≤ 1.5)
assert!((n.sf(1.5)  - 0.06680720126885806).abs() < 1e-9); // one-sided p

// A two-sided 95% critical value for Student's t with 5 d.o.f.
let t_crit = StudentT::new(5.0).ppf(0.975);               // 2.5705818356…

// χ²(4) 95th percentile — the usual "is my fit bad?" threshold.
let chi2_crit = ChiSquared::new(4.0).ppf(0.95);           // 9.4877290…
```

Discrete tails come as free functions (the CDF is a regularized incomplete gamma
or beta, so no struct is needed):

```rust
use oxiroot::stat::*;

let p_le3 = poisson_cdf(3.0, 2.5);        // P(X ≤ 3), mean 2.5 → 0.75757…
let p_gt3 = binom_sf(3.0, 10.0, 0.3);     // P(X > 3), n=10 p=0.3 → 0.35038…
```

## Descriptive statistics

Summaries over a plain `&[f64]`, following scipy's conventions — **population**
moments unless noted, `ddof = 1` where scipy does (`sem`, `describe.variance`),
and NumPy-style linear-interpolation quantiles. Empty input yields `NaN`.

| Function | Notes |
| --- | --- |
| `describe(&data) -> Describe` | one-pass `nobs`/`min`/`max`/`mean`/`variance`/`skewness`/`kurtosis` |
| `gmean` / `hmean` | geometric / harmonic mean |
| `moment(&data, k)` | `k`-th central moment |
| `skew(&data, bias)` | Fisher–Pearson skewness (`bias=false` applies the sample correction) |
| `kurtosis(&data, fisher, bias)` | `fisher=true` subtracts 3 (excess) |
| `sem` / `variation` | standard error of the mean / coefficient of variation |
| `quantile(&data, q)` / `median` / `iqr` | order statistics (linear interpolation) |
| `median_abs_deviation(&data, normal)` | MAD, optionally scaled to estimate σ |
| `entropy` / `kl_divergence` | Shannon entropy / KL divergence (natural log) |
| `zscore` / `rankdata` | per-element z-scores / average-tie ranks |

```rust
use oxiroot::stat::*;

let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];

let d = describe(&data);
assert_eq!(d.nobs, 8);
assert!((d.mean     - 5.0).abs()               < 1e-12);
assert!((d.variance - 4.571428571428571).abs() < 1e-9); // ddof = 1
assert!((d.skewness - 0.65625).abs()           < 1e-12);

assert!((median(&data) - 4.5).abs() < 1e-12);
assert!((iqr(&data)    - 1.5).abs() < 1e-12);
assert!((sem(&data)    - 0.7559289460184544).abs() < 1e-9);
assert_eq!(rankdata(&data), vec![1.0, 3.0, 3.0, 3.0, 5.5, 5.5, 7.0, 8.0]);
```

## Correlation

Both correlations return the coefficient **and** its two-sided p-value as
`Ok((r, p))` — matching `scipy.stats.pearsonr` / `spearmanr` — or an error for
input they cannot use (below).

```rust
use oxiroot::stat::*;

let x = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10.];
let y = [2., 1., 4., 3., 6., 5., 8., 7., 10., 9.];

let (r, p) = pearsonr(&x, &y)?;   // (0.93939…, 5.48e-05)
let (rho, _) = spearmanr(&x, &y)?; // 0.93939… (rank correlation)
# Ok::<(), StatError>(())
```

## Paired samples and errors

Functions that pair two samples element by element — `pearsonr`, `spearmanr`,
`chisquare`, `wilcoxon`, `kl_divergence`, `weighted_mean`, `weighted_std` and
`combine_measurements` — return `Result<_, StatError>`. They fail with
`StatError::LengthMismatch` when the two samples differ in length, rather than
silently pairing up to the shorter one. Some also fail with
`StatError::TooFewObservations` when there is nothing to compute from: the
correlations below two pairs, `chisquare` below two categories, and `wilcoxon`
on empty samples.

This is sometimes stricter than `scipy`, which raises for mismatched lengths and
for `pearsonr` below two pairs, but returns `NaN` for `spearmanr` below two
pairs, for a one-category `chisquare` and for an empty `wilcoxon`, and
broadcasts a length-1 second array in `chisquare` and `entropy`. Each function's
documentation spells out its case.

```rust
use oxiroot::stat::*;

let err = pearsonr(&[1., 2., 3., 4., 5.], &[1., 2., 3.]).unwrap_err();
assert_eq!(err, StatError::LengthMismatch { left: 5, right: 3 });
```

A `NaN` *value* in the data is not an error: it propagates to a `NaN` result,
as with `scipy`'s default `nan_policy`. The incomplete-gamma functions,
`erf`/`erfc` and the probabilities built on them, and the Kolmogorov–Smirnov,
Mann–Whitney and Wilcoxon tests all return `NaN` for `NaN` input; other functions
do not yet treat `NaN` consistently.

`StatError` implements `std::error::Error`, so it combines with file IO through a
boxed error. The prelude's `Result` alias takes an optional error type, so this
works after `use oxiroot::prelude::*`:

```rust
use oxiroot::prelude::*;
use oxiroot::stat;

fn correlate(path: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let file = RFile::open(path)?;          // oxiroot::Error
    let h = TH1::read_root(&file, "h")?;
    let x: Vec<f64> = (1..=h.xaxis.nbins as usize).map(|i| h.bin_center(i)).collect();
    let y = &h.contents[1..=x.len()];
    let (r, _) = stat::pearsonr(&x, y)?;    // StatError
    Ok(r)
}
```

## Hypothesis tests

Every test returns `(statistic, p_value)`; the paired `chisquare` and
`wilcoxon` wrap it in a `Result`, as above.

| Function | Test |
| --- | --- |
| `ttest_1samp(&data, mu)` | one-sample t-test of the mean |
| `ttest_ind(&a, &b)` | independent two-sample t-test (equal variance) |
| `normaltest(&data)` | D'Agostino–Pearson omnibus normality test |
| `chisquare(&f_obs, &f_exp)` | Pearson goodness-of-fit |
| `ks_1samp(&data, cdf)` | one-sample Kolmogorov–Smirnov vs a reference CDF |
| `ks_2samp(&a, &b)` | two-sample Kolmogorov–Smirnov |
| `mannwhitneyu(&x, &y)` | Mann–Whitney U rank-sum (nonparametric) |
| `wilcoxon(&x, &y)` | Wilcoxon signed-rank (paired, nonparametric) |

```rust
use oxiroot::stat::*;

let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
let (t, p) = ttest_1samp(&data, 4.0);   // (1.32287…, 0.22745…)
if p < 0.05 { /* reject H0: mean == 4 */ }

// Nonparametric two-sample comparisons.
let (u, pu) = mannwhitneyu(&[0.5, 1.2, 2.3, 3.1], &[2.1, 3.3, 4.4, 5.5]);

// One-sample KS against the standard normal.
let (d, pks) = ks_1samp(&data, |x| Normal::standard().cdf(x));
```

!!! note "Kolmogorov p-values are asymptotic"
    `ks_1samp`/`ks_2samp` report the **asymptotic** Kolmogorov survival of
    `√n · D` — the same convention as ROOT's `KolmogorovTest`. This differs from
    scipy's *exact* small-sample p-value for `ks_2samp`; the statistic `D` is
    identical, and `ks_1samp` matches scipy's asymptotic mode. This is a
    deliberate, documented choice so histogram and array tests agree.

## Physics helpers

HEP conventions: Gaussian significance in σ, inverse-variance combination of
measurements, and the frequentist confidence intervals used for efficiencies and
counting experiments.

**Significance ↔ p-value** (the "nσ" of a discovery):

```rust
use oxiroot::stat::*;

let p = pvalue_from_significance(5.0);      // 2.8665e-07  (one-sided)
let z = significance_from_pvalue(p);        // back to 5.0, accurate in the tail
```

**Combining measurements** by inverse-variance weighting:

```rust
use oxiroot::stat::*;

let (mean, err) = combine_measurements(&[10.0, 12.0], &[1.0, 2.0])?;
// mean = 10.4, err = 0.8944…  (weights 1/σ²; err = 1/√Σw)
# Ok::<(), StatError>(())
```

**Interval estimators.** For an efficiency `k/n` (or a Poisson count) at
confidence level `cl`, pick the coverage you want:

| Function | Interval |
| --- | --- |
| `clopper_pearson(k, n, cl)` | exact binomial ("Clopper–Pearson", ROOT `TEfficiency` default) |
| `wilson_interval(k, n, cl)` | Wilson score |
| `agresti_coull_interval(k, n, cl)` | Agresti–Coull (adjusted Wald) |
| `poisson_conf_interval(k, cl)` | Garwood exact Poisson mean |
| `feldman_cousins(n_obs, background, cl)` | Feldman–Cousins unified Poisson (with background) |

```rust
use oxiroot::stat::*;

// 8 successes in 10 trials, 95% CL.
let (lo, hi) = clopper_pearson(8.0, 10.0, 0.95);   // (0.4439…, 0.9748…)

// Feldman–Cousins 90% CL, no background, 3 observed.
let (lo, hi) = feldman_cousins(3, 0.0, 0.90);      // ≈ (1.10, 7.42)
```

The Feldman–Cousins belt is built by the likelihood-ratio ordering on a fine μ
grid; its `background = 0` limits reproduce the canonical
[Feldman & Cousins (1998)](https://arxiv.org/abs/physics/9711021) Table IV
values (e.g. `feldman_cousins(0, 0.0, 0.90)` → upper limit `2.44`).

## HEP lineshapes

The `lineshapes` module has the peak, tail, and background functions physicists
fit to mass and energy-loss spectra. The **peaked shapes** are normalized to unit
peak; `breit_wigner`, `relativistic_breit_wigner`, `voigtian`, `moyal`, and
`landau` are **unit-area densities**; `argus` is the conventional (unnormalized)
background.

| Function | Shape |
| --- | --- |
| `crystal_ball(x, mean, sigma, alpha, n)` | Gaussian core + power-law tail (RooFit `RooCBShape`) |
| `double_crystal_ball(x, mean, sigma, …)` | independent power-law tail on each side |
| `breit_wigner(x, mean, gamma)` / `relativistic_breit_wigner` | Lorentzian / PDG resonance |
| `voigtian(x, mean, sigma, gamma)` | Gaussian ⊗ Lorentzian (resolution-broadened) |
| `novosibirsk(x, peak, sigma, tail)` | asymmetric peak |
| `bifurcated_gaussian(x, mean, sigma_lo, sigma_hi)` | different width each side |
| `argus(x, m0, c, p)` | kinematic-endpoint background |
| `moyal` / `landau(x, mean, sigma)` | energy-loss densities (`landau` = ROOT's `TMath::Landau`) |

Conventions match RooFit / ROOT and, where they exist, `scipy` — `crystal_ball`
equals `scipy.stats.crystalball`, `breit_wigner` equals `scipy.stats.cauchy`,
`voigtian` equals `scipy.special.voigt_profile`, `moyal` equals
`scipy.stats.moyal`.

```rust
use oxiroot::stat::*;

let y = crystal_ball(101.0, 100.0, 2.5, 1.5, 3.0); // CB value at x = 101
let v = voigtian(0.0, 0.0, 1.0, 0.5);              // 0.27896 (== voigt_profile)
```

Every peak is also a ready-to-fit [`Model`](fitting.md) with an amplitude
parameter — `Model::crystal_ball`, `voigtian`, `double_crystal_ball`,
`breit_wigner`, `novosibirsk`, `argus` — so you fit one to a histogram directly
(`estimate_from` seeds the Gaussian core):

```rust
use oxiroot::prelude::*;

let mut h = Hist::reg(80, 90.0, 110.0).double().named("mass");
// … fill h …
let model = Model::crystal_ball("cb").estimate_from(&h).lower_limit("sigma", 0.0);
let result = h.fit(&model);
```

## Bootstrap

`bootstrap_ci` gives a percentile confidence interval for **any** statistic of a
sample, using a small seeded PRNG (SplitMix64) so a run is reproducible without a
`rand` dependency.

```rust
use oxiroot::stat::*;

let x = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10.];
let mean = |d: &[f64]| d.iter().sum::<f64>() / d.len() as f64;

// 4000 resamples, 95% CI, seed 42 — same seed ⇒ same interval.
let (lo, hi) = bootstrap_ci(&x, mean, 4000, 0.95, 42);
```

Pass any closure `Fn(&[f64]) -> f64` — the median, a trimmed mean, a ratio,
whatever you need an interval for.

## Relationship to the histogram tests

The histogram [`chi2_test`/`kolmogorov_test`](histograms.md) and the fit
[goodness-of-fit p-value](fitting.md) call straight into this crate
(`chi_square_prob` is `gammaincc(ndf/2, χ²/2)`; `kolmogorov_prob` is ROOT's
`TMath::KolmogorovProb`), so a p-value computed from a `TH1` and one computed
from a raw `&[f64]` use exactly the same numerics.
