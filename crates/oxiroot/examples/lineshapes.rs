//! The HEP lineshapes in `oxiroot::stat` — the peak and resonance shapes
//! physicists actually fit — evaluated directly, then a Crystal Ball peak fitted
//! to simulated data with the `fit` feature. Part A (the shapes) needs no
//! feature; only the fit at the end is gated.
//!
//! ```sh
//! cargo run -p oxiroot --example lineshapes --features fit
//! ```

// the prelude (Model/TH1/FitOptions) is only used by the fit at the very end
#[cfg(feature = "fit")]
use oxiroot::prelude::*;
use oxiroot::stat;

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

fn main() {
    // --- Part A: the shapes themselves (always available). ---------------------
    // Compare a Gaussian to a Crystal Ball with the same core (mean 0, sigma 1).
    // The Crystal Ball switches to a power-law tail below `-alpha·sigma`, so on
    // the LOW side it stays visibly non-zero where the Gaussian has died away —
    // exactly the radiative tail seen in reconstructed mass peaks.
    let (mean, sigma, alpha, n) = (0.0, 1.0, 1.0, 3.0);
    println!("Gaussian (·) vs Crystal Ball (#): unit-peak core mean=0 sigma=1,");
    println!(
        "Crystal Ball tail begins at x = -alpha·sigma = {:.1}\n",
        -alpha
    );
    println!("    x     gaus     CB      bars (CB '#', gaus '.')");
    let mut x = -6.0;
    while x <= 4.0 + 1e-9 {
        let g = stat::gaussian(x, mean, sigma);
        let cb = stat::crystal_ball(x, mean, sigma, alpha, n);
        // Map each value in [0, 1] to a 40-column bar; overlay '.' for the
        // Gaussian on top of the Crystal Ball's '#'.
        let width = 40usize;
        let cb_cols = (cb * width as f64).round() as usize;
        let g_cols = (g * width as f64).round() as usize;
        let mut bar = vec![b' '; width];
        for c in bar.iter_mut().take(cb_cols.min(width)) {
            *c = b'#';
        }
        if g_cols >= 1 && g_cols <= width {
            bar[g_cols - 1] = b'.'; // where the Gaussian reaches
        }
        let bar = String::from_utf8(bar).expect("ASCII bar");
        println!("  {x:>4.1}  {g:>6.3}  {cb:>6.3}  |{bar}|");
        x += 0.5;
    }
    // Read the table: near x = -4 the Gaussian is ~1e-4 (an empty bar) while the
    // Crystal Ball still carries real probability in its tail.
    println!(
        "\n  note: at x=-4, gaus={:.2e} (gone) but CB={:.3} (tail alive)",
        stat::gaussian(-4.0, mean, sigma),
        stat::crystal_ball(-4.0, mean, sigma, alpha, n),
    );

    // --- A small table of the other resonance / background shapes. -------------
    // Each is sampled at a few x around a peak/endpoint at 5, with a note on what
    // it models. Densities are unit-area; ARGUS is the unnormalized endpoint bkg.
    println!("\nOther lineshapes sampled at x = 3, 5, 7 (peak/endpoint near 5):");
    println!(
        "  {:<24}  {:>8}  {:>8}  {:>8}   models",
        "shape", "x=3", "x=5", "x=7"
    );
    let row = |name: &str, f: &dyn Fn(f64) -> f64, note: &str| {
        println!(
            "  {name:<24}  {:>8.4}  {:>8.4}  {:>8.4}   {note}",
            f(3.0),
            f(5.0),
            f(7.0),
        );
    };
    row(
        "voigtian(5, 1, 0.5)",
        &|x| stat::voigtian(x, 5.0, 1.0, 0.5),
        "Gaussian resolution ⊗ Lorentzian",
    );
    row(
        "breit_wigner(5, 1.0)",
        &|x| stat::breit_wigner(x, 5.0, 1.0),
        "a resonance line (Lorentzian)",
    );
    row(
        "novosibirsk(5, 1, 0.3)",
        &|x| stat::novosibirsk(x, 5.0, 1.0, 0.3),
        "skewed peak (radiative tail)",
    );
    row(
        "argus(x, 10, -3, 0.5)",
        &|x| stat::argus(x, 10.0, -3.0, 0.5),
        "phase-space endpoint background",
    );
    row(
        "landau(5, 1.0)",
        &|x| stat::landau(x, 5.0, 1.0),
        "ionization energy loss (dE/dx)",
    );

    // --- Part B: fit a Crystal Ball to simulated data (needs `fit`). -----------
    #[cfg(feature = "fit")]
    fit_crystal_ball();
    #[cfg(not(feature = "fit"))]
    println!(
        "\n(Part B — fitting a Crystal Ball — needs the `fit` feature:\n \
         cargo run -p oxiroot --example lineshapes --features fit)"
    );
}

/// Simulate a peak with a low-side radiative tail, then recover its shape by
/// fitting an `oxiroot::fit` Crystal Ball [`Model`] to the filled histogram.
#[cfg(feature = "fit")]
fn fit_crystal_ball() {
    let mut rng = XorShift64(0x0DD_F00D_CAFE_BEEF);
    let (true_mean, true_sigma) = (91.2, 2.5); // a Z-like mass peak [GeV]

    let mut peak = Hist::reg(80, 75.0, 100.0)
        .double()
        .named("cb_mass")
        .titled("mass with a low-side tail [GeV]");
    peak.sumw2(); // per-bin errors for the chi-square

    // ~4000 events: a Gaussian core, but 12 % are dragged just below the peak to
    // fake the radiative tail the Crystal Ball is built to describe.
    for _ in 0..4_000 {
        let mut m = rng.gauss(true_mean, true_sigma);
        if rng.uniform() < 0.12 {
            m -= 1.5 + 3.0 * rng.uniform(); // pull into the low-side tail
        }
        peak.fill(m);
    }

    // Seed the Gaussian core straight from the data (constant/mean/sigma) with
    // `estimate_from`, then set the tail parameters (alpha, n) by hand near the
    // truth — `estimate_from` only touches the Gaussian-core triple, so the
    // alpha/n we set here survive. Keep the width/tail physical (and `n`
    // bounded, so it can't run away) for a clean MIGRAD convergence.
    let mut model = Model::crystal_ball("cb")
        .with_params(vec![peak.maximum(), true_mean, true_sigma, 1.3, 4.0])
        .estimate_from(&peak)
        .lower_limit("sigma", 0.0)
        .limit("alpha", 0.2, 5.0)
        .limit("n", 1.0, 20.0);

    let r = peak.fit_into(&mut model, &FitOptions::new());

    println!("\nCrystal Ball fit (truth: mean = {true_mean}, sigma = {true_sigma}):");
    let (p, e) = (&r.params, &r.errors);
    println!(
        "  mean  = {:.3} ± {:.3} GeV   sigma = {:.3} ± {:.3} GeV",
        p[1], e[1], p[2], e[2],
    );
    println!(
        "  alpha = {:.3}   n = {:.3}   chi2/ndf = {:.2}   valid = {}",
        p[3],
        p[4],
        r.chi2_per_ndf(),
        r.valid,
    );
    // The fitted model now evaluates the best-fit curve.
    println!(
        "  fitted peak height: {:.1} counts/bin at m = {:.2} GeV",
        model.eval(p[1]),
        p[1],
    );
}
