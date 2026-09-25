//! ROOT's parametric functions `TF1`/`TF2`/`TF3` — here [`Func1D`], [`Func2D`]
//! and [`Func3D`] — backed by the [`oxiroot_formula`] expression engine.
//!
//! A function is a formula (`"[0]*sin([1]*x)"`, `"gaus"`, `"expo"`, `"pol2"`, …),
//! its parameter values, and a range. It evaluates in pure Rust ([`eval`](Func1D::eval),
//! [`integral`](Func1D::integral), [`derivative`](Func1D::derivative)) and reads/writes
//! as an ordinary ROOT `TF1`/`TF2`/`TF3` key (embedding a `TFormula`), so ROOT
//! C++ and uproot read what oxiroot writes and vice versa.
//!
//! ```
//! use oxiroot_hist_func::Func1D;
//! let f = Func1D::new("f", "[0]*sin([1]*x) + [2]", 0.0, 6.283).unwrap()
//!     .with_params(vec![2.0, 1.5, 0.5]);
//! assert!((f.eval(1.0) - 2.494_990).abs() < 1e-6);
//! ```
//!
//! The functions are part of the histogram family: they describe `Func1D` with
//! its captured streamer info ([`oxiroot_hist::hist_streamer_classes`]) and store
//! their record as an [`oxiroot_hist::GraphFunction`], the same `Func1D` body a graph's
//! `fFunctions` list holds. They live in their own crate so a histogram-only build
//! does not compile the formula engine. The `oxiroot` facade re-exports them as
//! `oxiroot::hist::{Func1D, Func2D, Func3D}` and in its prelude.
//!
//! Features:
//! - `fit`: `Func1D::to_model`, which turns a function into an `oxiroot_fit::Model`.

mod func;
mod io;
mod sample;

pub use func::{Func1D, Func2D, Func3D};
