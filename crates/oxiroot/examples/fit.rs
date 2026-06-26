//! Histogram fitting with oxiroot (the `fit` feature, backed by the pure-Rust
//! Minuit2 port). Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example fit --features fit
//! ```
//!
//! It fills a histogram with simulated Z → μμ events and fits the mass peak with
//! a Gaussian (both chi-square and binned maximum likelihood), then fits a peak
//! sitting on a flat background with a custom model defined as a closure.

#[cfg(not(feature = "fit"))]
fn main() {
    eprintln!("This example needs the `fit` feature:");
    eprintln!("  cargo run -p oxiroot --example fit --features fit");
}

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run.
#[cfg(feature = "fit")]
struct Rng(u64);

#[cfg(feature = "fit")]
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

#[cfg(feature = "fit")]
fn main() {
    use oxiroot::prelude::*;

    let mut rng = Rng(0x0DD_F00D_CAFE_BEEF);
    // True peak parameters the fits should recover.
    let (true_mean, true_sigma) = (91.2, 2.5);

    // --- 1. A clean Gaussian peak: chi-square vs binned likelihood. ---
    let mut peak = TH1::new("mass", "di-muon mass [GeV]", 60, 80.0, 100.0);
    peak.sumw2(); // track per-bin errors for the chi-square
    for _ in 0..10_000 {
        peak.fill(rng.gauss(true_mean, true_sigma));
    }

    // `estimate_from` seeds (constant, mean, sigma) straight from the bins — no
    // manual moment loop, and it works even for set_bin_content histograms.
    let model = TF1::gaussian("z").estimate_from(&peak);
    let chi2 = peak.fit(&model); // chi-square (the default)
    let like = peak.fit_with(&model, FitMethod::Likelihood); // Poisson likelihood

    println!("Gaussian peak fit (truth: mean = {true_mean}, sigma = {true_sigma}):");
    let (p, e) = (&chi2.params, &chi2.errors);
    println!(
        "  chi2  : mean = {:.3} ± {:.3} GeV   sigma = {:.3} ± {:.3} GeV   chi2/ndf = {:.2}",
        p[1],
        e[1],
        p[2],
        e[2],
        chi2.chi2_per_ndf()
    );
    println!(
        "  likeli: mean = {:.3} ± {:.3} GeV   sigma = {:.3} ± {:.3} GeV",
        like.params[1], like.errors[1], like.params[2], like.errors[2]
    );

    // --- 2. The same peak on a flat background, fitted with a custom model. ---
    let mut withbkg = TH1::new("mass_bkg", "peak + background", 60, 80.0, 100.0);
    withbkg.sumw2();
    for _ in 0..10_000 {
        withbkg.fill(rng.gauss(true_mean, true_sigma)); // signal
    }
    for _ in 0..6_000 {
        withbkg.fill(80.0 + 20.0 * rng.uniform()); // flat background
    }

    // A closure model: Gaussian signal + constant background.
    // params: [norm, mean, sigma, background-per-bin].
    let mut sig_bkg = TF1::new(
        "sig+bkg",
        &["norm", "mean", "sigma", "bkg"],
        vec![withbkg.maximum(), 91.0, 2.0, withbkg.minimum()],
        |x, q| q[0] * (-0.5 * ((x - q[1]) / q[2]).powi(2)).exp() + q[3],
    );
    // fit_into writes the best-fit parameters back into the model, so `sig_bkg`
    // then evaluates the fitted curve.
    let r = withbkg.fit_into(&mut sig_bkg, &FitOptions::new());

    println!("Signal + background fit (custom closure model):");
    println!(
        "  mean = {:.3} GeV   sigma = {:.3} GeV   background = {:.1}/bin   chi2/ndf = {:.2}",
        r.params[1],
        r.params[2],
        r.params[3],
        r.chi2_per_ndf()
    );
    // A plain Gaussian here would be biased by the background; the extra term
    // recovers the true peak. The fitted model draws the full curve:
    println!(
        "  fitted curve height at the peak: {:.1} counts/bin",
        sig_bkg.eval(r.params[1])
    );
}
