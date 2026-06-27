//! Fitting for histograms and graphs.
//!
//! The fitting engine lives in the standalone [`oxiroot_fit`](oxiroot_fit)
//! crate, which works on any 1-D data. This module just teaches it to read a
//! [`TH1`] and a [`TGraph`] by implementing [`FitData`], so the `.fit(...)`
//! methods (from [`FitExt`]) work directly:
//!
//! ```ignore
//! use oxiroot_hist::{Model, FitExt};
//! let model = Model::gaussian("g").estimate_from(&h);
//! let fit = h.fit(&model); // or g.fit(&model) for a TGraph
//! println!("mean = {} ± {}", fit.params[1], fit.errors[1]);
//! ```
//!
//! Behind the `fit` feature (it pulls in `oxiroot-fit` → the pure-Rust Minuit2
//! port). The whole `oxiroot-fit` surface is re-exported here for convenience.

pub use oxiroot_fit::{
    FitData, FitExt, FitMethod, FitOptions, FitResult, Model, Point, Points, TF1,
};

use crate::graph::{GraphErrors, TGraph};
use crate::th1::TH1;

/// Each in-range bin becomes a point `(center, content, error)`. An empty bin
/// has error 0, so Neyman chi-square drops it (as ROOT does); the other costs
/// keep it.
impl FitData for TH1 {
    fn points(&self) -> Vec<Point> {
        let n = self.xaxis.nbins.max(0) as usize;
        (1..=n)
            .map(|i| {
                let y = self.contents.get(i).copied().unwrap_or(0.0);
                Point::new(self.bin_center(i), y, self.bin_error(i))
            })
            .collect()
    }
}

/// Each graph point becomes `(x, y, σ)`, with `σ` the y-error bar: the symmetric
/// `ey`, the mean of the asymmetric `(ey_low, ey_high)`, or `1.0` (an unweighted
/// least-squares fit) when the graph carries no errors.
impl FitData for TGraph {
    fn points(&self) -> Vec<Point> {
        self.x
            .iter()
            .zip(&self.y)
            .enumerate()
            .map(|(i, (&x, &y))| Point::new(x, y, graph_sigma(&self.errors, i)))
            .collect()
    }
}

fn graph_sigma(errors: &GraphErrors, i: usize) -> f64 {
    match errors {
        GraphErrors::None => 1.0,
        GraphErrors::Symmetric { ey, .. } => ey.get(i).copied().unwrap_or(1.0),
        GraphErrors::Asymmetric {
            ey_low, ey_high, ..
        } => {
            let lo = ey_low.get(i).copied().unwrap_or(0.0);
            let hi = ey_high.get(i).copied().unwrap_or(0.0);
            (lo + hi) / 2.0
        }
    }
}
