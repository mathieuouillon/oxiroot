//! Pure-Rust plotting for ROOT histograms and graphs.
//!
//! `oxiroot-plot` renders [`oxiroot_hist`] objects (`TH1`/`TH2`/`TGraph`/
//! `TProfile`) to **SVG and PNG** with a matplotlib-like API and an mplhep-style
//! histogram look — no ROOT, no matplotlib, no system fonts. Everything is drawn
//! through one backend-independent [`draw`] IR that fans out to a tiny-skia
//! raster (PNG) and a hand-written SVG, so the two outputs share identical
//! geometry. DejaVu Sans (matplotlib's own default font) is bundled, and `$…$`
//! math is typeset with the ReX TeX engine into the same IR.
//!
//! ```no_run
//! use oxiroot_plot::Axes;
//! use oxiroot_hist::TH1;
//!
//! let mut h = TH1::new(50, 0.0, 100.0).named("pt");
//! h.fill(42.0);
//!
//! let mut ax = Axes::new();
//! ax.hist(&h);                       // mplhep step staircase
//! ax.set_xlabel("$p_T$ [GeV]");      // LaTeX math via ReX
//! ax.set_ylabel("Events");
//! ax.save("pt.png")?;                // or "pt.svg"
//! # Ok::<(), oxiroot_plot::Error>(())
//! ```

#![doc(html_root_url = "https://docs.rs/oxiroot-plot")]

pub mod artists;
pub mod axes;
pub mod cmap;
mod cmap_data;
pub mod color;
mod colorbar;
pub mod draw;
pub mod error;
pub mod figure;
pub mod gridspec;
pub mod legend;
pub mod mathtext;
pub mod render;
pub mod style;
pub mod text;
pub mod ticker;
pub mod transform;

pub use artists::{HistType, Marker};
pub use axes::{Axes, ErrorbarOpts, Hist2dOpts, HistOpts};
pub use cmap::Colormap;
pub use color::{cycle_color, Color, TAB10};
pub use error::{Error, Result};
pub use figure::{
    ratio_subplots, ratio_subplots_with, subplots, subplots_grid, subplots_grid_with,
    subplots_with, Figure, SaveOptions,
};
pub use gridspec::GridSpec;
pub use style::Style;

#[cfg(test)]
mod tests {
    use super::*;
    use oxiroot_hist::{TGraph, TH1, TH2};

    fn is_png(bytes: &[u8]) -> bool {
        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    }

    fn gauss_hist() -> TH1 {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut gauss = move || (0..12).map(|_| next()).sum::<f64>() - 6.0;
        let mut h = TH1::new(40, 50.0, 130.0).named("mass");
        h.sumw2();
        for _ in 0..20000 {
            h.fill(90.0 + 8.0 * gauss());
        }
        h
    }

    #[test]
    fn renders_shapes_and_text_both_backends() {
        use draw::{DrawCommand, DrawGroup, Rect, Stroke};
        let (w, h) = (320u32, 200u32);
        let mut g = DrawGroup::new(None);
        g.push(DrawCommand::Rect {
            rect: Rect::new(10.0, 10.0, 300.0, 180.0),
            fill: None,
            stroke: Some(Stroke::line(Color::BLACK, 1.0)),
        });
        g.extend(text::layout(
            "oxiroot 0123",
            20.0,
            60.0,
            24.0,
            text::FontStyle::Regular,
            Color::BLACK,
            text::HAlign::Left,
            text::VAlign::Baseline,
            0.0,
        ));
        let groups = [g];
        assert!(is_png(
            &render::raster::render_png(&groups, w, h, Color::WHITE).unwrap()
        ));
        let svg = render::svg::render(&groups, w, h, Color::WHITE);
        assert!(svg.starts_with("<svg") && svg.contains("</svg>") && svg.contains("<path"));
    }

    #[test]
    fn line_plot_autoscales() {
        let xs: Vec<f64> = (0..=100).map(|i| i as f64 * 0.1).collect();
        let ys: Vec<f64> = xs.iter().map(|x| x.sin()).collect();
        let mut ax = Axes::new();
        ax.plot(&xs, &ys);
        ax.set_xlabel("$x$ [rad]");
        let (w, h) = ax.style.figsize_px();
        assert!(is_png(
            &render::raster::render_png(&ax.render(w, h), w, h, Color::WHITE).unwrap()
        ));
    }

    #[test]
    fn hist_step_and_errorbar() {
        let h = gauss_hist();
        let mut ax = Axes::new();
        ax.histplot(&h, HistOpts::new().yerr(true));
        let (w, hh) = ax.style.figsize_px();
        let groups = ax.render(w, hh);
        assert!(is_png(
            &render::raster::render_png(&groups, w, hh, Color::WHITE).unwrap()
        ));
        // mplhep step + yerr emits many primitives (staircase + error bars).
        let cmds: usize = groups.iter().map(|g| g.cmds.len()).sum();
        assert!(cmds > 40, "expected a rich staircase, got {cmds} commands");
    }

    #[test]
    fn graph_with_legend() {
        let x: Vec<f64> = (0..6).map(|i| 60.0 + 12.0 * i as f64).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|x| 1500.0 * (-0.5 * ((x - 90.0) / 9.0).powi(2)).exp())
            .collect();
        let e: Vec<f64> = y.iter().map(|v| v.sqrt().max(10.0)).collect();
        let g = TGraph::with_errors(x.clone(), y, vec![6.0; x.len()], e).named("g");
        let mut ax = Axes::new();
        ax.errorbar_opts(&g, ErrorbarOpts::new().color(Color::BLACK).label("data"));
        ax.legend();
        let (w, h) = ax.style.figsize_px();
        assert!(is_png(
            &render::raster::render_png(&ax.render(w, h), w, h, Color::WHITE).unwrap()
        ));
    }

    #[test]
    fn hist2d_heatmap_with_colorbar() {
        let mut h2 = TH2::new(20, -3.0, 3.0, 20, -3.0, 3.0).named("h2");
        for ix in 0..20 {
            for iy in 0..20 {
                let x = -3.0 + (ix as f64 + 0.5) * 0.3;
                let y = -3.0 + (iy as f64 + 0.5) * 0.3;
                h2.fill_weight(x, y, (-(x * x + y * y) / 2.0).exp() * 100.0);
            }
        }
        let mut ax = Axes::new();
        ax.hist2dplot(&h2, Hist2dOpts::new().label("entries"));
        let (w, h) = ax.style.figsize_px();
        assert!(is_png(
            &render::raster::render_png(&ax.render(w, h), w, h, Color::WHITE).unwrap()
        ));
    }

    #[test]
    fn math_label_emits_glyph_paths() {
        use draw::{DrawCommand, DrawGroup};
        let mut g = DrawGroup::new(None);
        mathtext::layout_label(
            &mut g,
            "$\\frac{1}{\\sqrt{2\\pi}}\\, e^{-x^2/2}$",
            10.0,
            40.0,
            28.0,
            Color::BLACK,
            text::HAlign::Left,
            text::VAlign::Baseline,
            0.0,
        );
        // ReX should produce glyph outlines (paths) and at least one rule (polygon).
        let paths = g
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Path { .. }))
            .count();
        let rules = g
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Polygon { .. }))
            .count();
        assert!(paths > 5, "expected glyph paths, got {paths}");
        assert!(rules >= 1, "expected a fraction/radical rule, got {rules}");
    }

    #[test]
    fn gridspec_geometry() {
        // A 1×1 cell equals the margins box.
        let gs = GridSpec::new(1, 1);
        let (l, r, b, t) = gs.margins;
        let (w, h) = (640.0_f32, 480.0_f32);
        let cell = gs.cell_box(w, h, 0, 0, 0, 0);
        assert!((cell.x - l * w).abs() < 0.5);
        assert!((cell.w - (r - l) * w).abs() < 0.5);
        assert!((cell.y - (1.0 - t) * h).abs() < 0.5);
        assert!((cell.h - (t - b) * h).abs() < 0.5);

        // A 2-row ratio grid: panels touch and heights are 3:1.
        let gs2 = GridSpec::new(2, 1)
            .height_ratios(vec![3.0, 1.0])
            .hspace(0.0);
        let r0 = gs2.cell_box(w, h, 0, 0, 0, 0);
        let r1 = gs2.cell_box(w, h, 1, 1, 0, 0);
        assert!((r0.bottom() - r1.y).abs() < 0.5, "panels should touch");
        assert!((r0.h / r1.h - 3.0).abs() < 0.02, "height ratio 3:1");
    }

    #[test]
    fn pdf_output_is_structurally_valid() {
        let mut ax = Axes::new();
        ax.plot(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 0.4, 0.8]);
        ax.set_xlabel("x");
        let dir = std::env::temp_dir();
        let path = dir.join("oxiroot_plot_test.pdf");
        ax.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.4"), "PDF header");
        assert!(bytes.ends_with(b"%%EOF\n"), "PDF trailer");
        assert!(
            bytes.windows(4).any(|w| w == b"xref"),
            "PDF must have an xref table"
        );
        // The first xref offset should point at "1 0 obj".
        assert!(
            bytes.windows(8).any(|w| w == b"1 0 obj\n"),
            "object 1 present"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dpi_scales_the_raster() {
        let mut ax = Axes::new();
        ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
        let (w1, _) = ax.style.figsize_px();
        let g = ax.render(w1, ax.style.figsize_px().1);
        let _ = g;
        // figsize_px scales linearly with dpi.
        let mut hi = ax.style.clone();
        hi.dpi = 200.0;
        assert_eq!(hi.figsize_px().0, w1 * 2);
    }

    #[test]
    fn visual_dump() {
        let Ok(dir) = std::env::var("PLOT_DUMP") else {
            return;
        };
        let h = gauss_hist();

        // A single plot with a grid.
        let mut ax = Axes::new();
        ax.hist(&h);
        ax.grid(true);
        ax.set_xlabel("$m$ [GeV]");
        ax.set_ylabel("Events");
        ax.save(format!("{dir}/grid.png")).unwrap();
        ax.save(format!("{dir}/grid.pdf")).unwrap();
        ax.save_with(format!("{dir}/grid_hi.png"), &SaveOptions::new().dpi(220.0))
            .unwrap();

        // A 2×2 grid.
        let (fig, mut axs) = subplots_grid(2, 2);
        axs[0].hist(&h);
        axs[1].plot(&[0.0, 1.0, 2.0, 3.0], &[1.0, 3.0, 2.0, 4.0]);
        axs[2].hist(&h);
        axs[2].grid(true);
        axs[3].plot(&[0.0, 1.0, 2.0], &[2.0, 1.0, 3.0]);
        fig.with_axes(axs)
            .savefig(format!("{dir}/grid2x2.png"))
            .unwrap();

        // A ratio plot.
        let (fig, mut main, mut ratio) = ratio_subplots();
        main.histplot(&h, HistOpts::new().histtype(HistType::Fill).label("MC"));
        main.set_ylabel("Events");
        main.legend();
        let edges = h.edges();
        let centers: Vec<f64> = (0..h.values().len())
            .map(|i| 0.5 * (edges[i] + edges[i + 1]))
            .collect();
        let ones: Vec<f64> = centers.iter().map(|_| 1.0).collect();
        let r = TGraph::with_errors(
            centers.clone(),
            ones,
            vec![0.0; centers.len()],
            vec![0.08; centers.len()],
        )
        .named("r");
        ratio.errorbar_opts(&r, ErrorbarOpts::new().color(Color::BLACK));
        ratio.set_ylim(0.5, 1.5);
        ratio.set_ylabel("data/MC");
        ratio.set_xlabel("$m$ [GeV]");
        ratio.grid(true);
        fig.ratio(main, ratio)
            .savefig(format!("{dir}/ratio.png"))
            .unwrap();
    }
}
