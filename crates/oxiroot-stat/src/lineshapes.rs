//! HEP lineshapes for fitting mass and energy-loss spectra: the Crystal Ball and
//! its double-sided variant, Breit–Wigner / relativistic Breit–Wigner, the Voigt
//! profile, Novosibirsk, ARGUS, the bifurcated Gaussian, Moyal, and Landau.
//!
//! Two conventions, chosen to be the ones physicists actually fit:
//!
//! - The **peaked shapes** — [`gaussian`], [`crystal_ball`],
//!   [`double_crystal_ball`], [`novosibirsk`], [`bifurcated_gaussian`] — are
//!   normalized to **unit height at the peak**, so you multiply by an amplitude
//!   (a yield / peak height) to fit a spectrum.
//! - The **densities** — [`breit_wigner`], [`relativistic_breit_wigner`],
//!   [`voigtian`], [`moyal`], [`landau`] — are normalized **probability
//!   densities** (unit area). [`argus`] is the conventional (unnormalized)
//!   endpoint background shape.
//!
//! Conventions match RooFit / ROOT and, where they exist, `scipy.stats`
//! (`crystalball`/`cauchy`/`moyal`) and `scipy.special.voigt_profile`; the
//! `oxiroot-fit` crate wraps these as fittable [`Model`](../../oxiroot_fit/struct.Model.html)s.

use std::f64::consts::{PI, SQRT_2};

/// A tiny floor so a zero width never divides by zero.
const TINY: f64 = f64::EPSILON;

/// A Gaussian normalized to **unit peak**: `exp(-½·((x−mean)/sigma)²)`.
#[must_use]
pub fn gaussian(x: f64, mean: f64, sigma: f64) -> f64 {
    let s = if sigma == 0.0 { TINY } else { sigma };
    let t = (x - mean) / s;
    (-0.5 * t * t).exp()
}

/// The **Crystal Ball** function: a Gaussian core with a power-law tail on the
/// low side, matched to be continuous and continuously differentiable (RooFit's
/// `RooCBShape`). Normalized to **unit peak**. `alpha` is where the tail begins
/// (in units of `sigma`) and `n` its power; `alpha < 0` puts the tail on the
/// high side instead. Equals `scipy.stats.crystalball` up to its area
/// normalization.
#[must_use]
pub fn crystal_ball(x: f64, mean: f64, sigma: f64, alpha: f64, n: f64) -> f64 {
    let s = if sigma == 0.0 { TINY } else { sigma };
    let mut t = (x - mean) / s;
    if alpha < 0.0 {
        t = -t; // tail on the high side
    }
    let a = alpha.abs().max(TINY);
    if t > -a {
        (-0.5 * t * t).exp()
    } else {
        let big_a = (n / a).powf(n) * (-0.5 * a * a).exp();
        let big_b = n / a - a;
        big_a * (big_b - t).powf(-n)
    }
}

/// A **double-sided Crystal Ball**: a Gaussian core with an independent power-law
/// tail on each side — the radiative-tail shape for mass peaks. `alpha_lo`/`n_lo`
/// govern the low-side tail, `alpha_hi`/`n_hi` the high side. Normalized to
/// **unit peak**.
#[must_use]
pub fn double_crystal_ball(
    x: f64,
    mean: f64,
    sigma: f64,
    alpha_lo: f64,
    n_lo: f64,
    alpha_hi: f64,
    n_hi: f64,
) -> f64 {
    let s = if sigma == 0.0 { TINY } else { sigma };
    let t = (x - mean) / s;
    let alo = alpha_lo.abs().max(TINY);
    let ahi = alpha_hi.abs().max(TINY);
    if t < -alo {
        let big_a = (n_lo / alo).powf(n_lo) * (-0.5 * alo * alo).exp();
        let big_b = n_lo / alo - alo;
        big_a * (big_b - t).powf(-n_lo)
    } else if t > ahi {
        let big_a = (n_hi / ahi).powf(n_hi) * (-0.5 * ahi * ahi).exp();
        let big_b = n_hi / ahi - ahi;
        big_a * (big_b + t).powf(-n_hi)
    } else {
        (-0.5 * t * t).exp()
    }
}

/// A **bifurcated Gaussian**: a Gaussian with a different width on each side of
/// the peak (`sigma_lo` below `mean`, `sigma_hi` above). Normalized to **unit
/// peak**.
#[must_use]
pub fn bifurcated_gaussian(x: f64, mean: f64, sigma_lo: f64, sigma_hi: f64) -> f64 {
    let s = if x < mean { sigma_lo } else { sigma_hi };
    let s = if s == 0.0 { TINY } else { s };
    let t = (x - mean) / s;
    (-0.5 * t * t).exp()
}

/// The (non-relativistic) **Breit–Wigner** resonance line — a Lorentzian of full
/// width `gamma` (FWHM) centred at `mean`, normalized to **unit area**. Equal to
/// `scipy.stats.cauchy` with `scale = gamma/2`.
#[must_use]
pub fn breit_wigner(x: f64, mean: f64, gamma: f64) -> f64 {
    let hw = gamma.abs().max(TINY) / 2.0;
    (hw / PI) / ((x - mean).powi(2) + hw * hw)
}

/// The **relativistic Breit–Wigner** for a resonance of mass `mean` and width
/// `gamma` (the PDG / ROOT form `∝ 1/((x²−M²)² + M²Γ²)`), normalized to **unit
/// area**.
#[must_use]
pub fn relativistic_breit_wigner(x: f64, mean: f64, gamma: f64) -> f64 {
    let m = mean;
    let g = gamma.abs().max(TINY);
    let gam = (m * m * (m * m + g * g)).sqrt();
    let k = (2.0 * SQRT_2 * m * g * gam) / (PI * (m * m + gam).sqrt());
    k / ((x * x - m * m).powi(2) + m * m * g * g)
}

/// The **Voigt profile** — a Lorentzian of half-width `gamma` (HWHM) convolved
/// with a Gaussian of width `sigma`, centred at `mean` and normalized to **unit
/// area**. The resolution-broadened resonance shape. Equal to
/// `scipy.special.voigt_profile(x − mean, sigma, gamma)`.
#[must_use]
pub fn voigtian(x: f64, mean: f64, sigma: f64, gamma: f64) -> f64 {
    let s = sigma.abs().max(TINY);
    let inv = 1.0 / (s * SQRT_2);
    let (wr, _wi) = faddeeva_w((x - mean) * inv, gamma.abs() * inv);
    wr * inv / PI.sqrt()
}

/// The **Novosibirsk** function — an asymmetric peak whose skew is set by `tail`
/// (`0` recovers a Gaussian of width `sigma` at `peak`). Normalized to **unit
/// peak** at `peak`.
#[must_use]
pub fn novosibirsk(x: f64, peak: f64, sigma: f64, tail: f64) -> f64 {
    let s = if sigma == 0.0 { TINY } else { sigma };
    if tail.abs() < 1e-7 {
        let t = (x - peak) / s;
        return (-0.5 * t * t).exp();
    }
    let qa = tail * 4.0_f64.ln().sqrt();
    let qb = qa.sinh() / qa;
    let qx = (x - peak) / s * qb;
    let qy = 1.0 + tail * qx;
    if qy <= 1e-7 {
        return 0.0;
    }
    let arg = qy.ln() / tail;
    (-0.5 * arg * arg).exp()
}

/// The **ARGUS** background shape (the phase-space endpoint used in B physics):
/// `x·√(1 − (x/m0)²)·exp(c·(1 − (x/m0)²))` for `0 < x < m0`, else `0`; `p`
/// generalizes the `√` exponent (ROOT's default `0.5`). Returned **unnormalized**.
#[must_use]
pub fn argus(x: f64, m0: f64, c: f64, p: f64) -> f64 {
    if x <= 0.0 || x >= m0 {
        return 0.0;
    }
    let z = 1.0 - (x / m0).powi(2);
    x * z.powf(p) * (c * z).exp()
}

/// The **Moyal** distribution — a closed-form approximation to Landau's
/// energy-loss shape — as a **unit-area** density at location `mean`, scale
/// `sigma`. Equal to `scipy.stats.moyal`.
#[must_use]
pub fn moyal(x: f64, mean: f64, sigma: f64) -> f64 {
    let s = sigma.abs().max(TINY);
    let t = (x - mean) / s;
    (-0.5 * (t + (-t).exp())).exp() / (s * (2.0 * PI).sqrt())
}

/// The **Landau** energy-loss distribution as a **unit-area** density with most
/// probable value shifted to `mean` and scale `sigma` — ROOT's `TMath::Landau`
/// (the Kölbig–Schorr `denlan` approximation).
#[must_use]
pub fn landau(x: f64, mean: f64, sigma: f64) -> f64 {
    let s = sigma.abs().max(TINY);
    landau_pdf((x - mean) / s) / s
}

// --- helpers ----------------------------------------------------------------

/// The standard Landau density `denlan` (Kölbig & Schorr, CERNlib G110), matching
/// `ROOT::Math::landau_pdf(v)` for scale 1, location 0.
#[allow(clippy::excessive_precision)]
fn landau_pdf(v: f64) -> f64 {
    const P1: [f64; 5] = [
        0.4259894875,
        -0.1249762550,
        0.03984243700,
        -0.006298287635,
        0.001511162253,
    ];
    const Q1: [f64; 5] = [
        1.0,
        -0.3388260629,
        0.09594393323,
        -0.01608042283,
        0.003778942063,
    ];
    const P2: [f64; 5] = [
        0.1788541609,
        0.1173957403,
        0.01488850518,
        -0.001394989411,
        0.0001283617211,
    ];
    const Q2: [f64; 5] = [
        1.0,
        0.7428795082,
        0.3153932961,
        0.06694219548,
        0.008790609714,
    ];
    const P3: [f64; 5] = [
        0.1788544503,
        0.09359161662,
        0.006325387654,
        0.00006611667319,
        -0.000002031049101,
    ];
    const Q3: [f64; 5] = [
        1.0,
        0.6097809921,
        0.2560616665,
        0.04746722384,
        0.006957301675,
    ];
    const P4: [f64; 5] = [
        0.9874054407,
        118.6723273,
        849.2794360,
        -743.7792444,
        427.0262186,
    ];
    const Q4: [f64; 5] = [1.0, 106.8615961, 337.6496214, 2016.712389, 1597.063511];
    const P5: [f64; 5] = [
        1.003675074,
        167.5702434,
        4789.711289,
        21217.86767,
        -22324.94910,
    ];
    const Q5: [f64; 5] = [1.0, 156.9424537, 3745.310488, 9834.698876, 66924.28357];
    const P6: [f64; 5] = [
        1.000827619,
        664.9143136,
        62972.92665,
        475554.6998,
        -5743609.109,
    ];
    const Q6: [f64; 5] = [1.0, 651.4101098, 56974.73333, 165917.4725, -2815759.939];
    const A1: [f64; 3] = [0.04166666667, -0.01996527778, 0.02709538966];
    const A2: [f64; 2] = [-1.845568670, -4.284640743];

    // Horner evaluation of a degree-4 polynomial `c[0] + c[1]v + … + c[4]v⁴`.
    let poly = |c: &[f64; 5], v: f64| c[0] + (c[1] + (c[2] + (c[3] + c[4] * v) * v) * v) * v;

    if v < -5.5 {
        let u = (v + 1.0).exp();
        if u < 1e-10 {
            return 0.0;
        }
        let ue = (-1.0 / u).exp();
        let us = u.sqrt();
        0.3989422803 * (ue / us) * (1.0 + (A1[0] + (A1[1] + A1[2] * u) * u) * u)
    } else if v < -1.0 {
        let u = (-v - 1.0).exp();
        (-u).exp() * u.sqrt() * poly(&P1, v) / poly(&Q1, v)
    } else if v < 1.0 {
        poly(&P2, v) / poly(&Q2, v)
    } else if v < 5.0 {
        poly(&P3, v) / poly(&Q3, v)
    } else if v < 12.0 {
        let u = 1.0 / v;
        u * u * poly(&P4, u) / poly(&Q4, u)
    } else if v < 50.0 {
        let u = 1.0 / v;
        u * u * poly(&P5, u) / poly(&Q5, u)
    } else if v < 300.0 {
        let u = 1.0 / v;
        u * u * poly(&P6, u) / poly(&Q6, u)
    } else {
        let u = 1.0 / (v - v * v.ln() / (v + 1.0));
        u * u * (1.0 + (A2[0] + A2[1] * u) * u)
    }
}

// Minimal complex arithmetic (re, im) for the Faddeeva evaluation.
type C = (f64, f64);
fn cadd(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}
fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn cdiv(a: C, b: C) -> C {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}
/// `real + T`.
fn radd(r: f64, t: C) -> C {
    (r + t.0, t.1)
}
/// `real − T`.
fn rsub(r: f64, t: C) -> C {
    (r - t.0, -t.1)
}
/// `exp(z)`.
fn cexp(z: C) -> C {
    let e = z.0.exp();
    (e * z.1.cos(), e * z.1.sin())
}

/// The Faddeeva function `w(z) = exp(−z²)·erfc(−iz)` for the upper half plane
/// (`z = x + iy`, `y ≥ 0`) via Humlicek's (1982) `w4` rational approximation —
/// accurate to a few ×10⁻⁴, ample for a lineshape. Returns `(Re w, Im w)`.
#[allow(clippy::excessive_precision)]
fn faddeeva_w(x: f64, y: f64) -> C {
    let t: C = (y, -x); // T = y − i x
    let s = x.abs() + y;
    if s >= 15.0 {
        // Region I.
        cdiv(
            (0.5641896 * t.0, 0.5641896 * t.1),
            cadd((0.5, 0.0), cmul(t, t)),
        )
    } else if s >= 5.5 {
        // Region II.
        let t2 = cmul(t, t);
        let num = cmul(t, radd(1.410474, (0.5641896 * t2.0, 0.5641896 * t2.1)));
        let den = cadd((0.75, 0.0), cmul(t2, radd(3.0, t2)));
        cdiv(num, den)
    } else if y >= 0.195 * x.abs() - 0.176 {
        // Region III.
        let num = radd(
            16.4955,
            cmul(
                t,
                radd(
                    20.20933,
                    cmul(
                        t,
                        radd(
                            11.96482,
                            cmul(t, radd(3.778987, (0.5642236 * t.0, 0.5642236 * t.1))),
                        ),
                    ),
                ),
            ),
        );
        let den = radd(
            16.4955,
            cmul(
                t,
                radd(
                    38.82363,
                    cmul(
                        t,
                        radd(
                            39.27121,
                            cmul(t, radd(21.69274, cmul(t, radd(6.699398, t)))),
                        ),
                    ),
                ),
            ),
        );
        cdiv(num, den)
    } else {
        // Region IV.
        let u = cmul(t, t);
        let num = rsub(
            36183.31,
            cmul(
                u,
                rsub(
                    3321.9905,
                    cmul(
                        u,
                        rsub(
                            1540.787,
                            cmul(
                                u,
                                rsub(
                                    219.0313,
                                    cmul(
                                        u,
                                        rsub(
                                            35.76683,
                                            cmul(u, rsub(1.320522, (0.56419 * u.0, 0.56419 * u.1))),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let den = rsub(
            32066.6,
            cmul(
                u,
                rsub(
                    24322.84,
                    cmul(
                        u,
                        rsub(
                            9022.228,
                            cmul(
                                u,
                                rsub(
                                    2186.181,
                                    cmul(
                                        u,
                                        rsub(
                                            364.2191,
                                            cmul(u, rsub(61.57037, cmul(u, rsub(1.841439, u)))),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        cadd(
            cexp(u),
            (-cmul(t, cdiv(num, den)).0, -cmul(t, cdiv(num, den)).1),
        )
    }
}
