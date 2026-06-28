//! Plotting with oxiroot (the `plot` feature) — render ROOT histograms and
//! graphs to SVG/PNG with a matplotlib-like API and an mplhep histogram style.
//! Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example plot --features plot
//! ```
//!
//! It builds a Z → μμ mass histogram and overlays "data" points on a filled MC
//! template (with a legend and LaTeX axis labels), then renders a 2-D TH2 as a
//! viridis heatmap with a colorbar. Each figure is written as both PNG and SVG.

#[cfg(not(feature = "plot"))]
fn main() {
    eprintln!("re-run with `--features plot` to render the figures");
}

#[cfg(feature = "plot")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use oxiroot::plot::{Axes, Color, ErrorbarOpts, Hist2dOpts, HistOpts, HistType};
    use oxiroot::prelude::*;

    let out = std::env::temp_dir().join("oxiroot-plots");
    std::fs::create_dir_all(&out)?;

    // --- A 1-D histogram: filled MC template + "data" points + a legend. ---
    let mut mc = TH1::new(40, 50.0, 130.0)
        .named("mass")
        .titled("di-muon mass");
    mc.sumw2();
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as f64 / (1u64 << 31) as f64
    };
    let mut gauss = move || (0..12).map(|_| next()).sum::<f64>() - 6.0;
    for _ in 0..40000 {
        mc.fill(91.0 + 7.5 * gauss());
    }

    // A handful of "data" points with statistical error bars (a TGraphErrors).
    let dx: Vec<f64> = (0..8).map(|i| 55.0 + 10.0 * i as f64).collect();
    let dy: Vec<f64> = dx
        .iter()
        .map(|x| 2000.0 * (-0.5 * ((x - 91.0) / 7.5).powi(2)).exp())
        .collect();
    let dey: Vec<f64> = dy.iter().map(|y| y.sqrt().max(15.0)).collect();
    let data = TGraph::with_errors(dx.clone(), dy, vec![5.0; dx.len()], dey).named("data");

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
    ax.legend();
    ax.save(out.join("mass.png"))?;
    ax.save(out.join("mass.svg"))?;

    // --- A 2-D histogram as a viridis heatmap with a colorbar. ---
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
    ax2.set_title("oxiroot-plot: 2-D");
    ax2.save(out.join("heatmap.png"))?;
    ax2.save(out.join("heatmap.svg"))?;

    for name in ["mass.png", "mass.svg", "heatmap.png", "heatmap.svg"] {
        let p = out.join(name);
        let len = std::fs::metadata(&p)?.len();
        assert!(len > 0, "{name} is empty");
        println!("wrote {} ({len} bytes)", p.display());
    }
    Ok(())
}
