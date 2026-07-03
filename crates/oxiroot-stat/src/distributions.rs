//! Continuous distributions (`Normal`, `StudentT`, `ChiSquared`, `FisherF`) with
//! `pdf`/`cdf`/`sf`/`ppf`, and the Poisson/Binomial CDF & survival functions —
//! matching the `scipy.stats` distributions of the same name. Built on the error,
//! incomplete-gamma, and incomplete-beta functions in [`crate::special`].

use std::f64::consts::{PI, SQRT_2};

use crate::special::{betainc, erfc, gammainc, gammaincc, gammaln, ndtri};

/// Invert a monotone CDF on `[lo, hi]` by bisection (used for the `ppf`s with no
/// closed form). `lo`/`hi` bracket the support.
fn invert(cdf: impl Fn(f64) -> f64, p: f64, mut lo: f64, mut hi: f64) -> f64 {
    if p <= 0.0 {
        return lo;
    }
    if p >= 1.0 {
        return hi;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo).abs() <= 1e-13 * (1.0 + hi.abs()) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// The normal (Gaussian) distribution `N(mean, std²)` — `scipy.stats.norm`.
#[derive(Debug, Clone, Copy)]
pub struct Normal {
    /// Mean (location).
    pub mean: f64,
    /// Standard deviation (scale).
    pub std: f64,
}

impl Normal {
    /// `N(mean, std²)`.
    #[must_use]
    pub fn new(mean: f64, std: f64) -> Normal {
        Normal { mean, std }
    }
    /// The standard normal `N(0, 1)`.
    #[must_use]
    pub fn standard() -> Normal {
        Normal {
            mean: 0.0,
            std: 1.0,
        }
    }
    /// Probability density at `x`.
    #[must_use]
    pub fn pdf(&self, x: f64) -> f64 {
        let z = (x - self.mean) / self.std;
        (-0.5 * z * z).exp() / (self.std * (2.0 * PI).sqrt())
    }
    /// Cumulative distribution `P(X ≤ x)`.
    #[must_use]
    pub fn cdf(&self, x: f64) -> f64 {
        0.5 * erfc(-(x - self.mean) / (self.std * SQRT_2))
    }
    /// Survival function `P(X > x) = 1 − cdf(x)`.
    #[must_use]
    pub fn sf(&self, x: f64) -> f64 {
        0.5 * erfc((x - self.mean) / (self.std * SQRT_2))
    }
    /// Quantile / inverse CDF.
    #[must_use]
    pub fn ppf(&self, p: f64) -> f64 {
        self.mean + self.std * ndtri(p)
    }
}

/// Student's t distribution with `df` degrees of freedom — `scipy.stats.t`.
#[derive(Debug, Clone, Copy)]
pub struct StudentT {
    /// Degrees of freedom.
    pub df: f64,
}

impl StudentT {
    /// Student's t with `df` degrees of freedom.
    #[must_use]
    pub fn new(df: f64) -> StudentT {
        StudentT { df }
    }
    /// Probability density at `x`.
    #[must_use]
    pub fn pdf(&self, x: f64) -> f64 {
        let v = self.df;
        let lead = gammaln((v + 1.0) / 2.0) - gammaln(v / 2.0) - 0.5 * (v * PI).ln();
        lead.exp() * (1.0 + x * x / v).powf(-(v + 1.0) / 2.0)
    }
    /// Cumulative distribution `P(X ≤ x)`.
    #[must_use]
    pub fn cdf(&self, x: f64) -> f64 {
        let v = self.df;
        let tail = 0.5 * betainc(v / 2.0, 0.5, v / (v + x * x));
        if x <= 0.0 {
            tail
        } else {
            1.0 - tail
        }
    }
    /// Survival function `P(X > x)`.
    #[must_use]
    pub fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }
    /// Quantile / inverse CDF.
    #[must_use]
    pub fn ppf(&self, p: f64) -> f64 {
        invert(|x| self.cdf(x), p, -1.0e6, 1.0e6)
    }
}

/// The chi-squared distribution with `df` degrees of freedom — `scipy.stats.chi2`.
#[derive(Debug, Clone, Copy)]
pub struct ChiSquared {
    /// Degrees of freedom.
    pub df: f64,
}

impl ChiSquared {
    /// Chi-squared with `df` degrees of freedom.
    #[must_use]
    pub fn new(df: f64) -> ChiSquared {
        ChiSquared { df }
    }
    /// Probability density at `x`.
    #[must_use]
    pub fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            return 0.0;
        }
        let k = self.df;
        if x == 0.0 {
            return if k < 2.0 {
                f64::INFINITY
            } else if k == 2.0 {
                0.5
            } else {
                0.0
            };
        }
        let lp = (k / 2.0 - 1.0) * x.ln() - x / 2.0 - (k / 2.0) * 2.0_f64.ln() - gammaln(k / 2.0);
        lp.exp()
    }
    /// Cumulative distribution `P(X ≤ x)`.
    #[must_use]
    pub fn cdf(&self, x: f64) -> f64 {
        gammainc(self.df / 2.0, x / 2.0)
    }
    /// Survival function `P(X > x)`.
    #[must_use]
    pub fn sf(&self, x: f64) -> f64 {
        gammaincc(self.df / 2.0, x / 2.0)
    }
    /// Quantile / inverse CDF.
    #[must_use]
    pub fn ppf(&self, p: f64) -> f64 {
        invert(|x| self.cdf(x), p, 0.0, 1.0e7)
    }
}

/// The F distribution with `(dfn, dfd)` degrees of freedom — `scipy.stats.f`.
#[derive(Debug, Clone, Copy)]
pub struct FisherF {
    /// Numerator degrees of freedom.
    pub dfn: f64,
    /// Denominator degrees of freedom.
    pub dfd: f64,
}

impl FisherF {
    /// F with numerator `dfn` and denominator `dfd` degrees of freedom.
    #[must_use]
    pub fn new(dfn: f64, dfd: f64) -> FisherF {
        FisherF { dfn, dfd }
    }
    /// Probability density at `x`.
    #[must_use]
    pub fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let (m, n) = (self.dfn, self.dfd);
        let lp = 0.5 * (m * (m * x).ln() + n * n.ln() - (m + n) * (m * x + n).ln())
            - x.ln()
            - (gammaln(m / 2.0) + gammaln(n / 2.0) - gammaln((m + n) / 2.0));
        lp.exp()
    }
    /// Cumulative distribution `P(X ≤ x)`.
    #[must_use]
    pub fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let (m, n) = (self.dfn, self.dfd);
        betainc(m / 2.0, n / 2.0, m * x / (m * x + n))
    }
    /// Survival function `P(X > x)`.
    #[must_use]
    pub fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }
    /// Quantile / inverse CDF.
    #[must_use]
    pub fn ppf(&self, p: f64) -> f64 {
        invert(|x| self.cdf(x), p, 0.0, 1.0e7)
    }
}

/// Poisson CDF `P(X ≤ k)` for mean `mu` — `scipy.stats.poisson.cdf`
/// (`= Q(⌊k⌋+1, mu)`).
#[must_use]
pub fn poisson_cdf(k: f64, mu: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    gammaincc(k.floor() + 1.0, mu)
}

/// Poisson survival `P(X > k)` for mean `mu` — `scipy.stats.poisson.sf`.
#[must_use]
pub fn poisson_sf(k: f64, mu: f64) -> f64 {
    if k < 0.0 {
        return 1.0;
    }
    gammainc(k.floor() + 1.0, mu)
}

/// Binomial CDF `P(X ≤ k)` for `n` trials with success probability `p` —
/// `scipy.stats.binom.cdf` (`= I_{1−p}(n−k, k+1)`).
#[must_use]
pub fn binom_cdf(k: f64, n: f64, p: f64) -> f64 {
    let k = k.floor();
    if k < 0.0 {
        return 0.0;
    }
    if k >= n {
        return 1.0;
    }
    betainc(n - k, k + 1.0, 1.0 - p)
}

/// Binomial survival `P(X > k)` for `n` trials with success probability `p` —
/// `scipy.stats.binom.sf`.
#[must_use]
pub fn binom_sf(k: f64, n: f64, p: f64) -> f64 {
    1.0 - binom_cdf(k, n, p)
}
