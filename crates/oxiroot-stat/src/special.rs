//! Special functions (pure `f64`, no dependencies): the log-gamma and
//! regularized incomplete gamma/beta functions, the error function, and the
//! inverse normal CDF. These underpin the distributions and tests in the rest of
//! the crate; names follow `scipy.special` where there is a direct counterpart.

const MACHEP: f64 = 1.1102230246251565e-16;
const BIG: f64 = 4.503599627370496e15;
const BIG_INV: f64 = 2.220446049250313e-16;
/// `ln(f64::MAX)`; an exponent below `-MAX_LOG` underflows to 0.
const MAX_LOG: f64 = 709.782712893384;
/// Above 2⁵³ a `f64` cannot represent `a + 1` exactly, so the incomplete-gamma
/// series and continued fraction, which step a counter from `a` by 1, stop
/// making progress. Near `x ≈ a` there (the only place they are reached), the
/// functions return `NaN` rather than spin.
const MAX_EXACT_STEP: f64 = 9_007_199_254_740_992.0;

/// Iteration budget for the incomplete-gamma series and continued fraction.
///
/// Both converge once their terms start shrinking, but when `x` is close to `a`
/// the series terms only fall off like `exp(−k²/2a)`, so it needs O(√a) steps:
/// about 8√a at `a = 10⁶`, rising slowly to about 11√a near 2⁵³. The budget is
/// `2000 + 20√a`, at least twice that everywhere. It exists only to guarantee
/// termination; running out yields `NaN`, never a truncated sum.
fn iteration_budget(a: f64) -> usize {
    // Float-to-int casts saturate, so a huge `a` cannot overflow here.
    (2000.0 + 20.0 * a.sqrt()) as usize
}

/// Natural log of the absolute gamma function, `ln|Γ(x)|` — `scipy.special.gammaln`.
/// Lanczos approximation (g = 7), with the reflection formula for `x < 0.5`.
#[must_use]
pub fn gammaln(x: f64) -> f64 {
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
        pi.ln() - (pi * x).sin().abs().ln() - gammaln(1.0 - x)
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

/// Regularized lower incomplete gamma `P(a, x)` — `scipy.special.gammainc`.
/// The CDF of a Gamma(`a`) at `x`; `0` for `x <= 0` or `a <= 0`.
///
/// A `NaN` argument yields `NaN`, as does `(+∞, +∞)`; otherwise `P(a, +∞) == 1`
/// and `P(+∞, x) == 0`. For `a >= 2⁵³` with `x` close to `a` the result is `NaN`
/// because `a + 1` is no longer exact there.
#[must_use]
pub fn gammainc(a: f64, x: f64) -> f64 {
    // Ordered before the `<= 0.0` guards: every comparison against NaN is
    // false, so without this NaN would fall through to the series below.
    if a.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if x <= 0.0 || a <= 0.0 {
        return 0.0;
    }
    match (a.is_infinite(), x.is_infinite()) {
        (true, true) => return f64::NAN,
        (false, true) => return 1.0,
        (true, false) => return 0.0,
        (false, false) => {}
    }
    if x > 1.0 && x > a {
        return 1.0 - gammaincc(a, x);
    }
    let ax = a * x.ln() - x - gammaln(a);
    if ax < -MAX_LOG {
        return 0.0;
    }
    if !ax.is_finite() || a >= MAX_EXACT_STEP {
        return f64::NAN;
    }
    let ax = ax.exp();
    let mut r = a;
    let mut c = 1.0;
    let mut ans = 1.0;
    for _ in 0..iteration_budget(a) {
        r += 1.0;
        c *= x / r;
        ans += c;
        if c / ans <= MACHEP {
            return ans * ax / a;
        }
    }
    f64::NAN
}

/// Regularized upper incomplete gamma `Q(a, x) = 1 - P(a, x)` —
/// `scipy.special.gammaincc`. `1` for `x <= 0` or `a <= 0`.
///
/// A `NaN` argument yields `NaN`, as does `(+∞, +∞)`; otherwise `Q(a, +∞) == 0`
/// and `Q(+∞, x) == 1`. For `a >= 2⁵³` with `x` close to `a` the result is `NaN`
/// because `a + 1` is no longer exact there.
#[must_use]
pub fn gammaincc(a: f64, x: f64) -> f64 {
    // See `gammainc`: NaN must be rejected before any ordered comparison.
    if a.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if x <= 0.0 || a <= 0.0 {
        return 1.0;
    }
    match (a.is_infinite(), x.is_infinite()) {
        (true, true) => return f64::NAN,
        (false, true) => return 0.0,
        (true, false) => return 1.0,
        (false, false) => {}
    }
    if x < 1.0 || x < a {
        return 1.0 - gammainc(a, x);
    }
    let ax = a * x.ln() - x - gammaln(a);
    if ax < -MAX_LOG {
        return 0.0;
    }
    if !ax.is_finite() || a >= MAX_EXACT_STEP {
        return f64::NAN;
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
    for _ in 0..iteration_budget(a) {
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
                return ans * ax;
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
    f64::NAN
}

/// The error function `erf(x)` — `scipy.special.erf`. Built from the regularized
/// incomplete gamma: `erf(x) = sign(x) · P(1/2, x²)`.
#[must_use]
pub fn erf(x: f64) -> f64 {
    if x < 0.0 {
        -gammainc(0.5, x * x)
    } else {
        gammainc(0.5, x * x)
    }
}

/// The complementary error function `erfc(x) = 1 - erf(x)` —
/// `scipy.special.erfc`. Uses the tail-accurate `Q(1/2, x²)` for `x >= 0`.
#[must_use]
pub fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        1.0 + gammainc(0.5, x * x)
    } else {
        gammaincc(0.5, x * x)
    }
}

/// Natural log of the beta function, `ln B(a, b)` — `scipy.special.betaln`.
#[must_use]
pub fn betaln(a: f64, b: f64) -> f64 {
    gammaln(a) + gammaln(b) - gammaln(a + b)
}

/// The beta function `B(a, b) = Γ(a)Γ(b)/Γ(a+b)` — `scipy.special.beta`
/// (for `a, b > 0`).
#[must_use]
pub fn beta(a: f64, b: f64) -> f64 {
    betaln(a, b).exp()
}

/// The regularized incomplete beta `I_x(a, b)` — `scipy.special.betainc`. The CDF
/// of a Beta(`a`, `b`) at `x ∈ [0, 1]`. Numerical-Recipes continued fraction with
/// the `I_x(a,b) = 1 - I_{1-x}(b,a)` symmetry for fast convergence.
#[must_use]
pub fn betainc(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front = (gammaln(a + b) - gammaln(a) - gammaln(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * betacf(a, b, x) / a
    } else {
        1.0 - front * betacf(b, a, 1.0 - x) / b
    }
}

/// Lentz's continued fraction for the incomplete beta (Numerical Recipes `betacf`).
fn betacf(a: f64, b: f64, x: f64) -> f64 {
    const FPMIN: f64 = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=200 {
        let m = f64::from(m);
        let m2 = 2.0 * m;
        // Even step.
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        // Odd step.
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 3.0e-15 {
            break;
        }
    }
    h
}

/// Inverse of the regularized incomplete beta in `x`: the `y`-quantile of a
/// Beta(`a`, `b`) — `scipy.special.betaincinv`. Bisection on the monotone
/// [`betainc`].
#[must_use]
pub fn betaincinv(a: f64, b: f64, y: f64) -> f64 {
    if y <= 0.0 {
        return 0.0;
    }
    if y >= 1.0 {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if betainc(a, b, mid) < y {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-15 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Inverse of the standard normal CDF (the quantile / probit function) —
/// `scipy.special.ndtri`. Acklam's rational approximation refined by one Halley
/// step, accurate to full `f64` precision on `(0, 1)`.
// Acklam's published coefficients are kept verbatim.
#[allow(clippy::excessive_precision)]
#[must_use]
pub fn ndtri(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383577518672690e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    const P_LOW: f64 = 0.02425;
    let mut x = if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    // One Halley refinement to reach machine precision.
    let e = 0.5 * erfc(-x / std::f64::consts::SQRT_2) - p;
    let u = e * (2.0 * std::f64::consts::PI).sqrt() * (x * x / 2.0).exp();
    x -= u / (1.0 + x * u / 2.0);
    x
}
