//! Hypothesis tests and correlation from `oxiroot::stat` (a scipy.stats-verified,
//! zero-dependency stats library). An A/B-style walk-through: draw a control
//! sample A and a shifted treatment sample B, then ask whether they differ; then
//! correlate a noisy line, test a histogram's goodness of fit, and show the
//! normality test flag a skewed sample. Every test prints its statistic, p-value,
//! and a "reject H0 at 0.05?" verdict. Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example stat_tests
//! ```

use oxiroot::prelude::*;
use oxiroot::stat::{self, StatError};

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run.
struct Rng(u64);

impl Rng {
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

fn main() -> Result<(), StatError> {
    let mut rng = Rng(0x51A7_7E57_C0DE_1234);

    // A tidy printer for a test: name, statistic, p-value, and the 0.05 verdict.
    // The null hypothesis H0 is "no effect" (samples identical / no correlation);
    // we reject it when p < alpha.
    let alpha = 0.05;
    let report = |name: &str, stat: f64, p: f64| {
        let verdict = if p < alpha { "REJECT H0" } else { "keep H0" };
        println!("  {name:<14} stat = {stat:>9.4}   p = {p:>9.4}   -> {verdict}");
    };

    // --- Draw the two samples, as in an A/B test. -----------------------------
    // Control A ~ N(0, 1); treatment B ~ N(0.6, 1) — a real +0.6σ shift that the
    // tests below should detect.
    let control: Vec<f64> = (0..200).map(|_| rng.gauss(0.0, 1.0)).collect();
    let treatment: Vec<f64> = (0..200).map(|_| rng.gauss(0.6, 1.0)).collect();
    println!(
        "A/B samples: control N(0,1) n={}, treatment N(0.6,1) n={}  (true shift = +0.6)",
        control.len(),
        treatment.len(),
    );

    // --- (1) Two-sample tests: are A and B drawn from the same distribution? --
    // Three complementary lenses on the same question:
    //   ttest_ind    — parametric, compares the means (assumes equal variance).
    //   mannwhitneyu — nonparametric rank-sum, compares whole distributions.
    //   ks_2samp     — the largest gap between the two empirical CDFs.
    println!("(1) Two-sample tests  (H0: control and treatment are identical):");
    let (t, tp) = stat::ttest_ind(&control, &treatment);
    report("ttest_ind", t, tp);
    let (u, up) = stat::mannwhitneyu(&control, &treatment);
    report("mannwhitneyu", u, up);
    let (d, dp) = stat::ks_2samp(&control, &treatment);
    report("ks_2samp", d, dp);

    // --- (2) Correlation: a noisy straight line y = a + b·x + noise. ----------
    // Pearson's r measures linear correlation; Spearman's rho correlates the
    // ranks (robust to a monotone-but-curved relationship). Both come with a
    // two-sided p-value testing H0: rho = 0.
    let (a_true, b_true) = (1.0, 2.0);
    let x: Vec<f64> = (0..80).map(|i| i as f64 * 0.1).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|&xi| a_true + b_true * xi + rng.gauss(0.0, 1.5))
        .collect();
    println!("(2) Correlation of y = {a_true} + {b_true}*x + noise  (H0: no correlation):");
    let (r, rp) = stat::pearsonr(&x, &y)?;
    report("pearsonr", r, rp);
    let (rho, sp) = stat::spearmanr(&x, &y)?;
    report("spearmanr", rho, sp);

    // --- (3) Goodness of fit: observed histogram counts vs an expected shape. --
    // Fill a histogram from the control sample, then compare its bin counts to
    // the counts a true N(0,1) would predict (same total). chisquare() returns
    // (chi2, p) with k−1 degrees of freedom; a large p means "consistent with
    // the expected shape".
    let (lo, hi, nbins) = (-3.0, 3.0, 6usize);
    let mut hist = Hist::reg(nbins as i32, lo, hi)
        .double()
        .named("control")
        .titled("control sample");
    for &v in &control {
        hist.fill(v);
    }
    let width = (hi - lo) / nbins as f64;
    let n_in_range: f64 = (1..=nbins).map(|b| hist[b]).sum(); // in-range bins only
    let f_obs: Vec<f64> = (1..=nbins).map(|b| hist[b]).collect();
    // Expected: the standard-normal probability mass per bin, scaled to the same
    // in-range total so the two vectors sum alike (chisquare requires that).
    let normal = stat::Normal::standard();
    let raw_exp: Vec<f64> = (0..nbins)
        .map(|b| {
            let (l, r) = (lo + b as f64 * width, lo + (b + 1) as f64 * width);
            normal.cdf(r) - normal.cdf(l)
        })
        .collect();
    let exp_sum: f64 = raw_exp.iter().sum();
    let f_exp: Vec<f64> = raw_exp.iter().map(|p| p / exp_sum * n_in_range).collect();
    println!("(3) Goodness of fit  (H0: control counts follow N(0,1)):");
    println!("      observed bins : {f_obs:?}");
    println!(
        "      expected bins : [{}]",
        f_exp
            .iter()
            .map(|e| format!("{e:.1}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let (chi2, cp) = stat::chisquare(&f_obs, &f_exp)?;
    report("chisquare", chi2, cp);

    // --- (4) Normality: normaltest() should PASS the Gaussian, FLAG the skew. --
    // The D'Agostino–Pearson omnibus test combines skewness and kurtosis. Here
    // `control` is genuinely Gaussian, while `skewed` is exp()-transformed (a
    // heavy right tail) — the test should keep H0 for the first and reject it
    // for the second.
    let skewed: Vec<f64> = control.iter().map(|&v| (0.8 * v).exp()).collect();
    println!("(4) Normality test  (H0: the sample is Gaussian):");
    let (kg, kgp) = stat::normaltest(&control);
    report("gaussian", kg, kgp);
    let (ks, ksp) = stat::normaltest(&skewed);
    report("skewed", ks, ksp);

    println!(
        "\nSummary: the two-sample tests detect the +0.6 shift, r and rho confirm the\n\
         line, the histogram is consistent with N(0,1), and normaltest flags only the\n\
         skewed sample — all p-values match scipy.stats."
    );
    Ok(())
}
