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
pub use figure::{subplots, subplots_with, Figure};
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
}
