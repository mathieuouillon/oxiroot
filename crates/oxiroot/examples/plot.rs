//! Plotting with oxiroot (the `plot` feature) — render ROOT histograms and
//! graphs to SVG/PNG with a matplotlib-like API and an mplhep histogram style.
//! Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example plot --features plot
//! ```
//!
//! It renders three figures, each as both PNG and SVG:
//!   1. `mass`    — a filled MC template with "data" points overlaid, a legend,
//!                  and a LaTeX axis label (the default matplotlib look).
//!   2. `mplhep`  — the same histogram as a step staircase with error bars in the
//!                  mplhep style (in-pointing ticks, minors, all four sides).
//!   3. `heatmap` — a 2-D TH2 as a viridis color mesh with a colorbar.

#[cfg(not(feature = "plot"))]
fn main() {
    eprintln!("re-run with `--features plot` to render the figures");
}

#[cfg(feature = "plot")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use oxiroot::plot::{Axes, Color, ErrorbarOpts, Hist2dOpts, HistOpts, HistType, Style};
    use oxiroot::prelude::*;

    let out = std::env::temp_dir().join("oxiroot-plots");
    std::fs::create_dir_all(&out)?;

    // A deterministic Gaussian-filled di-muon mass histogram (Sumw2 on).
    let mut mc = TH1::new(40, 50.0, 130.0)
        .named("mass")
        .titled("di-muon mass");
    mc.sumw2();
    let mut rng = Lcg::new(0x1234_5678_9abc_def0);
    for _ in 0..40_000 {
        mc.fill(91.0 + 7.5 * rng.gauss());
    }

    // A handful of "data" points with statistical error bars (a TGraphErrors).
    let dx: Vec<f64> = (0..8).map(|i| 55.0 + 10.0 * i as f64).collect();
    let dy: Vec<f64> = dx
        .iter()
        .map(|x| 2000.0 * (-0.5 * ((x - 91.0) / 7.5).powi(2)).exp())
        .collect();
    let dey: Vec<f64> = dy.iter().map(|y| y.sqrt().max(15.0)).collect();
    let data = TGraph::with_errors(dx.clone(), dy, vec![5.0; dx.len()], dey).named("data");

    // --- 1. Filled MC + data overlay, default matplotlib look. ---
    let mut ax = Axes::new();
    ax.histplot(
        &mc,
        HistOpts::new()
            .histtype(HistType::Fill)
            .fill_color(Color::hex("#1f77b4").with_alpha(0.4))
            .label("MC"),
    );
    ax.errorbar_opts(&data, ErrorbarOpts::new().color(Color::BLACK).label("data"));
    ax.set_xlabel("$m_{\\mu\\mu}$ [GeV]");
    ax.set_ylabel("Events / 2 GeV");
    ax.set_title("$Z \\rightarrow \\mu\\mu$ candidates");
    ax.legend();
    save_both(&ax, &out, "mass")?;

    // --- 2. The same histogram as a step + error bars in the mplhep style. ---
    let mut hep = Axes::with_style(Style::mplhep());
    hep.histplot(&mc, HistOpts::new().yerr(true).label("MC"));
    hep.set_xlabel("$m_{\\mu\\mu}$ [GeV]");
    hep.set_ylabel("Events / 2 GeV");
    hep.legend();
    save_both(&hep, &out, "mplhep")?;

    // --- 3. A 2-D TH2 as a viridis heatmap with a colorbar. ---
    let mut h2 = TH2::new(40, -4.0, 4.0, 40, -4.0, 4.0).named("h2");
    for ix in 0..40 {
        for iy in 0..40 {
            let x = -4.0 + (ix as f64 + 0.5) * 0.2;
            let y = -4.0 + (iy as f64 + 0.5) * 0.2;
            let z = (-(x * x + y * y) / 2.0).exp() * 100.0
                + 50.0 * (-((x - 1.5).powi(2) + (y + 1.2).powi(2)) / 0.5).exp();
            h2.fill_weight(x, y, z);
        }
    }
    let mut ax2 = Axes::new();
    ax2.hist2dplot(&h2, Hist2dOpts::new().label("entries"));
    ax2.set_xlabel("$x$");
    ax2.set_ylabel("$y$");
    ax2.set_title("two Gaussians");
    save_both(&ax2, &out, "heatmap")?;

    Ok(())
}

#[cfg(feature = "plot")]
fn save_both(
    ax: &oxiroot::plot::Axes,
    dir: &std::path::Path,
    stem: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for ext in ["png", "svg"] {
        let path = dir.join(format!("{stem}.{ext}"));
        ax.save(&path)?;
        let len = std::fs::metadata(&path)?.len();
        assert!(len > 0, "{} is empty", path.display());
        println!("wrote {} ({len} bytes)", path.display());
    }
    Ok(())
}

/// A tiny linear-congruential generator with a 12-uniform Gaussian, so the
/// figures are byte-for-byte reproducible without an RNG dependency.
#[cfg(feature = "plot")]
struct Lcg(u64);

#[cfg(feature = "plot")]
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as f64 / (1u64 << 31) as f64
    }
    fn gauss(&mut self) -> f64 {
        (0..12).map(|_| self.unit()).sum::<f64>() - 6.0
    }
}
