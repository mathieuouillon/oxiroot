//! Standalone `TF1`/`TF2`/`TF3` functions: build several formula forms (a ROOT
//! `gaus` shortcut, `expo`, a `pol2` polynomial, and a hand-written expression),
//! evaluate them in pure Rust (`eval`), do calculus (`integral`/`derivative`),
//! scan a grid to find a peak and a crossing, then write them to a ROOT file and
//! read one back — a round-trip that ROOT C++ and uproot also read.
//!
//! ```sh
//! cargo run -p oxiroot --example functions
//! ```

use oxiroot::prelude::*;

fn main() -> oxiroot::Result<()> {
    // --- Four 1-D functions, one per formula flavour. --------------------------
    // A hand-written expression: a decaying sine. Parameters are [0], [1], …; the
    // free variable is `x`. `with_params` fills them in builder style.
    let damped = TF1::new("damped", "[0]*exp(-[1]*x)*sin([2]*x)", 0.0, 10.0)?
        .with_params(vec![5.0, 0.3, 2.0]);

    // ROOT's `gaus` shortcut expands to `[0]*exp(-0.5*((x-[1])/[2])^2)` — a
    // Gaussian with (constant, mean, sigma). The engine knows the parameter count.
    let gauss = TF1::new("gauss", "gaus", -5.0, 5.0)?.with_params(vec![2.0, 0.5, 1.2]);

    // `expo` is `exp([0] + [1]*x)`; `pol2` is `[0] + [1]*x + [2]*x^2`.
    let decay = TF1::new("decay", "expo", 0.0, 5.0)?.with_params(vec![1.0, -0.7]);
    let parab = TF1::new("parab", "pol2", -3.0, 3.0)?.with_params(vec![1.0, -2.0, 1.0]);

    println!("Four TF1s (formula shown in ROOT's canonical [pN] form):");
    for f in [&damped, &gauss, &decay, &parab] {
        // `title()` is the source as typed; `formula()` is ROOT's [pN] form.
        println!(
            "  {:<7} {:<34} npar={}  eval(1.0) = {:+.6}",
            f.name(),
            f.formula(),
            f.npar(),
            f.eval(1.0),
        );
    }

    // --- Calculus: these are real callable functions, so we can integrate ------
    // and differentiate them numerically (adaptive Gauss–Kronrod / Richardson).
    // integral(a,b) is the signed area under the curve; derivative(x) is the slope.
    println!("\nCalculus on `damped` = 5·exp(-0.3x)·sin(2x):");
    println!(
        "  integral(0,10) = {:+.6}   (net signed area over the range)",
        damped.integral(0.0, 10.0)
    );
    println!(
        "  derivative(0)  = {:+.6}   (slope at x=0; here 5·2 = 10)",
        damped.derivative(0.0)
    );
    // A parabola integrates and differentiates to values you can check by hand:
    // ∫_{-1}^{1} (1 - 2x + x²) dx = 2 + 0 + 2/3 = 2.6667, and d/dx = -2 + 2x → 0 at x=1.
    println!("Check on `parab` = 1 - 2x + x²:");
    println!(
        "  integral(-1,1) = {:.6}   (analytic 8/3 = 2.666667)",
        parab.integral(-1.0, 1.0)
    );
    println!(
        "  derivative(1)  = {:.6}   (analytic -2 + 2·1 = 0)",
        parab.derivative(1.0)
    );

    // --- Numeric use: scan the grid to locate features of a callable function. -
    // Find the maximum of the damped sine by evaluating on a fine grid, and the
    // Gaussian's half-maximum crossing on its right flank (where it drops to 1.0,
    // half of its peak constant 2.0). Both are ordinary `eval` loops.
    let n = 2_000;
    let (mut x_max, mut y_max) = (0.0, f64::NEG_INFINITY);
    for i in 0..=n {
        let x = 10.0 * i as f64 / n as f64;
        let y = damped.eval(x);
        if y > y_max {
            (x_max, y_max) = (x, y);
        }
    }
    println!("\nGrid scan of `damped` over [0,10]:");
    println!("  maximum ≈ {:.4} at x ≈ {:.4}", y_max, x_max);

    let half = gauss.param(0) / 2.0; // half of the Gaussian's peak height
    let mut crossing = f64::NAN;
    for i in 0..=n {
        let x = gauss.param(1) + 5.0 * i as f64 / n as f64; // walk right from the mean
        if gauss.eval(x) <= half {
            crossing = x;
            break;
        }
    }
    // For a Gaussian the half-max is sigma·sqrt(2·ln2) ≈ 1.1774·sigma to the right.
    let expected = gauss.param(1) + gauss.param(2) * (2.0_f64 * 2.0_f64.ln()).sqrt();
    println!(
        "  `gauss` right half-max crossing ≈ {:.4}  (analytic mean + σ·√(2ln2) = {:.4})",
        crossing, expected,
    );

    // --- 2-D and 3-D functions add the y (and z) variables. --------------------
    let f2 =
        TF2::new("f2", "[0]*sin(x) + [1]*y*y", -3.0, 3.0, -2.0, 2.0)?.with_params(vec![1.5, 0.7]);
    let f3 = TF3::new("f3", "[0]*x + y*z", 0.0, 2.0, 0.0, 2.0, 0.0, 2.0)?.with_params(vec![2.0]);
    println!("\nHigher-dimensional functions:");
    println!(
        "  f2(1,1)   = {:.6}   (1.5·sin1 + 0.7·1)",
        f2.eval(1.0, 1.0)
    );
    println!("  f3(1,1,1) = {:.6}   (2·1 + 1·1)", f3.eval(1.0, 1.0, 1.0));

    // --- Write all six to a ROOT file (ROOT C++ and uproot read these keys). ----
    // The file lives in the temp dir and is removed before we return — no litter.
    let out = std::env::temp_dir().join("oxiroot_ex_functions.root");
    FileWriter::create(&out)
        .add(&damped)
        .add(&gauss)
        .add(&decay)
        .add(&parab)
        .add(&f2)
        .add(&f3)
        .write(Compression::Zstd(5))?;
    println!("\nwrote {}", out.display());

    // --- Round-trip: read one TF1 back and confirm it evaluates identically. ---
    // `TF1::read_root` re-parses the embedded TFormula and its parameters, so the
    // decoded function reproduces `eval` to the bit at sampled points.
    let g = TF1::read_root(&FileReader::open(&out)?, "gauss")?;
    for &x in &[-2.0, -0.5, 0.5, 2.0] {
        assert!(
            (g.eval(x) - gauss.eval(x)).abs() < 1e-12,
            "round-trip mismatch at x = {x}",
        );
    }
    println!(
        "read back `gauss`: npar={}, eval(0.5) = {:.6} (matches original {:.6})",
        g.npar(),
        g.eval(0.5),
        gauss.eval(0.5),
    );

    let _ = std::fs::remove_file(&out);
    Ok(())
}
