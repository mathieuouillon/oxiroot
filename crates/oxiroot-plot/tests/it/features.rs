//! What a build without the default features still does: plot any data that
//! implements the data traits, to SVG and PDF, and refuse PNG with an error that
//! names the missing feature.

use oxiroot_plot::{Axes, Hist1dData, Hist2dData, PointData, SaveOpts};

/// A histogram that is not an oxiroot type.
struct Counts {
    edges: Vec<f64>,
    counts: Vec<f64>,
}

impl Hist1dData for Counts {
    fn edges(&self) -> Vec<f64> {
        self.edges.clone()
    }
    fn values(&self) -> Vec<f64> {
        self.counts.clone()
    }
    fn error(&self, i: usize) -> f64 {
        self.counts[i].sqrt()
    }
}

/// A 2x2 grid.
struct Grid;

impl Hist2dData for Grid {
    fn x_edges(&self) -> Vec<f64> {
        vec![0.0, 1.0, 2.0]
    }
    fn y_edges(&self) -> Vec<f64> {
        vec![0.0, 1.0, 2.0]
    }
    fn grid(&self) -> Vec<Vec<f64>> {
        vec![vec![1.0, 2.0], vec![3.0, 0.0]]
    }
}

/// Points with symmetric y errors.
struct Measurements;

impl PointData for Measurements {
    fn xs(&self) -> Vec<f64> {
        vec![1.0, 2.0, 3.0]
    }
    fn ys(&self) -> Vec<f64> {
        vec![2.0, 4.0, 3.0]
    }
    fn y_errors(&self) -> Option<(Vec<f64>, Vec<f64>)> {
        Some((vec![0.5; 3], vec![0.5; 3]))
    }
}

#[test]
fn any_data_implementing_the_traits_plots() {
    let counts = Counts {
        edges: vec![0.0, 1.0, 2.0, 3.0],
        counts: vec![4.0, 9.0, 1.0],
    };
    let mut ax = Axes::new();
    ax.hist(&counts).profile(&counts).errorbar(&Measurements);
    let svg = ax.to_svg_string();
    assert!(svg.starts_with("<svg") && svg.contains("<path"));
    assert!(ax.to_pdf_bytes().starts_with(b"%PDF"));

    let mut heat = Axes::new();
    heat.hist2d(&Grid);
    assert!(heat.to_svg_string().contains("</svg>"));
}

#[test]
#[cfg(not(feature = "png"))]
fn png_output_without_the_png_feature_is_a_clear_error() {
    let mut ax = Axes::new();
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]).unwrap();
    let err = ax.to_png_bytes(SaveOpts::new()).unwrap_err();
    assert!(err.to_string().contains("`png` feature"), "{err}");
    let dir = std::env::temp_dir();
    assert!(ax.save(dir.join("oxiroot_plot_no_png.png")).is_err());
    // SVG and PDF need no feature.
    assert!(ax.to_svg_string().starts_with("<svg"));
    assert!(ax.to_pdf_bytes().starts_with(b"%PDF"));
}

#[test]
#[cfg(feature = "png")]
fn png_output_with_the_png_feature_works() {
    let mut ax = Axes::new();
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]).unwrap();
    let png = ax.to_png_bytes(SaveOpts::new()).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}
