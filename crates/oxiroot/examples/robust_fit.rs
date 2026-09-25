//! Robust loss functions for line fitting (the `fit` feature), mirroring the
//! [`scipy.optimize.least_squares`](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.least_squares.html)
//! robust-fitting tutorial. It fits a straight line `y = a + b·x` to data that a
//! handful of gross outliers have polluted, comparing an ordinary least-squares
//! fit (`Loss::Linear`, which the outliers drag off the truth) against the robust
//! losses `SoftL1`, `Huber`, and `Cauchy`, which down-weight the outliers and
//! recover the true `a = 1, b = 2`. Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example robust_fit --features fit
//! ```

#[cfg(not(feature = "fit"))]
fn main() {
    eprintln!("This example needs the `fit` feature:");
    eprintln!("  cargo run -p oxiroot --example robust_fit --features fit");
}

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run.
#[cfg(feature = "fit")]
struct XorShift64(u64);

#[cfg(feature = "fit")]
impl XorShift64 {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self, mean: f64, sigma: f64) -> f64 {
        let (u1, u2) = (self.uniform().max(1e-12), self.uniform());
        mean + sigma * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(feature = "fit")]
fn main() {
    use oxiroot::prelude::*;

    let mut rng = XorShift64(0x0DD_F00D_CAFE_BEEF);
    // The truth every fit should recover: y = a + b·x with a = 1, b = 2.
    let (true_a, true_b) = (1.0, 2.0);

    // --- Build a noisy line, then corrupt a few points into gross outliers. ----
    // 30 clean points on the line with small Gaussian scatter...
    let n = 30usize;
    let xs: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
    let mut ys: Vec<f64> = xs
        .iter()
        .map(|&x| true_a + true_b * x + rng.gauss(0.0, 0.2))
        .collect();

    // ...then wreck 4 of them: throw each far off the line, alternating sign.
    // These are the points a plain least-squares fit will chase.
    let outliers = [4usize, 11, 18, 25];
    for (k, &i) in outliers.iter().enumerate() {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        ys[i] += sign * 40.0; // a huge offset — nowhere near the true line
    }
    // Uniform per-point errors: every point looks equally trustworthy up front,
    // so only the loss function can tell the outliers apart.
    let sigmas = vec![1.0; n];
    println!(
        "{n} points on y = {true_a} + {true_b}·x, {} of them corrupted into gross outliers:",
        outliers.len()
    );
    for &i in &outliers {
        println!("  outlier at x = {:.1}: y = {:.1}", xs[i], ys[i]);
    }
    println!();

    // The model: a straight line `a + b·x`, seeded at (0, 0).
    let line = || Model::new("line", &["a", "b"], vec![0.0, 0.0], |x, p| p[0] + p[1] * x);
    let data = Points::new(&xs, &ys, &sigmas).expect("x, y and their errors have the same length");

    // --- 1. Ordinary least squares (Loss::Linear). ----------------------------
    // The default cost: every squared residual counts in full, so the four
    // far-out points dominate and pull the line badly off the truth.
    let ols = data.fit_opts(&line(), &FitOptions::new().loss(Loss::Linear));
    let (a0, b0) = (ols.params[0], ols.params[1]);

    // --- 2. Robust losses, each SEEDED from a prior fit. ----------------------
    // Robust losses have a small basin of attraction, so the gradient-based
    // MIGRAD can stall if started cold at (0, 0). Seeding it (via `with_params`)
    // with the parameters of a fit that already ran keeps it inside the basin.
    let robust = |loss: Loss, seed: &[f64]| {
        let opts = FitOptions::new().loss(loss);
        data.fit_opts(&line().with_params(seed.to_vec()), &opts)
    };
    // SoftL1 and Huber are forgiving — seed them from the (biased) OLS result and
    // they still climb out to the truth.
    let softl1 = robust(Loss::SoftL1, &[a0, b0]);
    let huber = robust(Loss::Huber, &[a0, b0]);
    // Cauchy suppresses outliers hardest but has the narrowest basin — from the
    // OLS seed MIGRAD stalls. So chain it: seed Cauchy from the recovered Huber
    // parameters, and it converges cleanly. (This is the standard robust-fitting
    // recipe: a gentle loss first, then a sharper one seeded from it.)
    let cauchy = robust(Loss::Cauchy, &huber.params);

    // --- Report: one row per loss, plus how far each strays from the truth. ----
    println!("Line fits of the SAME corrupted data (truth: a = 1.000, b = 2.000):");
    println!(
        "  {:<8}  {:>8}  {:>8}   |error| in (a, b)",
        "loss", "a", "b"
    );
    let row = |label: &str, r: &FitResult| {
        let (a, b) = (r.params[0], r.params[1]);
        println!(
            "  {:<8}  {:>8.3}  {:>8.3}   ({:.3}, {:.3})",
            label,
            a,
            b,
            (a - true_a).abs(),
            (b - true_b).abs(),
        );
    };
    row("Linear", &ols);
    row("SoftL1", &softl1);
    row("Huber", &huber);
    row("Cauchy", &cauchy);
    println!();
    println!("Linear (OLS) is dragged toward the outliers; SoftL1/Huber/Cauchy down-weight");
    println!(
        "them and recover a ≈ {true_a}, b ≈ {true_b}. Cauchy suppresses outliers hardest, but"
    );
    println!("was seeded from the Huber fit so its narrow basin still converges.");

    // --- Write the outlier-cleaned line as a persistable Func1D, then read it back.
    // The robust (Cauchy) fit is the trustworthy one — save its curve to a ROOT
    // file so it can be reused. The file lives in the temp dir and is deleted
    // before we return; nothing is left behind.
    let path = std::env::temp_dir().join("oxiroot_ex_robust_fit.root");
    let fitted = Func1D::new("robust_line", "[0]+[1]*x", xs[0], xs[n - 1])
        .expect("valid formula")
        .with_params(vec![cauchy.params[0], cauchy.params[1]]);
    fitted
        .write_root(&path, Compression::Zstd(5))
        .expect("write TF1");
    let f = FileReader::open(&path).expect("open file");
    let back = Func1D::read_root(&f, "robust_line").expect("read TF1");
    println!();
    println!(
        "wrote + read back the robust line as a TF1: y({:.1}) = {:.3}, y({:.1}) = {:.3}",
        xs[0],
        back.eval(xs[0]),
        xs[n - 1],
        back.eval(xs[n - 1]),
    );
    let _ = std::fs::remove_file(&path);
}
