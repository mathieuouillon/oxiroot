//! [`FitData`] — the data abstraction every fit consumes — and [`FitExt`], the
//! blanket trait that gives every dataset the `.fit(...)` methods.

use oxiroot_stat::StatError;

use crate::engine::run_fit;
use crate::model::Model;
use crate::result::{FitOptions, FitResult};

/// One data point a model is fit against: an independent value `x`, a measured
/// value `y`, and a Gaussian uncertainty `sigma` on `y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Independent variable.
    pub x: f64,
    /// Measured value at `x`.
    pub y: f64,
    /// Gaussian uncertainty on `y`. A value `<= 0` means "no usable error":
    /// χ² (Neyman) fits skip such points, the other costs keep them.
    pub sigma: f64,
}

impl Point {
    /// A point `(x, y)` with uncertainty `sigma`.
    #[must_use]
    pub fn new(x: f64, y: f64, sigma: f64) -> Point {
        Point { x, y, sigma }
    }
}

/// A 1-D dataset a parametric [`Model`] can be fit to. Implement it for any data
/// source — a histogram's bins, a graph's points, raw measurements — and the
/// blanket [`FitExt`] gives it `.fit(...)` for free.
///
/// `points` returns every candidate point; the fit engine applies the range
/// filter and (for Neyman χ²) the empty-point cut itself.
pub trait FitData {
    /// The data as `(x, y, sigma)` points.
    fn points(&self) -> Vec<Point>;
}

/// A standalone dataset of raw `(x, y, sigma)` points — for fitting data that is
/// neither a histogram nor a graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Points {
    points: Vec<Point>,
}

impl Points {
    /// Build from parallel `x`, `y`, and `sigma` slices.
    ///
    /// # Errors
    /// [`StatError::LengthMismatch`] if `y` or `sigma` is not as long as `x`
    /// (`left` is the length of `x`).
    pub fn new(x: &[f64], y: &[f64], sigma: &[f64]) -> Result<Points, StatError> {
        paired(x.len(), y.len())?;
        paired(x.len(), sigma.len())?;
        let points = x
            .iter()
            .zip(y)
            .zip(sigma)
            .map(|((&x, &y), &s)| Point::new(x, y, s))
            .collect();
        Ok(Points { points })
    }

    /// Build from `x`/`y` with unit (unweighted) errors — an ordinary
    /// least-squares fit.
    ///
    /// # Errors
    /// [`StatError::LengthMismatch`] if `y` is not as long as `x`.
    pub fn unweighted(x: &[f64], y: &[f64]) -> Result<Points, StatError> {
        paired(x.len(), y.len())?;
        let points = x
            .iter()
            .zip(y)
            .map(|(&x, &y)| Point::new(x, y, 1.0))
            .collect();
        Ok(Points { points })
    }

    /// Build directly from [`Point`]s.
    #[must_use]
    pub fn from_points(points: Vec<Point>) -> Points {
        Points { points }
    }

    /// The points.
    #[must_use]
    pub fn as_points(&self) -> &[Point] {
        &self.points
    }
}

impl FitData for Points {
    fn points(&self) -> Vec<Point> {
        self.points.clone()
    }
}

/// Fit a bare collection of points. Because the [`FitExt`] blanket is `?Sized`,
/// implementing `FitData` on the unsized `[Point]` also gives `.fit(...)` to
/// `&[Point]`, `[Point; N]`, and `Vec<Point>` (via deref/unsizing).
impl FitData for [Point] {
    fn points(&self) -> Vec<Point> {
        self.to_vec()
    }
}

/// Fit methods for any [`FitData`]. Implemented for all of them by a blanket
/// impl, so `data.fit(&model)` works for a `Hist1D`, a `Graph`, or [`Points`]
/// alike. Bring it into scope (`use oxiroot_fit::FitExt;`, or the `oxiroot`
/// prelude) to call these.
pub trait FitExt: FitData {
    /// Fit `model` by chi-square minimization over the full range (ROOT's
    /// default `Fit`) — shorthand for [`fit_opts`](Self::fit_opts) with the
    /// default [`FitOptions`]. To pick a different cost, a range, or a robust
    /// loss, build a [`FitOptions`] and use [`fit_opts`](Self::fit_opts).
    #[must_use]
    fn fit(&self, model: &Model) -> FitResult {
        self.fit_opts(model, &FitOptions::new())
    }

    /// Fit `model` with full control over the cost and range ([`FitOptions`]),
    /// seeded from `model.params` and its per-parameter constraints. The points
    /// entering the fit are those whose `x` is in `opts.range` (default: all);
    /// Neyman chi-square additionally drops points with no error (`sigma <= 0`).
    /// `FitResult::chi2` is the cost at the minimum.
    #[must_use]
    fn fit_opts(&self, model: &Model, opts: &FitOptions) -> FitResult {
        run_fit(&self.points(), model, opts)
    }

    /// Fit `model` (per `opts`) and write the best-fit parameters back into it,
    /// so `model.eval(x)` evaluates the *fitted* curve afterwards. Use
    /// [`fit_opts`](Self::fit_opts) to keep the model's initial parameters.
    fn fit_into(&self, model: &mut Model, opts: &FitOptions) -> FitResult {
        let result = self.fit_opts(model, opts);
        model.params.clone_from(&result.params);
        result
    }
}

impl<T: FitData + ?Sized> FitExt for T {}

/// Reject paired inputs whose lengths differ.
fn paired(left: usize, right: usize) -> Result<(), StatError> {
    if left == right {
        Ok(())
    } else {
        Err(StatError::LengthMismatch { left, right })
    }
}
