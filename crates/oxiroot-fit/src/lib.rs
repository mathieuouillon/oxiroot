//! Parametric curve fitting for **any 1-D data** — not just histograms.
//!
//! A fit needs only, per data point, an independent value `x`, a measured value
//! `y`, and a Gaussian uncertainty `σ`. Anything that can produce those — a
//! histogram's bins, a graph's points, or raw measurements — implements the
//! [`FitData`] trait and gains the [`FitExt`] fit methods:
//!
//! ```
//! use oxiroot_fit::{FitExt, Model, Points};
//!
//! // Raw (x, y, σ) measurements; the same API works for a TH1 or a TGraph.
//! let data = Points::new(&[0.0, 1.0, 2.0, 3.0], &[1.0, 3.0, 5.0, 7.0], &[0.1; 4]);
//! let line = Model::polynomial("line", 1).with_params(vec![0.0, 1.0]);
//! let fit = data.fit(&line); // χ² by default
//! assert!(fit.valid);
//! assert!((fit.params[0] - 1.0).abs() < 1e-6); // intercept ≈ 1
//! assert!((fit.params[1] - 2.0).abs() < 1e-6); // slope ≈ 2
//! ```
//!
//! The minimizer is the pure-Rust [Minuit2](https://crates.io/crates/minuit2)
//! port (the algorithm ROOT uses). [`Model`] is a named parametric fit function
//! with optional per-parameter limits/fixing/steps — built from a closure, a
//! built-in shape ([`Model::gaussian`]/[`exponential`](Model::exponential)/
//! [`polynomial`](Model::polynomial)), or an arbitrary formula string
//! ([`Model::from_formula`]). Costs: Neyman χ², Pearson χ², or binned Poisson
//! likelihood ([`FitMethod`]).
//!
//! The evaluable, persistable ROOT function objects `TF1`/`TF2`/`TF3` live in
//! [`oxiroot_hist`](https://crates.io/crates/oxiroot-hist); a `TF1` converts to a
//! `Model` for fitting.

mod data;
mod engine;
mod model;
mod result;

pub use data::{FitData, FitExt, Point, Points};
pub use model::Model;
pub use result::{FitMethod, FitOptions, FitResult, Minimizer};
