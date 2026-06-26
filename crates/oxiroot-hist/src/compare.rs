//! Statistical comparison of two histograms: ROOT's `TH1::Chi2Test` (the
//! unweighted/unweighted case) and `TH1::KolmogorovTest`.
//!
//! Both need a special function ROOT pulls from its math library — the
//! chi-square survival function (the complemented incomplete gamma `igamc`) and
//! `TMath::KolmogorovProb` — which are reimplemented here so the crate keeps its
//! minimal-dependency footprint.

use oxiroot_io_core::error::{Error, Result};

use crate::th1::TH1;

/// Which weighting scheme a chi-square test assumes for the two histograms
/// (ROOT's `Chi2Test` options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chi2TestKind {
    /// Both histograms are unweighted counts (`"UU"`).
    #[default]
    UnweightedUnweighted,
    /// `self` is unweighted counts, `other` is weighted (`"UW"`).
    UnweightedWeighted,
    /// Both histograms are weighted (`"WW"`).
    WeightedWeighted,
}

/// Result of a chi-square compatibility test ([`TH1::chi2_test`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chi2TestResult {
    /// The test's p-value (probability the two histograms are drawn from the
    /// same distribution): `1 - CDF_{χ²,ndf}(chi2)`.
    pub p_value: f64,
    /// The chi-square statistic.
    pub chi2: f64,
    /// Degrees of freedom (non-empty bins − 1).
    pub ndf: usize,
}

/// Result of a Kolmogorov–Smirnov test ([`TH1::kolmogorov_test`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KsTestResult {
    /// The KS probability that the two histograms come from the same
    /// distribution (1 = indistinguishable, → 0 = clearly different).
    pub prob: f64,
    /// The maximum distance between the two normalized cumulatives (ROOT's
    /// `"M"` option).
    pub distance: f64,
}

impl TH1 {
    /// Pearson chi-square compatibility test against `other` for two *unweighted*
    /// (count) histograms — ROOT's `Chi2Test "UU"`; shorthand for
    /// [`chi2_test_with`](Self::chi2_test_with) with
    /// [`Chi2TestKind::UnweightedUnweighted`].
    ///
    /// # Errors
    /// Returns [`Error::BinningMismatch`] if the two axes are not identical.
    pub fn chi2_test(&self, other: &TH1) -> Result<Chi2TestResult> {
        self.chi2_test_with(other, Chi2TestKind::UnweightedUnweighted)
    }

    /// Chi-square compatibility test against `other` with the given weighting
    /// scheme (ROOT's `Chi2Test` `"UU"`/`"UW"`/`"WW"`). Weighted variants use the
    /// per-bin errors (`bin_error`, i.e. `√Sumw2` or the Poisson `√content`).
    ///
    /// # Errors
    /// Returns [`Error::BinningMismatch`] if the two axes are not identical.
    pub fn chi2_test_with(&self, other: &TH1, kind: Chi2TestKind) -> Result<Chi2TestResult> {
        if !self.xaxis.same_binning(&other.xaxis) {
            return Err(Error::BinningMismatch {
                detail: "chi2_test: histograms have different binning".into(),
            });
        }
        let n = self.xaxis.nbins.max(0) as usize;
        let c1 = |i: usize| self.contents.get(i).copied().unwrap_or(0.0);
        let c2 = |i: usize| other.contents.get(i).copied().unwrap_or(0.0);
        let e1sq = |i: usize| self.bin_error(i).powi(2);
        let e2sq = |i: usize| other.bin_error(i).powi(2);
        let sum1: f64 = (1..=n).map(c1).sum();
        let sum2: f64 = (1..=n).map(c2).sum();

        let mut chi2 = 0.0;
        let mut nonempty = 0i64;
        match kind {
            Chi2TestKind::UnweightedUnweighted => {
                for i in 1..=n {
                    let (a, b) = (c1(i), c2(i));
                    if a == 0.0 && b == 0.0 {
                        continue;
                    }
                    nonempty += 1;
                    let delta = sum2 * a - sum1 * b;
                    chi2 += delta * delta / (a + b);
                }
                if sum1 * sum2 > 0.0 {
                    chi2 /= sum1 * sum2;
                }
            }
            Chi2TestKind::WeightedWeighted => {
                for i in 1..=n {
                    let (s1, s2) = (e1sq(i), e2sq(i));
                    if s1 == 0.0 && s2 == 0.0 {
                        continue;
                    }
                    nonempty += 1;
                    let delta = sum1 * c2(i) - sum2 * c1(i); // W1·c2 − W2·c1
                    let denom = sum1 * sum1 * s2 + sum2 * sum2 * s1;
                    if denom > 0.0 {
                        chi2 += delta * delta / denom;
                    }
                }
            }
            Chi2TestKind::UnweightedWeighted => {
                let sumw2: f64 = (1..=n).map(e2sq).sum();
                for i in 1..=n {
                    let (cnt1, cnt2) = (c1(i), c2(i));
                    let mut s2 = e2sq(i);
                    if cnt1 == 0.0 && cnt2 == 0.0 {
                        continue;
                    }
                    if cnt2 == 0.0 && s2 == 0.0 {
                        if sumw2 > 0.0 && sum2 > 0.0 {
                            s2 = sumw2 / sum2;
                        } else {
                            continue;
                        }
                    }
                    nonempty += 1;
                    // Per-bin quadratic for the estimated probability (ROOT).
                    let var1 = sum2 * cnt2 - sum1 * s2;
                    let var2 = (var1 * var1 + 4.0 * sum2 * sum2 * cnt1 * s2).sqrt();
                    let probb = (var1 + var2) / (2.0 * sum2 * sum2);
                    let (nexp1, nexp2) = (probb * sum1, probb * sum2);
                    if nexp1 > 0.0 {
                        chi2 += (cnt1 - nexp1).powi(2) / nexp1;
                    }
                    if s2 > 0.0 {
                        chi2 += (cnt2 - nexp2).powi(2) / s2;
                    }
                }
            }
        }
        let ndf = (nonempty - 1).max(0) as usize;
        Ok(Chi2TestResult {
            p_value: chi_square_prob(chi2, ndf),
            chi2,
            ndf,
        })
    }

    /// Kolmogorov–Smirnov compatibility test against `other` (ROOT's
    /// `TH1::KolmogorovTest` for unweighted histograms). Errors if the binnings
    /// differ.
    ///
    /// # Errors
    /// Returns [`Error::BinningMismatch`] if the two axes are not identical.
    pub fn kolmogorov_test(&self, other: &TH1) -> Result<KsTestResult> {
        if !self.xaxis.same_binning(&other.xaxis) {
            return Err(Error::BinningMismatch {
                detail: "kolmogorov_test: histograms have different binning".into(),
            });
        }
        let n = self.xaxis.nbins.max(0) as usize;
        let bin = |h: &TH1, i: usize| h.contents.get(i).copied().unwrap_or(0.0);
        let sum1: f64 = (1..=n).map(|i| bin(self, i)).sum();
        let sum2: f64 = (1..=n).map(|i| bin(other, i)).sum();
        if sum1 <= 0.0 || sum2 <= 0.0 {
            return Ok(KsTestResult {
                prob: 0.0,
                distance: 0.0,
            });
        }

        let (s1, s2) = (1.0 / sum1, 1.0 / sum2);
        let (mut rcum1, mut rcum2, mut dfmax) = (0.0, 0.0, 0.0_f64);
        for i in 1..=n {
            rcum1 += bin(self, i) * s1;
            rcum2 += bin(other, i) * s2;
            dfmax = dfmax.max((rcum1 - rcum2).abs());
        }
        // Effective entries reduce to the content sums for unweighted histograms.
        let z = dfmax * (sum1 * sum2 / (sum1 + sum2)).sqrt();
        Ok(KsTestResult {
            prob: kolmogorov_prob(z),
            distance: dfmax,
        })
    }
}

/// Chi-square survival function `P(X > chi2)` for `X ~ χ²(ndf)` — ROOT's
/// `TMath::Prob`, i.e. the complemented regularized incomplete gamma
/// `Q(ndf/2, chi2/2)`. Also the goodness-of-fit p-value behind `FitResult`.
pub(crate) fn chi_square_prob(chi2: f64, ndf: usize) -> f64 {
    if ndf == 0 {
        return 0.0;
    }
    if chi2 <= 0.0 {
        return 1.0;
    }
    igamc(ndf as f64 / 2.0, chi2 / 2.0)
}

/// ROOT's `TMath::KolmogorovProb(z)` — the asymptotic Kolmogorov distribution.
fn kolmogorov_prob(z: f64) -> f64 {
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

// --- Incomplete gamma (Cephes), with a Lanczos ln Γ. ---

const MACHEP: f64 = 1.1102230246251565e-16;
const BIG: f64 = 4.503599627370496e15;
const BIG_INV: f64 = 2.220446049250313e-16;
/// `ln(f64::MAX)`; an exponent below `-MAX_LOG` underflows to 0.
const MAX_LOG: f64 = 709.782712893384;

/// Lanczos approximation to `ln Γ(x)` (g = 7).
fn ln_gamma(x: f64) -> f64 {
    const C: [f64; 9] = [
        0.9999999999998099,
        676.5203681218851,
        -1259.1392167224028,
        771.3234287776531,
        -176.6150291621406,
        12.507343278686905,
        -0.13857109526572012,
        9.984369578019572e-6,
        1.5056327351493116e-7,
    ];
    if x < 0.5 {
        let pi = std::f64::consts::PI;
        pi.ln() - (pi * x).sin().abs().ln() - ln_gamma(1.0 - x)
    } else {
        let x = x - 1.0;
        let t = x + 7.5;
        let mut a = C[0];
        for (i, &c) in C.iter().enumerate().skip(1) {
            a += c / (x + i as f64);
        }
        0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
    }
}

/// Regularized lower incomplete gamma `P(a, x)` (Cephes series).
fn igam(a: f64, x: f64) -> f64 {
    if x <= 0.0 || a <= 0.0 {
        return 0.0;
    }
    if x > 1.0 && x > a {
        return 1.0 - igamc(a, x);
    }
    let ax = a * x.ln() - x - ln_gamma(a);
    if ax < -MAX_LOG {
        return 0.0;
    }
    let ax = ax.exp();
    let mut r = a;
    let mut c = 1.0;
    let mut ans = 1.0;
    loop {
        r += 1.0;
        c *= x / r;
        ans += c;
        if c / ans <= MACHEP {
            break;
        }
    }
    ans * ax / a
}

/// Regularized complemented incomplete gamma `Q(a, x) = 1 - P(a, x)` (Cephes
/// continued fraction).
fn igamc(a: f64, x: f64) -> f64 {
    if x <= 0.0 || a <= 0.0 {
        return 1.0;
    }
    if x < 1.0 || x < a {
        return 1.0 - igam(a, x);
    }
    let ax = a * x.ln() - x - ln_gamma(a);
    if ax < -MAX_LOG {
        return 0.0;
    }
    let ax = ax.exp();

    let mut y = 1.0 - a;
    let mut z = x + y + 1.0;
    let mut c = 0.0;
    let mut pkm2 = 1.0;
    let mut qkm2 = x;
    let mut pkm1 = x + 1.0;
    let mut qkm1 = z * x;
    let mut ans = pkm1 / qkm1;
    loop {
        c += 1.0;
        y += 1.0;
        z += 2.0;
        let yc = y * c;
        let pk = pkm1 * z - pkm2 * yc;
        let qk = qkm1 * z - qkm2 * yc;
        if qk != 0.0 {
            let r = pk / qk;
            let t = ((ans - r) / r).abs();
            ans = r;
            if t <= MACHEP {
                break;
            }
        }
        pkm2 = pkm1;
        pkm1 = pk;
        qkm2 = qkm1;
        qkm1 = qk;
        if pk.abs() > BIG {
            pkm2 *= BIG_INV;
            pkm1 *= BIG_INV;
            qkm2 *= BIG_INV;
            qkm1 *= BIG_INV;
        }
    }
    ans * ax
}
