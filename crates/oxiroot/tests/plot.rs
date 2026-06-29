//! Facade-level integration test: drive the whole plotting stack the way a user
//! would — through `oxiroot::plot` + `oxiroot::prelude` — and, with the `fit`
//! feature, fit a model and overlay it. The whole file compiles to nothing unless
//! the `plot` feature is on, so it is a no-op under the default `cargo test`.
#![cfg(feature = "plot")]

use oxiroot::plot::{Axes, Color, HistOpts, HistType, SaveOpts};
use oxiroot::prelude::*;

fn gauss_hist() -> TH1 {
    let mut h = TH1::new(50, 0.0, 100.0).named("pt");
    let mut s = 0xC0FF_EE12_3456_789Au64;
    let mut next = move || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) as f64 / (1u64 << 31) as f64
    };
    let mut gauss = move || (0..12).map(|_| next()).sum::<f64>() - 6.0;
    for _ in 0..4000 {
        h.fill(50.0 + 12.0 * gauss());
    }
    h
}

#[test]
fn facade_plots_a_histogram_to_every_format() {
    let h = gauss_hist();
    let mut ax = Axes::new();
    ax.hist_with(
        &h,
        HistOpts::new()
            .histtype(HistType::Fill)
            .fill_color(Color::hex("#1f77b4").with_alpha(0.4))
            .label("MC"),
    );
    ax.xlabel("$p_T$ [GeV]");
    ax.ylabel("Events");
    ax.legend();

    let svg = ax.to_svg_string();
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    assert!(ax
        .to_png_bytes(SaveOpts::new())
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert!(ax.to_pdf_bytes().starts_with(b"%PDF-1.4"));

    // And through a real file (format chosen by extension).
    let path = std::env::temp_dir().join("oxiroot_facade_plot.svg");
    ax.save(&path).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("</svg>"));
    let _ = std::fs::remove_file(&path);
}

#[cfg(feature = "fit")]
#[test]
fn facade_fits_and_overlays_a_model() {
    use oxiroot::fit::TF1;

    let h = gauss_hist();
    let model = TF1::gaussian("g").estimate_from(&h);
    let result = h.fit(&model);
    assert!(result.ndf > 0, "a 50-bin Gaussian fit has positive ndf");
    let fitted = model.with_params(result.params.clone());

    let mut ax = Axes::new();
    ax.hist_with(&h, HistOpts::new().histtype(HistType::Fill).label("data"));
    ax.model(&fitted, 0.0..100.0);
    ax.legend();

    // The fitted curve is one extra polyline over the histogram staircase.
    assert!(ax.to_svg_string().contains("<polyline"));
}
