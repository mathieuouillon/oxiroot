//! Numerical integration and differentiation of a scalar function — the math
//! behind `TF1::Integral` and `TF1::Derivative`. Both take any `Fn(f64) -> f64`,
//! so they work on a parsed formula or any closure.

/// Definite integral of `f` over `[a, b]` — adaptive Gauss–Kronrod (the 15-point
/// rule with its embedded 7-point Gauss estimate for the error), recursively
/// bisecting the interval of largest error. Matches ROOT's `TF1::Integral`
/// (default `ROOT::Math::GaussIntegrator`) to ~10 significant figures for smooth
/// integrands. Reversed limits negate the result.
#[must_use]
pub fn integrate<F: Fn(f64) -> f64>(f: F, a: f64, b: f64) -> f64 {
    if a == b {
        return 0.0;
    }
    if b < a {
        return -integrate(f, b, a);
    }
    let (r0, _) = qk15(&f, a, b);
    let tol = (1e-11 * r0.abs()).max(1e-13);
    adaptive(&f, a, b, tol, 40)
}

fn adaptive<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64, tol: f64, depth: u32) -> f64 {
    let (r, e) = qk15(f, a, b);
    if e <= tol || depth == 0 {
        return r;
    }
    let m = 0.5 * (a + b);
    adaptive(f, a, m, tol * 0.5, depth - 1) + adaptive(f, m, b, tol * 0.5, depth - 1)
}

/// The Gauss–Kronrod 15-point rule on `[a, b]`, returning `(integral, abserr)`.
/// Nodes/weights are the standard QUADPACK `qk15` constants (kept at full
/// published precision); the summation loops index the node arrays by the
/// rule's odd/even positions.
#[allow(clippy::excessive_precision, clippy::needless_range_loop)]
fn qk15<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64) -> (f64, f64) {
    // Abscissae of the 15-point Kronrod rule (positive half + centre).
    const XGK: [f64; 8] = [
        0.991_455_371_120_813_0,
        0.949_107_912_342_759_0,
        0.864_864_423_359_769_1,
        0.741_531_185_599_394_4,
        0.586_087_235_467_691_1,
        0.405_845_151_377_397_2,
        0.207_784_955_007_898_5,
        0.0,
    ];
    // Kronrod weights.
    const WGK: [f64; 8] = [
        0.022_935_322_010_529_2,
        0.063_092_092_629_978_6,
        0.104_790_010_322_250_2,
        0.140_653_259_715_525_9,
        0.169_004_726_639_267_9,
        0.190_350_578_064_785_4,
        0.204_432_940_075_298_9,
        0.209_482_141_084_727_8,
    ];
    // Gauss (7-point) weights, applied at the odd-indexed Kronrod abscissae.
    const WG: [f64; 4] = [
        0.129_484_966_168_869_7,
        0.279_705_391_489_276_7,
        0.381_830_050_505_118_9,
        0.417_959_183_673_469_4,
    ];

    let center = 0.5 * (a + b);
    let half = 0.5 * (b - a);
    let fc = f(center);

    // The 7-point Gauss estimate includes the centre with weight `WG[3]`; the
    // 15-point Kronrod estimate weights it with `WGK[7]`.
    let mut resg = WG[3] * fc;
    let mut resk = WGK[7] * fc;
    let mut resabs = resk.abs();
    let mut fv1 = [0.0f64; 7];
    let mut fv2 = [0.0f64; 7];

    for j in 0..3 {
        let jtw = 2 * j + 1;
        let absc = half * XGK[jtw];
        let f1 = f(center - absc);
        let f2 = f(center + absc);
        fv1[jtw] = f1;
        fv2[jtw] = f2;
        let fsum = f1 + f2;
        resg += WG[j] * fsum;
        resk += WGK[jtw] * fsum;
        resabs += WGK[jtw] * (f1.abs() + f2.abs());
    }
    for j in 0..4 {
        let jtwm1 = 2 * j;
        let absc = half * XGK[jtwm1];
        let f1 = f(center - absc);
        let f2 = f(center + absc);
        fv1[jtwm1] = f1;
        fv2[jtwm1] = f2;
        let fsum = f1 + f2;
        resk += WGK[jtwm1] * fsum;
        resabs += WGK[jtwm1] * (f1.abs() + f2.abs());
    }

    let reskh = resk * 0.5;
    let mut resasc = WGK[7] * (fc - reskh).abs();
    for j in 0..7 {
        resasc += WGK[j] * ((fv1[j] - reskh).abs() + (fv2[j] - reskh).abs());
    }

    let result = resk * half;
    resabs *= half.abs();
    resasc *= half.abs();
    let mut abserr = ((resk - resg) * half).abs();
    if resasc != 0.0 && abserr != 0.0 {
        abserr = resasc * (200.0 * abserr / resasc).powf(1.5).min(1.0);
    }
    const MIN_ERR: f64 = 50.0 * f64::EPSILON;
    if resabs > f64::MIN_POSITIVE / MIN_ERR {
        abserr = abserr.max(MIN_ERR * resabs);
    }
    (result, abserr)
}

/// Derivative of `f` at `x` — a two-level Richardson-extrapolated central
/// difference (`O(h⁴)`), matching ROOT's `TF1::Derivative` accuracy. `h` scales
/// with `|x|` so the step stays meaningful across magnitudes.
#[must_use]
pub fn derivative<F: Fn(f64) -> f64>(f: F, x: f64) -> f64 {
    let h = 1e-3 * (1.0 + x.abs());
    let d1 = (f(x + h) - f(x - h)) / (2.0 * h);
    let d2 = (f(x + 0.5 * h) - f(x - 0.5 * h)) / h;
    (4.0 * d2 - d1) / 3.0
}
