//! High-level rendering tests that drive the public `oxiroot_plot` API end to end
//! and assert real properties of the output: the format chosen by extension, the
//! in-memory render bytes, DPI scaling, determinism, SVG structure, and that every
//! artist / layout actually renders. No pixel comparison — only structural and
//! invariant checks that won't flake.

use oxiroot_hist::{TGraph, TProfile, TH1, TH2};
use oxiroot_plot::{
    ratio_subplots, subplots, subplots_grid, Axes, Color, CurveOpts, Error, ErrorbarOpts, FontSet,
    Hist2dOpts, HistOpts, HistType, SaveOpts, Style,
};

// --- deterministic fixtures (a tiny LCG → reproducible bytes, no rng dep) ---

fn gauss_hist() -> TH1 {
    let mut h = TH1::new(40, 50.0, 130.0)
        .named("mass")
        .titled("di-muon mass");
    h.sumw2();
    let mut s = 0x1234_5678_9abc_def0u64;
    let mut next = move || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) as f64 / (1u64 << 31) as f64
    };
    let mut gauss = move || (0..12).map(|_| next()).sum::<f64>() - 6.0;
    for _ in 0..5000 {
        h.fill(90.0 + 8.0 * gauss());
    }
    h
}

fn graph() -> TGraph {
    let x: Vec<f64> = (0..8).map(|i| 55.0 + 10.0 * i as f64).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|v| 1000.0 * (-0.5 * ((v - 90.0) / 9.0).powi(2)).exp())
        .collect();
    let e: Vec<f64> = y.iter().map(|v| v.sqrt().max(5.0)).collect();
    TGraph::with_errors(x.clone(), y, vec![5.0; x.len()], e).named("data")
}

fn th2() -> TH2 {
    let mut h = TH2::new(20, -3.0, 3.0, 20, -3.0, 3.0).named("h2");
    for ix in 0..20 {
        for iy in 0..20 {
            let x = -3.0 + (ix as f64 + 0.5) * 0.3;
            let y = -3.0 + (iy as f64 + 0.5) * 0.3;
            h.fill_weight(x, y, (-(x * x + y * y) / 2.0).exp() * 100.0);
        }
    }
    h
}

fn profile() -> TProfile {
    let mut p = TProfile::new(10, 0.0, 10.0).named("p");
    for i in 0..400 {
        let x = (i % 10) as f64 + 0.5;
        p.fill(x, 1.5 * x + (i % 3) as f64);
    }
    p
}

/// Decode just the width/height from a PNG's IHDR (big-endian at bytes 16..24).
fn png_dims(b: &[u8]) -> (u32, u32) {
    assert!(b.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
    let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
    (w, h)
}

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// A unique scratch dir for a test that writes files.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oxiroot_plot_test_{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// --- output format selection ---------------------------------------------------

#[test]
fn save_picks_format_from_extension() {
    let dir = scratch("formats");
    let mut ax = Axes::new();
    ax.hist(&gauss_hist());
    ax.xlabel("$m$ [GeV]");

    ax.save(dir.join("p.png")).unwrap();
    ax.save(dir.join("p.svg")).unwrap();
    ax.save(dir.join("p.pdf")).unwrap();

    let png = std::fs::read(dir.join("p.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "PNG magic");

    let svg = std::fs::read_to_string(dir.join("p.svg")).unwrap();
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));

    let pdf = std::fs::read(dir.join("p.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"), "PDF header");
    assert!(pdf.ends_with(b"%%EOF\n"), "PDF trailer");
    assert!(pdf.windows(4).any(|w| w == b"xref"), "PDF xref table");
}

#[test]
fn unknown_extension_is_an_error_that_mentions_the_formats() {
    let dir = scratch("badext");
    let mut ax = Axes::new();
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    let err = ax.save(dir.join("p.jpg")).unwrap_err();
    assert!(matches!(err, Error::UnknownFormat(_)));
    let msg = err.to_string();
    assert!(msg.contains("jpg") && msg.contains("pdf"), "got: {msg}");
}

// --- in-memory rendering -------------------------------------------------------

#[test]
fn in_memory_render_produces_each_format() {
    let mut ax = Axes::new();
    ax.hist(&gauss_hist());

    let png = ax.to_png_bytes(SaveOpts::new()).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

    let svg = ax.to_svg_string();
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));

    let pdf = ax.to_pdf_bytes();
    assert!(pdf.starts_with(b"%PDF-1.4") && pdf.ends_with(b"%%EOF\n"));
}

#[test]
fn file_and_in_memory_bytes_agree() {
    let dir = scratch("agree");
    let mut ax = Axes::new();
    ax.hist(&gauss_hist());
    ax.xlabel("$m$");

    ax.save(dir.join("a.png")).unwrap();
    ax.save(dir.join("a.svg")).unwrap();
    assert_eq!(
        std::fs::read(dir.join("a.png")).unwrap(),
        ax.to_png_bytes(SaveOpts::new()).unwrap(),
        "save(.png) must equal to_png_bytes"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("a.svg")).unwrap(),
        ax.to_svg_string(),
        "save(.svg) must equal to_svg_string"
    );
}

#[test]
fn hep_label_adds_glyphs_above_the_frame() {
    use oxiroot_plot::Style;
    // The bold experiment label + italic status + right-hand lumi/energy each add
    // glyph paths to the SVG, so a labelled axes renders more `<path` than a bare
    // one with the same data.
    let mut bare = Axes::with_style(Style::mplhep());
    bare.hist(&gauss_hist());
    let n_bare = occurrences(&bare.to_svg_string(), "<path");

    let mut labelled = Axes::with_style(Style::mplhep());
    labelled.hist(&gauss_hist());
    labelled
        .hep_label("CMS", "Preliminary")
        .hep_rhs("138 fb$^{-1}$ (13 TeV)");
    let n_labelled = occurrences(&labelled.to_svg_string(), "<path");

    assert!(
        n_labelled > n_bare + 15,
        "expected the experiment label to add many glyph paths: {n_bare} -> {n_labelled}"
    );
}

#[test]
fn rendering_is_deterministic() {
    // Two independently built identical plots must produce identical bytes.
    let render = || {
        let mut ax = Axes::new();
        ax.hist_with(
            &gauss_hist(),
            HistOpts::new().histtype(HistType::Fill).label("MC"),
        );
        ax.errorbar_with(
            &graph(),
            ErrorbarOpts::new().color(Color::BLACK).label("data"),
        );
        ax.xlabel("$m_{\\mu\\mu}$ [GeV]");
        ax.legend();
        ax
    };
    let a = render();
    let b = render();
    assert_eq!(a.to_svg_string(), b.to_svg_string());
    assert_eq!(
        a.to_png_bytes(SaveOpts::new()).unwrap(),
        b.to_png_bytes(SaveOpts::new()).unwrap()
    );
    assert_eq!(a.to_pdf_bytes(), b.to_pdf_bytes());
}

// --- save options --------------------------------------------------------------

#[test]
fn dpi_scales_the_png_raster() {
    let mut ax = Axes::new();
    ax.hist(&gauss_hist());
    let base = png_dims(&ax.to_png_bytes(SaveOpts::new()).unwrap());
    let hi = png_dims(&ax.to_png_bytes(SaveOpts::new().dpi(200.0)).unwrap());
    // The default style is 100 dpi, so 200 dpi doubles both dimensions exactly.
    assert_eq!(hi.0, base.0 * 2, "width doubles at 2x dpi");
    assert_eq!(hi.1, base.1 * 2, "height doubles at 2x dpi");
    // Vector formats are resolution-independent: the SVG viewBox is unchanged.
    assert!(ax
        .to_svg_string()
        .contains(&format!("viewBox=\"0 0 {} {}\"", base.0, base.1)));
}

#[test]
fn transparency_changes_the_raster() {
    let mut ax = Axes::new();
    ax.hist(&gauss_hist());
    let opaque = ax.to_png_bytes(SaveOpts::new()).unwrap();
    let transparent = ax.to_png_bytes(SaveOpts::new().transparent()).unwrap();
    assert_ne!(
        opaque, transparent,
        "the transparent flag must affect the output"
    );
    // Same geometry, different pixels.
    assert_eq!(png_dims(&opaque), png_dims(&transparent));
}

// --- SVG structure & content ---------------------------------------------------

#[test]
fn svg_is_well_formed_and_sized() {
    let mut ax = Axes::new();
    ax.hist(&gauss_hist());
    let svg = ax.to_svg_string();
    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>"));
    assert!(
        svg.contains("width=\"640\" height=\"480\""),
        "default figure size"
    );
    assert!(svg.contains("viewBox=\"0 0 640 480\""));
    // There is always an opaque background rect and a frame.
    assert!(svg.contains("<rect"));
}

#[test]
fn math_and_text_labels_emit_glyph_paths() {
    let mut plain = Axes::new();
    plain.hist(&gauss_hist());
    let n_plain = occurrences(&plain.to_svg_string(), "<path");

    let mut labelled = Axes::new();
    labelled.hist(&gauss_hist());
    labelled.xlabel("$\\chi^2 / \\mathrm{ndf}$");
    labelled.ylabel("Events");
    labelled.title("$Z \\to \\mu\\mu$");
    let svg = labelled.to_svg_string();
    let n_labelled = occurrences(&svg, "<path");

    // Tick numerals already produce glyph paths; the labels add many more.
    assert!(n_plain > 5, "tick labels are glyph paths, got {n_plain}");
    assert!(
        n_labelled > n_plain + 10,
        "axis labels + math should add glyphs ({n_plain} -> {n_labelled})"
    );
}

#[test]
fn legend_adds_a_frame_and_label_glyphs() {
    let h = gauss_hist();
    let mut without = Axes::new();
    without.hist_with(&h, HistOpts::new().label("MC"));
    let n_without = occurrences(&without.to_svg_string(), "<path");

    let mut with = Axes::new();
    with.hist_with(&h, HistOpts::new().label("MC"));
    with.legend();
    let svg = with.to_svg_string();

    assert!(
        occurrences(&svg, "<path") > n_without,
        "legend label adds glyphs"
    );
    // The default style draws a translucent white legend frame.
    assert!(svg.contains("fill-opacity=\"0.8\""), "legend frame box");
}

// --- every artist type renders -------------------------------------------------

#[test]
fn all_histtypes_render() {
    let h = gauss_hist();
    for t in [
        HistType::Step,
        HistType::Fill,
        HistType::Errorbar,
        HistType::Band,
    ] {
        let mut ax = Axes::new();
        ax.hist_with(&h, HistOpts::new().histtype(t).yerr().label("h"));
        let svg = ax.to_svg_string();
        assert!(
            svg.starts_with("<svg") && svg.ends_with("</svg>"),
            "{t} failed"
        );
        assert!(occurrences(&svg, "<path") > 0, "{t} drew nothing");
    }
}

#[test]
fn graph_profile_plot_and_function_render() {
    // symmetric + asymmetric error graphs
    let sym = graph();
    let asym = TGraph::with_asymm_errors(
        vec![1.0, 2.0, 3.0],
        vec![3.0, 4.0, 3.5],
        vec![0.1, 0.1, 0.1],
        vec![0.2, 0.2, 0.2],
        vec![0.3, 0.1, 0.2],
        vec![0.1, 0.3, 0.2],
    )
    .named("asym");
    for g in [&sym, &asym] {
        let mut ax = Axes::new();
        ax.errorbar(g);
        assert!(ax.to_svg_string().contains("<svg"));
    }

    let mut ax = Axes::new();
    ax.profile(&profile());
    assert!(ax
        .to_png_bytes(SaveOpts::new())
        .unwrap()
        .starts_with(b"\x89PNG"));

    let xs: Vec<f64> = (0..50).map(|i| i as f64 * 0.2).collect();
    let ys: Vec<f64> = xs.iter().map(|x| x.sin()).collect();
    let mut ax = Axes::new();
    ax.plot(&xs, &ys);
    ax.function(|x| (x / 2.0).cos(), 0.0..10.0);
    let svg = ax.to_svg_string();
    assert!(
        occurrences(&svg, "<polyline") >= 2,
        "a line plot + a function curve"
    );
}

#[test]
fn function_with_options_styles_the_curve() {
    let mut ax = Axes::new();
    ax.function_with(
        |x| (x - 5.0_f64).powi(2),
        0.0..10.0,
        CurveOpts::new()
            .color(Color::hex("#d62728"))
            .linewidth(3.0)
            .dashed(vec![6.0, 3.0])
            .samples(64)
            .label("parabola"),
    );
    ax.legend();
    let svg = ax.to_svg_string();
    assert!(svg.contains("stroke=\"#d62728\""), "custom curve color");
    assert!(svg.contains("stroke-dasharray="), "dashed curve");
}

#[test]
fn hist2d_draws_a_mesh_and_colorbar() {
    let mut ax = Axes::new();
    ax.hist2d_with(&th2(), Hist2dOpts::new().label("entries"));
    let svg = ax.to_svg_string();
    // A 20x20 mesh plus a colorbar gradient => lots of filled rects.
    assert!(
        occurrences(&svg, "<rect") > 100,
        "expected a dense color mesh, got {} rects",
        occurrences(&svg, "<rect")
    );
}

#[test]
fn hist2d_leaves_empty_bins_as_background() {
    // A fully-filled mesh draws every cell; a mostly-empty one draws far fewer,
    // because empty bins (no data) are not painted — they show the page
    // background instead of the colormap's value-0 color.
    let rects = |h: &TH2| {
        let mut ax = Axes::new();
        ax.hist2d(h);
        occurrences(&ax.to_svg_string(), "<rect")
    };

    let mut full = TH2::new(10, 0.0, 1.0, 10, 0.0, 1.0).named("full");
    for ix in 0..10 {
        for iy in 0..10 {
            full.fill_weight(0.05 + ix as f64 * 0.1, 0.05 + iy as f64 * 0.1, 1.0);
        }
    }
    let mut sparse = TH2::new(10, 0.0, 1.0, 10, 0.0, 1.0).named("sparse");
    sparse.fill_weight(0.05, 0.05, 5.0);
    sparse.fill_weight(0.55, 0.55, 3.0); // only two filled cells

    let (nf, ns) = (rects(&full), rects(&sparse));
    // Background, frame, and colorbar rects are identical between the two; the
    // ~98-cell difference is exactly the skipped empty bins.
    assert!(
        nf >= ns + 90,
        "empty bins must be skipped: full={nf} rects, sparse={ns} rects"
    );
}

// --- figures and layouts -------------------------------------------------------

#[test]
fn subplots_grid_returns_one_axes_per_cell() {
    let (_fig, axs2) = subplots_grid(2, 2);
    assert_eq!(axs2.len(), 4);
    let (_fig, axs6) = subplots_grid(2, 3);
    assert_eq!(axs6.len(), 6);
}

#[test]
fn figure_grid_renders_all_panels() {
    let h = gauss_hist();
    let (fig, mut axs) = subplots_grid(2, 2);
    axs[0].hist(&h);
    axs[1].errorbar(&graph());
    axs[2].hist2d(&th2());
    axs[3].plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]);
    let png = fig.with_axes(axs).to_png_bytes(SaveOpts::new()).unwrap();
    let (w, hgt) = png_dims(&png);
    assert!(w >= 600 && hgt >= 400);
}

#[test]
fn shared_axes_and_suptitle_add_content() {
    let h = gauss_hist();
    let build = |suptitle: bool| {
        let (mut fig, mut axs) = subplots_grid(1, 2);
        axs[0].hist(&h);
        axs[1].hist(&h);
        fig = fig.sharex().sharey();
        if suptitle {
            fig = fig.suptitle("$Z \\to \\mu\\mu$");
        }
        fig.with_axes(axs).to_svg_string()
    };
    let plain = build(false);
    let titled = build(true);
    assert!(plain.starts_with("<svg") && titled.starts_with("<svg"));
    // The suptitle is typeset as extra glyph paths.
    assert!(
        occurrences(&titled, "<path") > occurrences(&plain, "<path"),
        "suptitle should add glyphs"
    );
}

#[test]
fn ratio_subplots_renders() {
    let (fig, mut main, mut ratio) = ratio_subplots();
    main.hist_with(
        &gauss_hist(),
        HistOpts::new().histtype(HistType::Fill).label("MC"),
    );
    main.ylabel("Events");
    main.legend();
    ratio.errorbar_with(&graph(), ErrorbarOpts::new().color(Color::BLACK));
    ratio.ylim(0.5..1.5);
    ratio.ylabel("data/MC");
    ratio.xlabel("$m$ [GeV]");
    ratio.grid();
    let pdf = fig.ratio(main, ratio).to_pdf_bytes();
    assert!(pdf.starts_with(b"%PDF-1.4") && pdf.ends_with(b"%%EOF\n"));
}

#[test]
fn figure_in_memory_render_all_formats() {
    let (fig, mut ax) = subplots();
    ax.hist(&gauss_hist());
    let fig = fig.with_axes([ax]);
    assert!(fig
        .to_png_bytes(SaveOpts::new())
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert!(fig.to_svg_string().starts_with("<svg"));
    assert!(fig.to_pdf_bytes().starts_with(b"%PDF-1.4"));
}

// --- edge cases ----------------------------------------------------------------

#[test]
fn empty_axes_renders_without_panicking() {
    let ax = Axes::new();
    assert!(ax.to_svg_string().contains("<svg"));
    assert!(ax
        .to_png_bytes(SaveOpts::new())
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert!(ax.to_pdf_bytes().starts_with(b"%PDF-1.4"));
}

#[test]
fn mplhep_style_renders() {
    let mut ax = Axes::with_style(Style::mplhep());
    ax.hist_with(&gauss_hist(), HistOpts::new().yerr().label("MC"));
    ax.grid_minor();
    ax.legend();
    assert!(ax.to_svg_string().contains("<svg"));
}

// --- fonts ---------------------------------------------------------------------

#[test]
fn builtin_font_sets_render() {
    let h = gauss_hist();
    for fonts in [FontSet::stix(), FontSet::dejavu(), FontSet::default()] {
        let mut ax = Axes::new();
        ax.fonts(fonts);
        ax.hist(&h);
        ax.xlabel("$\\Sigma$ [GeV]");
        let svg = ax.to_svg_string();
        assert!(svg.starts_with("<svg") && svg.contains("<path"));
    }
}

#[test]
fn font_choice_changes_the_glyphs() {
    // Switching the font set must actually change the rendered glyph paths.
    let render = |fonts: FontSet| {
        let mut ax = Axes::new();
        ax.fonts(fonts);
        ax.xlabel("Events / 2 GeV");
        ax.hist(&gauss_hist());
        ax.to_svg_string()
    };
    assert_ne!(
        render(FontSet::stix()),
        render(FontSet::dejavu()),
        "STIX and DejaVu should produce different glyph outlines"
    );
}

#[test]
fn custom_text_font_from_bytes_renders() {
    // A real font, embedded from the crate's own assets (so the test is
    // environment-independent), drives the custom-font path.
    static DEJAVU: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
    let fonts = FontSet::from_font(DEJAVU).expect("DejaVu is a valid font");
    let mut ax = Axes::new();
    ax.fonts(fonts);
    ax.hist(&gauss_hist());
    ax.xlabel("custom font");
    assert!(ax.to_svg_string().contains("<path"));
}

#[test]
fn custom_text_and_math_fonts_render() {
    static DEJAVU: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
    static MATH: &[u8] = include_bytes!("../assets/STIXTwoMath-Regular.otf");
    let fonts = FontSet::from_fonts(DEJAVU, MATH).expect("valid text + math fonts");
    let mut ax = Axes::new();
    ax.fonts(fonts);
    ax.xlabel("$\\frac{1}{\\sqrt{2}}$"); // exercises the custom math font
    ax.hist(&gauss_hist());
    assert!(ax.to_svg_string().contains("<path"));
}

#[test]
fn custom_font_rejects_garbage() {
    assert!(FontSet::from_font(b"definitely not a font").is_err());
    static DEJAVU: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
    assert!(
        FontSet::from_fonts(DEJAVU, b"not a math font").is_err(),
        "a non-font math argument must be rejected"
    );
}
