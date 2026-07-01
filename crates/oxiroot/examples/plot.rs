//! Plotting with oxiroot (the `plot` feature) — render ROOT histograms and
//! graphs to SVG/PNG with a matplotlib-like API and an mplhep histogram style.
//! Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example plot --features plot
//! ```
//!
//! It renders four figures, each as PNG, SVG, and PDF:
//!
//! 1. `mass` — a filled MC template with "data" points overlaid, a legend, and a
//!    LaTeX axis label (the default matplotlib look). Also saved at 220 DPI.
//! 2. `mplhep` — the same histogram as a step staircase with error bars in the
//!    mplhep style (in-pointing ticks, minors, all four sides), with a bold
//!    `CMS Preliminary` experiment label and luminosity/energy above the frame.
//! 3. `heatmap` — a 2-D TH2 as a viridis color mesh with a colorbar.
//! 4. `ratio` — a main panel over a data/MC ratio panel sharing the x-axis.

#[cfg(not(feature = "plot"))]
fn main() {
    eprintln!("re-run with `--features plot` to render the figures");
}

#[cfg(feature = "plot")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use oxiroot::plot::{
        ratio_subplots, Axes, Color, ErrorbarOpts, Hist2dOpts, HistOpts, HistType, SaveOpts, Style,
    };
    use oxiroot::prelude::*;

    let out = std::env::temp_dir().join("oxiroot-plots");
    std::fs::create_dir_all(&out)?;

    // A deterministic Gaussian-filled di-muon mass histogram (Sumw2 on).
    let mut mc = Hist::reg(40, 50.0, 130.0)
        .double()
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
    let data = TGraph::with_errors(dx.clone(), dy.clone(), vec![5.0; dx.len()], dey).named("data");

    // --- 1. Filled MC + data overlay, default matplotlib look. ---
    let mut ax = Axes::new();
    ax.hist_with(
        &mc,
        HistOpts::new()
            .histtype(HistType::Fill)
            .fill_color(Color::hex("#1f77b4").with_alpha(0.4))
            .label("MC"),
    );
    ax.errorbar_with(&data, ErrorbarOpts::new().color(Color::BLACK).label("data"));
    ax.xlabel("$m_{\\mu\\mu}$ [GeV]");
    ax.ylabel("Events / 2 GeV");
    ax.title("$Z \\rightarrow \\mu\\mu$ candidates");
    ax.legend();
    save_both(&ax, &out, "mass")?;

    // Also save the first figure at a higher DPI for a sharper raster.
    ax.save_with(out.join("mass_hi.png"), SaveOpts::new().dpi(220.0))?;
    println!("wrote {} (220 dpi)", out.join("mass_hi.png").display());

    // --- 2. The same histogram as a step + error bars, mplhep style, with a
    //        CMS experiment label and luminosity/energy above the frame. ---
    let mut hep = Axes::with_style(Style::mplhep());
    hep.hist_with(&mc, HistOpts::new().yerr().label("MC"));
    hep.hep_label("CMS", "Preliminary")
        .hep_rhs("138 fb$^{-1}$ (13 TeV)");
    hep.xlabel("$m_{\\mu\\mu}$ [GeV]");
    hep.ylabel("Events / 2 GeV");
    hep.legend();
    save_both(&hep, &out, "mplhep")?;

    // --- 3. A 2-D TH2 as a viridis heatmap with a colorbar. ---
    let mut h2 = Hist::reg(40, -4.0, 4.0)
        .reg(40, -4.0, 4.0)
        .double()
        .named("h2");
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
    ax2.hist2d_with(&h2, Hist2dOpts::new().label("entries"));
    ax2.xlabel("$x$");
    ax2.ylabel("$y$");
    ax2.title("two Gaussians");
    save_both(&ax2, &out, "heatmap")?;

    // --- 4. A ratio plot: filled MC + data over a data/MC ratio panel. ---
    let mc_vals = mc.values();
    let bin_content = |x: f64| -> f64 {
        mc_vals
            .get(mc.find_bin(x).wrapping_sub(1))
            .copied()
            .unwrap_or(0.0)
    };
    let ratio_y: Vec<f64> = dx
        .iter()
        .zip(&dy)
        .map(|(&x, &y)| {
            let c = bin_content(x);
            if c > 0.0 {
                y / c
            } else {
                1.0
            }
        })
        .collect();
    let ratio_ey: Vec<f64> = dx
        .iter()
        .zip(&dy)
        .map(|(&x, &y)| {
            let c = bin_content(x);
            if c > 0.0 {
                y.sqrt() / c
            } else {
                0.0
            }
        })
        .collect();
    let rgraph = TGraph::with_errors(dx.clone(), ratio_y, vec![0.0; dx.len()], ratio_ey).named("r");

    let (fig, mut main, mut ratio) = ratio_subplots();
    main.hist_with(
        &mc,
        HistOpts::new()
            .histtype(HistType::Fill)
            .fill_color(Color::hex("#1f77b4").with_alpha(0.4))
            .label("MC"),
    );
    main.errorbar_with(&data, ErrorbarOpts::new().color(Color::BLACK).label("data"));
    main.ylabel("Events / 2 GeV");
    main.legend();
    ratio.errorbar_with(&rgraph, ErrorbarOpts::new().color(Color::BLACK));
    ratio.ylim(0.5..1.5);
    ratio.ylabel("data/MC");
    ratio.xlabel("$m_{\\mu\\mu}$ [GeV]");
    ratio.grid();
    let figr = fig.ratio(main, ratio);
    for ext in ["png", "svg", "pdf"] {
        let path = out.join(format!("ratio.{ext}"));
        figr.save(&path)?;
        println!(
            "wrote {} ({} bytes)",
            path.display(),
            std::fs::metadata(&path)?.len()
        );
    }

    // --- 5. A shared-axis 2x2 subplot grid with a figure title. ---
    use oxiroot::plot::subplots_grid;
    let (gfig, mut axs) = subplots_grid(2, 2);
    axs[0].hist(&mc);
    axs[0].ylabel("Events");
    axs[1].hist_with(&mc, HistOpts::new().histtype(HistType::Fill));
    axs[2].errorbar(&data);
    axs[2].ylabel("Events");
    axs[3].plot(&dx, &dy);
    let gfig = gfig
        .sharex()
        .sharey()
        .suptitle("$Z \\rightarrow \\mu\\mu$ — subplot grid")
        .with_axes(axs);
    gfig.save(out.join("grid.png"))?;
    println!("wrote {}", out.join("grid.png").display());

    // --- 6. Fit a Gaussian to the MC and overlay the fitted curve. ---
    // (Needs `--features plot,fit`; skipped otherwise.)
    #[cfg(feature = "fit")]
    {
        use oxiroot::fit::TF1;
        let model = TF1::gaussian("gaus").estimate_from(&mc);
        let r = mc.fit(&model);
        let fitted = model.with_params(r.params.clone());
        let mut ax = Axes::new();
        ax.hist_with(
            &mc,
            HistOpts::new()
                .histtype(HistType::Fill)
                .fill_color(Color::hex("#1f77b4").with_alpha(0.4))
                .label("data"),
        );
        ax.model_with(
            &fitted,
            50.0..130.0,
            oxiroot::plot::CurveOpts::new()
                .color(Color::hex("#d62728"))
                .linewidth(2.0)
                .label(format!(
                    "Gaussian fit ($\\chi^2$/ndf = {:.1})",
                    r.chi2 / r.ndf.max(1) as f64
                )),
        );
        ax.xlabel("$m_{\\mu\\mu}$ [GeV]");
        ax.ylabel("Events / 2 GeV");
        ax.legend();
        save_both(&ax, &out, "fit")?;
    }

    Ok(())
}

#[cfg(feature = "plot")]
fn save_both(
    ax: &oxiroot::plot::Axes,
    dir: &std::path::Path,
    stem: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for ext in ["png", "svg", "pdf"] {
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
