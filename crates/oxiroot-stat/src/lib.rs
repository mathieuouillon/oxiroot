//! Special functions shared across oxiroot — the statistical distributions
//! behind histogram comparison tests ([`oxiroot_hist`](https://crates.io/crates/oxiroot-hist)'s
//! `chi2_test`/`kolmogorov_test`) and the goodness-of-fit p-value of
//! [`oxiroot_fit`](https://crates.io/crates/oxiroot-fit).
//!
//! Dependency-free leaf crate (pure `f64` math): the incomplete gamma function
//! (Cephes) with a Lanczos `ln Γ`, the χ² survival function, and the asymptotic
//! Kolmogorov distribution. Faithful to ROOT's `TMath::Prob` /
//! `TMath::KolmogorovProb`.

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
    igamc(ndf as f64 / 2.0, chi2 / 2.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chi_square_prob_edges_and_midpoint() {
        assert_eq!(chi_square_prob(10.0, 0), 0.0);
        assert_eq!(chi_square_prob(0.0, 5), 1.0);
        // P(X > ndf) for X ~ chi2(ndf) is ~0.37 near the mean for small ndf.
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
