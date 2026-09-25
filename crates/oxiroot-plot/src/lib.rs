//! Pure-Rust plotting for ROOT histograms and graphs.
//!
//! `oxiroot-plot` renders histograms and graphs to **SVG, PNG, and PDF** with a
//! matplotlib-like API and an mplhep-style histogram look — no ROOT, no
//! matplotlib, no system fonts. It draws the `oxiroot-hist` types
//! (`Hist1D`/`Hist2D`/`Graph`/`Profile1D`, the `hist` feature) and any other data that
//! implements [`Hist1dData`], [`Hist2dData`] or [`PointData`].
//! Everything is drawn through one backend-independent draw IR that fans out
//! to a tiny-skia raster (PNG), a hand-written SVG, and a hand-written PDF, so
//! the three outputs share identical geometry. The default font is STIX Two (a
//! LaTeX-like serif; see [`FontSet`]), and `$…$` math is typeset with the ReX
//! TeX engine into the same IR.
//!
//! The `hist`, `png` (PNG output) and `math` (TeX math) features are on by
//! default. Without them, SVG and PDF still render, math spans are laid out as
//! plain text, and a PNG request returns [`Error::MissingFeature`].
//!
//! # What it can draw
//!
//! - **Histograms** — [`Axes::hist`]/[`Axes::hist_with`] draw a `Hist1D` as an mplhep
//!   staircase ([`HistType::Step`]/`Fill`/`Band`/`Errorbar`) with `√N`/Sumw2
//!   error bars.
//! - **Graphs & profiles** — [`Axes::errorbar`] (`Graph`, any error variant) and
//!   [`Axes::profile`] (`Profile1D`); [`Axes::plot`] for raw `(x, y)`.
//! - **2-D histograms** — [`Axes::hist2d`]/[`Axes::hist2d_with`] render a `Hist2D` as
//!   a color mesh with a colorbar and the real matplotlib `viridis`/`plasma`
//!   [`Colormap`]s. [`Hist2dOpts::log`] (or [`Hist2dOpts::norm`] with a [`Norm`])
//!   switches to a log / symlog color scale with a decade colorbar, like
//!   matplotlib's `LogNorm`/`SymLogNorm`.
//! - **Curves** — [`Axes::function`] overlays any analytic closure; with the
//!   `fit` feature, `Axes::model` overlays a fitted `oxiroot_fit::Model`.
//! - **Decoration** — `xlabel`/`ylabel`/`title` (with LaTeX), `xlim`/`ylim`,
//!   [`Axes::legend`], and [`Axes::grid`].
//! - **Layouts** — [`subplots_grid`] and a custom [`GridSpec`] for multi-panel
//!   figures, and [`ratio_subplots`] for the HEP main-over-ratio plot.
//! - **Output** — [`Axes::save`]/[`Figure::save`] choose the format from the
//!   file extension (`.png`, `.svg`, `.pdf`); [`SaveOpts`] sets the DPI for a
//!   sharper PNG or a transparent background, and `to_png_bytes`/`to_svg_string`
//!   render in memory. [`PdfPages`] collects several figures into one multi-page
//!   vector PDF (matplotlib's `PdfPages`).
//!
//! The default look reproduces a plain matplotlib figure; [`Style::mplhep`]
//! switches to the in-pointing, all-sides, minor-tick HEP style.
//!
//! # A histogram with data points
//!
//! ```no_run
//! # #[cfg(feature = "hist")] {
//! use oxiroot_plot::{Axes, Color, ErrorbarOpts, HistOpts, HistType};
//! use oxiroot_hist::{Hist, Graph};
//!
//! let mut mc = Hist::reg(50, 0.0, 100.0).double().named("mc");
//! mc.sumw2();
//! for x in [40.0, 48.0, 50.0, 52.0, 60.0] {
//!     mc.fill(x);
//! }
//! let data = Graph::with_errors(vec![50.0], vec![3.0], vec![0.0], vec![1.7]).unwrap().named("d");
//!
//! let mut ax = Axes::new();
//! ax.hist_with(&mc, HistOpts::new().histtype(HistType::Fill).label("MC"));
//! ax.errorbar_with(&data, ErrorbarOpts::new().color(Color::BLACK).label("data"));
//! ax.xlabel("$p_T$ [GeV]");   // LaTeX math via ReX
//! ax.ylabel("Events");
//! ax.legend();
//! ax.save("pt.png")?;         // or "pt.svg" / "pt.pdf"
//! # }
//! # Ok::<(), oxiroot_plot::Error>(())
//! ```
//!
//! # A ratio plot
//!
//! ```no_run
//! # #[cfg(feature = "hist")] {
//! use oxiroot_plot::{ratio_subplots, Color, ErrorbarOpts, HistOpts, HistType};
//! use oxiroot_hist::{Hist, Graph};
//!
//! let mc = Hist::reg(50, 0.0, 100.0).double().named("mc");
//! let ratio_points = Graph::with_errors(vec![50.0], vec![1.0], vec![0.0], vec![0.1]).unwrap().named("r");
//!
//! let (fig, mut main, mut ratio) = ratio_subplots();
//! main.hist_with(&mc, HistOpts::new().histtype(HistType::Fill).label("MC"));
//! main.ylabel("Events");
//! main.legend();
//! ratio.errorbar_with(&ratio_points, ErrorbarOpts::new().color(Color::BLACK));
//! ratio.ylim(0.5..1.5);
//! ratio.ylabel("data/MC");
//! ratio.xlabel("$p_T$ [GeV]");
//! fig.ratio(main, ratio).save("ratio.pdf")?;
//! # }
//! # Ok::<(), oxiroot_plot::Error>(())
//! ```

#![doc(html_root_url = "https://docs.rs/oxiroot-plot")]

// The modules are private: every public item is re-exported at the crate root
// below, so each has one path.
mod artists;
mod axes;
mod cmap;
mod cmap_data;
mod color;
mod colorbar;
mod data;
mod draw;
mod error;
mod figure;
mod fonts;
mod gridspec;
mod legend;
mod mathtext;
mod norm;
mod render;
#[cfg(feature = "fit")]
mod statbox;
mod style;
mod text;
mod ticker;
mod timeaxis;
mod transform;

pub use artists::{HistType, Marker, ParseHistTypeError, ParseMarkerError};
pub use axes::{Axes, CurveOpts, ErrorbarOpts, Hist2dOpts, HistOpts};
pub use cmap::{Colormap, ParseColormapError};
pub use color::{Color, ParseColorError, TAB10};
pub use data::{Hist1dData, Hist2dData, PointData};
pub use error::{Error, Result};
pub use figure::{
    ratio_subplots, ratio_subplots_with, subplots, subplots_grid, subplots_grid_with,
    subplots_with, Figure, PdfPages, SaveOpts,
};
pub use fonts::FontSet;
pub use gridspec::GridSpec;
pub use norm::Norm;
#[cfg(feature = "fit")]
pub use statbox::{Corner, StatBox};
pub use style::{Sides, Style, TickDir};
pub use timeaxis::{format_time, TimeFormat};

#[cfg(test)]
mod tests {
    use super::*;
    // The render IR, text/math layout, and backends are private to the crate;
    // the tests reach them through `crate::` (still accessible in-crate).
    use crate::{draw, mathtext, render, text};
    #[cfg(feature = "hist")]
    use oxiroot_hist::{Graph, Hist, Hist1D};

    /// `groups` render to a PNG, or, without the `png` feature, to the error
    /// that names it; they render to an SVG either way.
    fn assert_renders(groups: &[draw::DrawGroup], w: u32, h: u32) {
        let png = render::raster::render_png(groups, w, h, Color::WHITE);
        #[cfg(feature = "png")]
        assert!(png.unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        #[cfg(not(feature = "png"))]
        assert!(matches!(
            png,
            Err(Error::MissingFeature { feature: "png", .. })
        ));
        let svg = render::svg::render(groups, w, h, Color::WHITE);
        assert!(svg.starts_with("<svg") && svg.contains("</svg>"));
    }

    #[cfg(feature = "hist")]
    fn gauss_hist() -> Hist1D {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut gauss = move || (0..12).map(|_| next()).sum::<f64>() - 6.0;
        let mut h = Hist::reg(40, 50.0, 130.0).double().named("mass");
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
        let fonts = FontSet::stix();
        g.extend(text::layout(
            &fonts,
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
        assert_renders(&groups, w, h);
        let svg = render::svg::render(&groups, w, h, Color::WHITE);
        assert!(svg.starts_with("<svg") && svg.contains("</svg>") && svg.contains("<path"));
    }

    #[test]
    fn line_plot_autoscales() {
        let xs: Vec<f64> = (0..=100).map(|i| i as f64 * 0.1).collect();
        let ys: Vec<f64> = xs.iter().map(|x| x.sin()).collect();
        let mut ax = Axes::new();
        ax.plot(&xs, &ys).unwrap();
        ax.xlabel("$x$ [rad]");
        let (w, h) = ax.style.figsize_px();
        assert_renders(&ax.render(w, h), w, h);
    }

    #[test]
    #[cfg(feature = "hist")]
    fn hist_step_and_errorbar() {
        let h = gauss_hist();
        let mut ax = Axes::new();
        ax.hist_with(&h, HistOpts::new().yerr());
        let (w, hh) = ax.style.figsize_px();
        let groups = ax.render(w, hh);
        assert_renders(&groups, w, hh);
        // mplhep step + yerr emits many primitives (staircase + error bars).
        let cmds: usize = groups.iter().map(|g| g.cmds.len()).sum();
        assert!(cmds > 40, "expected a rich staircase, got {cmds} commands");
    }

    #[test]
    #[cfg(feature = "hist")]
    fn graph_with_legend() {
        let x: Vec<f64> = (0..6).map(|i| 60.0 + 12.0 * i as f64).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|x| 1500.0 * (-0.5 * ((x - 90.0) / 9.0).powi(2)).exp())
            .collect();
        let e: Vec<f64> = y.iter().map(|v| v.sqrt().max(10.0)).collect();
        let g = Graph::with_errors(x.clone(), y, vec![6.0; x.len()], e)
            .unwrap()
            .named("g");
        let mut ax = Axes::new();
        ax.errorbar_with(&g, ErrorbarOpts::new().color(Color::BLACK).label("data"));
        ax.legend();
        let (w, h) = ax.style.figsize_px();
        assert_renders(&ax.render(w, h), w, h);
    }

    #[test]
    #[cfg(feature = "hist")]
    fn hist2d_heatmap_with_colorbar() {
        let mut h2 = Hist::reg(20, -3.0, 3.0)
            .reg(20, -3.0, 3.0)
            .double()
            .named("h2");
        for ix in 0..20 {
            for iy in 0..20 {
                let x = -3.0 + (ix as f64 + 0.5) * 0.3;
                let y = -3.0 + (iy as f64 + 0.5) * 0.3;
                h2.fill_weight(x, y, (-(x * x + y * y) / 2.0).exp() * 100.0);
            }
        }
        let mut ax = Axes::new();
        ax.hist2d_with(&h2, Hist2dOpts::new().label("entries"));
        let (w, h) = ax.style.figsize_px();
        assert_renders(&ax.render(w, h), w, h);
    }

    #[test]
    fn hist2d_leaves_empty_bins_as_background() {
        use artists::{Artist, MeshArtist};
        use cmap::Colormap;
        use draw::{DrawCommand, DrawGroup, Rect};
        use style::Style;
        use transform::Transform;

        // Count the rectangles a mesh emits: one per *filled* cell, none for the
        // empty (content 0) cells — they are left to show the background.
        let paint = |values: Vec<Vec<f64>>| -> usize {
            let edges = vec![0.0, 1.0, 2.0, 3.0];
            let mesh = MeshArtist {
                xedges: edges.clone(),
                yedges: edges,
                values,
                cmap: Colormap::Viridis,
                vmin: 1.0,
                vmax: 9.0,
                norm: norm::Norm::Linear,
            };
            let t = Transform::new(Rect::new(0.0, 0.0, 300.0, 300.0), 0.0, 3.0, 0.0, 3.0);
            let mut g = DrawGroup::new(None);
            Artist::Mesh(mesh).draw(&t, &Style::default(), &mut g);
            g.cmds
                .iter()
                .filter(|c| matches!(c, DrawCommand::Rect { .. }))
                .count()
        };

        // 3x3 grid with only two filled cells → two rects (seven empties undrawn).
        let sparse = vec![
            vec![5.0, 0.0, 0.0],
            vec![0.0, 0.0, 0.0],
            vec![0.0, 0.0, 9.0],
        ];
        assert_eq!(paint(sparse), 2, "empty bins must not be painted");
        // A fully-filled grid paints every cell.
        assert_eq!(paint(vec![vec![1.0; 3]; 3]), 9);
    }

    #[test]
    #[cfg(feature = "math")]
    fn math_label_emits_glyph_paths() {
        use draw::{DrawCommand, DrawGroup};
        let fonts = FontSet::stix();
        let mut g = DrawGroup::new(None);
        mathtext::layout_label(
            &mut g,
            &fonts,
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

    /// The vertical ink extent `(top, bottom)` of the glyph paths and rules in
    /// `g` (y grows downward), from their end and control points.
    #[cfg(feature = "math")]
    fn ink_y_extent(g: &draw::DrawGroup) -> (f32, f32) {
        use draw::{DrawCommand, Seg};
        let mut ys = Vec::new();
        for c in &g.cmds {
            match c {
                DrawCommand::Path { path, .. } => {
                    for s in &path.segs {
                        match *s {
                            Seg::Move(_, y) | Seg::Line(_, y) => ys.push(y),
                            Seg::Quad(_, y1, _, y2) => ys.extend([y1, y2]),
                            Seg::Cubic(_, y1, _, y2, _, y3) => ys.extend([y1, y2, y3]),
                            Seg::Close => {}
                        }
                    }
                }
                DrawCommand::Polygon { pts, .. } => ys.extend(pts.iter().map(|p| p.1)),
                _ => {}
            }
        }
        let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        (top, bottom)
    }

    #[test]
    #[cfg(feature = "math")]
    fn math_glyphs_have_the_text_size() {
        // An `x` set as text and as math at the same size has the same height:
        // STIX Two Text and STIX Two Math share their metrics (matplotlib sets
        // mathtext at the text's point size too). ReX takes its size in points,
        // so handing it pixels drew math 4/3 too large.
        let fonts = FontSet::stix();
        let height = |label: &str| {
            let mut g = draw::DrawGroup::new(None);
            mathtext::layout_label(
                &mut g,
                &fonts,
                label,
                0.0,
                0.0,
                100.0,
                Color::BLACK,
                text::HAlign::Left,
                text::VAlign::Baseline,
                0.0,
            );
            let (top, bottom) = ink_y_extent(&g);
            bottom - top
        };
        let (text, math) = (height("x"), height("$x$"));
        assert!(
            (math / text - 1.0).abs() < 0.05,
            "math x is {math:.2} px tall, text x {text:.2} px"
        );
    }

    #[test]
    #[cfg(feature = "math")]
    fn bottom_aligned_math_label_stays_above_its_anchor() {
        // `VAlign::Bottom` puts the bottom of the label at `y` (a title sits on
        // the frame this way). A fraction's denominator hangs below the
        // baseline, so the label's descent must count it.
        let fonts = FontSet::stix();
        let mut g = draw::DrawGroup::new(None);
        mathtext::layout_label(
            &mut g,
            &fonts,
            "yield $\\frac{a}{b}$",
            0.0,
            200.0,
            40.0,
            Color::BLACK,
            text::HAlign::Left,
            text::VAlign::Bottom,
            0.0,
        );
        let (_, bottom) = ink_y_extent(&g);
        assert!(
            bottom <= 200.5,
            "the label's ink reaches y = {bottom}, below 200"
        );
    }

    /// The frame: the clip rectangle of the data groups.
    fn frame(groups: &[draw::DrawGroup]) -> draw::Rect {
        groups
            .iter()
            .find_map(|g| g.clip)
            .expect("an axes renders a clipped data group")
    }

    /// An axes with one line, and `title` if given.
    fn titled_axes(title: Option<&str>) -> Axes {
        let mut ax = Axes::new();
        ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]).unwrap();
        if let Some(t) = title {
            ax.title(t);
        }
        ax
    }

    #[test]
    #[cfg(feature = "math")]
    fn a_tall_title_stays_inside_the_figure() {
        // A display-style stacked fraction is taller than the top margin: the
        // frame moves down to make room instead of the title leaving the figure.
        let title = "$\\displaystyle\\frac{\\frac{a}{b}}{\\frac{c}{d}}$ vs. $m$";
        let groups = titled_axes(Some(title)).render(640, 480);
        let top = groups
            .iter()
            .map(|g| ink_y_extent(g).0)
            .fold(f32::INFINITY, f32::min);
        assert!(
            top >= 0.0,
            "the title's ink reaches y = {top}, above the figure"
        );
        let plain = frame(&titled_axes(None).render(640, 480));
        assert!(frame(&groups).y > plain.y, "the frame did not move down");
    }

    #[test]
    fn an_ordinary_title_leaves_the_frame_alone() {
        let plain = frame(&titled_axes(None).render(640, 480));
        for title in ["Z candidates", "$Z \\rightarrow \\mu\\mu$ candidates"] {
            let with_title = frame(&titled_axes(Some(title)).render(640, 480));
            assert_eq!(with_title, plain, "{title:?} moved the frame");
        }
    }

    #[test]
    #[cfg(feature = "math")]
    fn inline_math_is_set_in_text_style() {
        // A `$…$` span is inline math: TeX's text style, which sets a
        // fraction's numerator and denominator smaller than display style does
        // (`\\displaystyle` switches back to display style).
        let fonts = FontSet::stix();
        let height = |label: &str| {
            let mut g = draw::DrawGroup::new(None);
            mathtext::layout_label(
                &mut g,
                &fonts,
                label,
                0.0,
                0.0,
                100.0,
                Color::BLACK,
                text::HAlign::Left,
                text::VAlign::Baseline,
                0.0,
            );
            let (top, bottom) = ink_y_extent(&g);
            bottom - top
        };
        let inline = height("$\\frac{a}{b}$");
        let display = height("$\\displaystyle\\frac{a}{b}$");
        assert!(
            inline < 0.9 * display,
            "inline fraction {inline:.1} px vs display {display:.1} px"
        );
    }

    #[test]
    #[cfg(feature = "math")]
    fn malformed_or_zero_size_math_labels_do_not_panic() {
        use draw::{DrawCommand, DrawGroup};
        let fonts = FontSet::stix();
        let draw = |label: &str, size: f32| {
            let mut g = DrawGroup::new(None);
            mathtext::layout_label(
                &mut g,
                &fonts,
                label,
                0.0,
                0.0,
                size,
                Color::BLACK,
                text::HAlign::Left,
                text::VAlign::Middle,
                0.0,
            );
            g
        };
        // An array row with more cells than columns does not parse (as in
        // LaTeX), so the label falls back to its source as plain text.
        let g = draw("$\\begin{array}{l} a & b \\end{array}$", 20.0);
        assert!(g.cmds.iter().any(|c| matches!(c, DrawCommand::Path { .. })));
        // ReX's glyph assembly overflowed at a zero size.
        for size in [0.0, -1.0, f32::NAN] {
            draw(
                "$\\overbrace{\\begin{array}{c} a \\\\ b \\end{array}}$",
                size,
            );
        }
    }

    #[test]
    #[cfg(not(feature = "math"))]
    fn math_label_without_the_math_feature_is_plain_text() {
        use draw::{DrawCommand, DrawGroup};
        let mut g = DrawGroup::new(None);
        mathtext::layout_label(
            &mut g,
            &FontSet::stix(),
            "$\\mathrm{p}_{T}$ [GeV]",
            10.0,
            40.0,
            28.0,
            Color::BLACK,
            text::HAlign::Left,
            text::VAlign::Baseline,
            0.0,
        );
        // Glyph outlines for the stripped source text, and no TeX rules.
        assert!(g.cmds.iter().any(|c| matches!(c, DrawCommand::Path { .. })));
        assert!(!g
            .cmds
            .iter()
            .any(|c| matches!(c, DrawCommand::Polygon { .. })));
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
        ax.plot(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 0.4, 0.8])
            .unwrap();
        ax.xlabel("x");
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
        ax.plot(&[0.0, 1.0], &[0.0, 1.0]).unwrap();
        let (w1, _) = ax.style.figsize_px();
        let g = ax.render(w1, ax.style.figsize_px().1);
        let _ = g;
        // figsize_px scales linearly with dpi.
        let mut hi = ax.style.clone();
        hi.dpi = 200.0;
        assert_eq!(hi.figsize_px().0, w1 * 2);
    }

    #[test]
    #[cfg(feature = "hist")]
    fn visual_dump() {
        let Ok(dir) = std::env::var("PLOT_DUMP") else {
            return;
        };
        let h = gauss_hist();

        // A single plot with a grid.
        let mut ax = Axes::new();
        ax.hist(&h);
        ax.grid();
        ax.xlabel("$m$ [GeV]");
        ax.ylabel("Events");
        ax.save(format!("{dir}/grid.png")).unwrap();
        ax.save(format!("{dir}/grid.pdf")).unwrap();
        ax.save_with(format!("{dir}/grid_hi.png"), SaveOpts::new().dpi(220.0))
            .unwrap();

        // A 2×2 grid.
        let (fig, mut axs) = subplots_grid(2, 2);
        axs[0].hist(&h);
        axs[1]
            .plot(&[0.0, 1.0, 2.0, 3.0], &[1.0, 3.0, 2.0, 4.0])
            .unwrap();
        axs[2].hist(&h);
        axs[2].grid();
        axs[3].plot(&[0.0, 1.0, 2.0], &[2.0, 1.0, 3.0]).unwrap();
        fig.with_axes(axs)
            .save(format!("{dir}/grid2x2.png"))
            .unwrap();

        // A ratio plot.
        let (fig, mut main, mut ratio) = ratio_subplots();
        main.hist_with(&h, HistOpts::new().histtype(HistType::Fill).label("MC"));
        main.ylabel("Events");
        main.legend();
        let edges = h.edges();
        let centers: Vec<f64> = (0..h.values().len())
            .map(|i| 0.5 * (edges[i] + edges[i + 1]))
            .collect();
        let ones: Vec<f64> = centers.iter().map(|_| 1.0).collect();
        let r = Graph::with_errors(
            centers.clone(),
            ones,
            vec![0.0; centers.len()],
            vec![0.08; centers.len()],
        )
        .unwrap()
        .named("r");
        ratio.errorbar_with(&r, ErrorbarOpts::new().color(Color::BLACK));
        ratio.ylim(0.5..1.5);
        ratio.ylabel("data/MC");
        ratio.xlabel("$m$ [GeV]");
        ratio.grid();
        fig.ratio(main, ratio)
            .save(format!("{dir}/ratio.png"))
            .unwrap();

        // A shared-axis 2×2 grid with a figure title.
        let (fig, mut axs) = subplots_grid(2, 2);
        for ax in &mut axs {
            ax.hist(&h);
        }
        axs[1]
            .plot(&[55.0, 90.0, 125.0], &[500.0, 1500.0, 400.0])
            .unwrap();
        fig.sharex()
            .sharey()
            .suptitle("$Z\\to\\mu\\mu$ — shared grid")
            .with_axes(axs)
            .save(format!("{dir}/shared.png"))
            .unwrap();

        // A function overlay on a histogram (e.g. a fitted Gaussian).
        let mut ax = Axes::new();
        ax.hist_with(&h, HistOpts::new().histtype(HistType::Fill).label("data"));
        let (a, mu, sigma) = (2050.0_f64, 90.0_f64, 8.0_f64);
        ax.function_with(
            move |x| a * (-0.5 * ((x - mu) / sigma).powi(2)).exp(),
            50.0..130.0,
            CurveOpts::new().color(Color::hex("#d62728")).label("fit"),
        );
        ax.xlabel("$m$ [GeV]");
        ax.ylabel("Events");
        ax.legend();
        ax.save(format!("{dir}/overlay.png")).unwrap();
    }

    /// Overlaying a fitted `Model` adds a curve (one polyline) on top of the
    /// histogram. Lives in-crate because it needs the optional `oxiroot_fit`
    /// dependency, which is only present under the `fit` feature (and the
    /// histogram needs `hist`).
    #[cfg(all(feature = "fit", feature = "hist"))]
    #[test]
    fn model_overlay_adds_a_curve() {
        use oxiroot_fit::Model;
        let h = gauss_hist();
        let model = Model::gaussian("g").with_params(vec![4000.0, 90.0, 8.0]);

        let mut base = Axes::new();
        base.hist(&h);
        let polylines_before = base.to_svg_string().matches("<polyline").count();

        let mut overlaid = Axes::new();
        overlaid.hist(&h);
        overlaid.model(&model, 50.0..130.0);
        let svg = overlaid.to_svg_string();
        let polylines_after = svg.matches("<polyline").count();

        assert_eq!(
            polylines_after,
            polylines_before + 1,
            "the model overlay should add exactly one curve polyline"
        );
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    }
    /// A histogram whose axis is a time axis draws its labels as times: the
    /// axes take the format from the data, and the ticks step in hours rather
    /// than in decimals.
    #[test]
    #[cfg(feature = "hist")]
    fn a_time_axis_is_picked_up_from_the_data_it_plots() {
        let mut h = Hist::reg(24, 0.0, 86_400.0).double().named("rate");
        h.xaxis.set_time_format("%H:%M%F2024-01-01 00:00:00");
        h.fill(3600.0);

        let mut ax = Axes::new();
        ax.hist(&h);
        let time = ax.x_time_format_for_test().expect("adopted from the data");
        assert_eq!(time.format, "%H:%M");
        assert_eq!(time.label(3600.0), "01:00");

        // What the caller sets wins over what the data carries.
        let mut ax = Axes::new();
        ax.x_time_format("%d/%m").hist(&h);
        assert_eq!(ax.x_time_format_for_test().unwrap().format, "%d/%m");

        // A histogram with an ordinary axis stays numeric.
        let plain = Hist::reg(4, 0.0, 4.0).double().named("plain");
        let mut ax = Axes::new();
        ax.hist(&plain);
        assert!(ax.x_time_format_for_test().is_none());

        // A graph takes it from its display frame, as ROOT stores it.
        let mut frame = Hist::reg(2, 0.0, 7200.0).float().named("Graph");
        frame.xaxis.set_time_format("%H:%M");
        let mut g = Graph::new(vec![0.0, 3600.0], vec![1.0, 2.0]).unwrap();
        g.histogram = Some(frame);
        let mut ax = Axes::new();
        ax.errorbar(&g);
        assert_eq!(ax.x_time_format_for_test().unwrap().format, "%H:%M");

        // And it renders.
        let svg = ax.to_svg_string();
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    }
}
