//! A tour of `oxiroot::stat`, the scipy.stats-verified pure-Rust statistics
//! library: describe a small sample of measurements, then use the continuous
//! distributions (`Normal`, `StudentT`, `ChiSquared`) to turn those numbers
//! into confidence intervals, critical values, and a p-value — the everyday
//! toolkit of a physics analysis, with no external crates.
//!
//! ```sh
//! cargo run -p oxiroot --example stat_intro
//! ```

use oxiroot::stat::{
    describe, iqr, kurtosis, median, median_abs_deviation, moment, quantile, sem, skew, zscore,
    ChiSquared, Normal, StudentT,
};

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run.
struct XorShift64(u64);

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

fn main() {
    // --- A small sample of "measurements". ------------------------------------
    // Pretend these are 20 readings of some quantity whose true value is 10.0
    // with a spread of 0.5 — e.g. a calibration constant measured 20 times.
    // Drawn from a fixed-seed Gaussian, so the numbers below never change.
    let mut rng = XorShift64(0x0DD_F00D_CAFE_BEEF);
    let (truth, spread) = (10.0, 0.5);
    let sample: Vec<f64> = (0..20).map(|_| rng.gauss(truth, spread)).collect();
    let n = sample.len();

    println!("Sample of {n} measurements (truth = {truth}, spread = {spread}):");
    // Print them a few per line so the raw data is visible.
    for row in sample.chunks(5) {
        let cells: Vec<String> = row.iter().map(|x| format!("{x:7.3}")).collect();
        println!("  {}", cells.join("  "));
    }

    // --- One-shot descriptive summary (scipy.stats.describe). ------------------
    // `describe` returns nobs / min / max / mean / variance (ddof=1) plus the
    // biased Fisher skewness and excess kurtosis, all in a single pass.
    let d = describe(&sample);
    println!("\ndescribe():");
    println!("  nobs     = {}", d.nobs);
    println!("  min .. max = {:.3} .. {:.3}", d.min, d.max);
    println!("  mean     = {:.4}", d.mean);
    println!("  variance = {:.5}   (sample, ddof = 1)", d.variance);
    println!("  std dev  = {:.4}", d.variance.sqrt());
    println!(
        "  skewness = {:+.4}  (biased Fisher–Pearson g1)",
        d.skewness
    );
    println!(
        "  kurtosis = {:+.4}  (biased excess; 0 for a normal)",
        d.kurtosis
    );

    // --- Robust / order statistics — less sensitive to outliers. ---------------
    // The median and IQR describe the middle of the data without assuming a
    // shape; the MAD (rescaled) is a robust stand-in for the standard deviation.
    println!("\nRobust statistics:");
    println!("  median             = {:.4}", median(&sample));
    println!(
        "  Q1 .. Q3           = {:.4} .. {:.4}",
        quantile(&sample, 0.25),
        quantile(&sample, 0.75)
    );
    println!("  IQR (Q3 − Q1)      = {:.4}", iqr(&sample));
    println!(
        "  MAD (→ std, normal)= {:.4}",
        median_abs_deviation(&sample, true)
    );
    // Un-normalized moments and the sample-corrected skew/kurtosis for contrast.
    println!("  2nd central moment = {:.5}", moment(&sample, 2));
    println!("  skew (unbiased)    = {:+.4}", skew(&sample, false));
    println!(
        "  kurtosis (Pearson) = {:.4}   (fisher = false: 3 for a normal)",
        kurtosis(&sample, false, false)
    );

    // The standard error of the mean says how well we know `mean` itself.
    let mean = d.mean;
    let se = sem(&sample); // std(ddof=1) / sqrt(n)
    println!("  standard error of the mean = {se:.4}");

    // z-scores flag how many standard deviations each point sits from the mean;
    // report the most extreme one as a simple outlier check.
    let z = zscore(&sample);
    let (imax, zmax) = z
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, &v)| (i, v))
        .expect("sample is non-empty");
    println!(
        "  most extreme point : sample[{imax}] = {:.3}  (z = {zmax:+.2})",
        sample[imax]
    );

    // --- Confidence interval on the mean, two ways. ----------------------------
    // Large-sample (normal) 95% two-sided half-width: z * SE, where z is the
    // 0.975 quantile of the standard normal (so 2.5% sits in each tail).
    let z95 = Normal::standard().ppf(0.975);
    let half_z = z95 * se;
    println!("\n95% confidence interval on the mean:");
    println!(
        "  normal (z = {z95:.4}):   {:.4} ± {:.4}   = [{:.4}, {:.4}]",
        mean,
        half_z,
        mean - half_z,
        mean + half_z,
    );

    // Small-sample correction: with only n measurements we should use Student's
    // t with n − 1 degrees of freedom, whose 0.975 quantile is a bit wider than
    // the normal's — the honest interval for a 20-point sample.
    let dof = (n - 1) as f64;
    let t95 = StudentT::new(dof).ppf(0.975);
    let half_t = t95 * se;
    println!(
        "  Student-t (df = {dof}, t = {t95:.4}): {:.4} ± {:.4}   = [{:.4}, {:.4}]",
        mean,
        half_t,
        mean - half_t,
        mean + half_t,
    );
    println!(
        "  → the t interval is {:.1}% wider (small-sample penalty).",
        100.0 * (half_t / half_z - 1.0)
    );

    // --- A chi-square critical value — the goodness-of-fit threshold. ----------
    // If a fit with k degrees of freedom gave a chi-square above this cutoff,
    // you would reject it at the 5% level (0.95 quantile of chi-squared).
    let k = 10.0;
    let chi2_crit = ChiSquared::new(k).ppf(0.95);
    println!("\nChi-square critical value (df = {k}, 95%): {chi2_crit:.3}");
    println!(
        "  a fit with chi2/ndf ≈ {:.2} would sit right at the 5% rejection edge.",
        chi2_crit / k
    );

    // --- A p-value from the survival function. ---------------------------------
    // Suppose theory predicts the true value is `truth`. How compatible is our
    // measured mean with it? The pull is (mean − truth) / SE; its one-sided
    // p-value is the upper-tail probability of the standard normal, sf(z).
    let pull = (mean - truth) / se;
    let p_one = Normal::standard().sf(pull.abs());
    println!("\nCompatibility of the measured mean with the truth ({truth}):");
    println!("  pull = (mean − truth) / SE = {pull:+.3} σ");
    println!("  one-sided p-value  P(Z > |pull|) = {p_one:.4}");
    println!("  two-sided p-value                = {:.4}", 2.0 * p_one);
}
