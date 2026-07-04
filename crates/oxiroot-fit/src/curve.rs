//! scipy-style [`curve_fit`] — fit a bare closure `f(x, params)` to `(x, y)`
//! data from an initial guess, without naming a [`Model`] first.

use crate::data::{FitExt, Points};
use crate::model::Model;
use crate::result::{FitOptions, FitResult};

/// Fit `f(x, params)` to the data `(x, y)` by (unweighted) least squares from the
/// initial guess `p0` — the analogue of
/// [`scipy.optimize.curve_fit`](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.curve_fit.html).
/// Parameters are named `p0`, `p1`, … automatically; `result.params` are the
/// best-fit values and `result.covariance` their covariance (scipy's
/// `popt`/`pcov`).
///
/// For per-point errors, a robust [`Loss`](crate::Loss), a fit range, or a
/// different cost, use [`curve_fit_opts`]; for bounds, build a [`Model`] and use
/// its [`fit`](FitExt::fit) with `.lower_limit(...)`.
///
/// ```
/// use oxiroot_fit::curve_fit;
/// let x = [0.0, 1.0, 2.0, 3.0, 4.0];
/// let y = [1.0, 3.0, 5.0, 7.0, 9.0]; // y = 1 + 2x
/// let fit = curve_fit(|x, p| p[0] + p[1] * x, &x, &y, &[0.0, 0.0]);
/// assert!((fit.params[0] - 1.0).abs() < 1e-6); // intercept ≈ 1
/// assert!((fit.params[1] - 2.0).abs() < 1e-6); // slope ≈ 2
/// ```
#[must_use]
pub fn curve_fit(
    f: impl Fn(f64, &[f64]) -> f64 + Send + Sync + 'static,
    x: &[f64],
    y: &[f64],
    p0: &[f64],
) -> FitResult {
    Points::unweighted(x, y).fit(&anon_model(f, p0))
}

/// Like [`curve_fit`], with per-point Gaussian errors `sigma` and full
/// [`FitOptions`] — the way to reach a robust [`Loss`](crate::Loss), a Pearson or
/// likelihood cost, or a fit range from the bare-closure entry point.
///
/// ```
/// use oxiroot_fit::{curve_fit, curve_fit_opts, FitOptions, Loss};
/// let x: Vec<f64> = (0..20).map(|i| i as f64).collect();
/// let mut y: Vec<f64> = x.iter().map(|&x| 1.0 + 2.0 * x).collect();
/// y[7] += 50.0; // an outlier
/// let sigma = vec![1.0; x.len()];
///
/// // A plain least-squares fit is dragged toward the outlier…
/// let plain = curve_fit(|x, p| p[0] + p[1] * x, &x, &y, &[0.0, 0.0]);
/// // …but a Huber-robust fit shrugs it off. Robust losses have a small basin of
/// // attraction, so seed near the answer (here, from the plain fit above).
/// let robust = curve_fit_opts(
///     |x, p| p[0] + p[1] * x, &x, &y, &sigma, &plain.params,
///     &FitOptions::new().loss(Loss::Huber),
/// );
/// assert!((plain.params[1] - 2.0).abs() > 0.1);   // plain slope pulled off 2
/// assert!((robust.params[1] - 2.0).abs() < 0.02); // robust slope back near 2
/// ```
#[must_use]
pub fn curve_fit_opts(
    f: impl Fn(f64, &[f64]) -> f64 + Send + Sync + 'static,
    x: &[f64],
    y: &[f64],
    sigma: &[f64],
    p0: &[f64],
    opts: &FitOptions,
) -> FitResult {
    Points::new(x, y, sigma).fit_opts(&anon_model(f, p0), opts)
}

/// A nameless [`Model`] with parameters `p0`, `p1`, … wrapping `f`.
fn anon_model(f: impl Fn(f64, &[f64]) -> f64 + Send + Sync + 'static, p0: &[f64]) -> Model {
    let names: Vec<String> = (0..p0.len()).map(|k| format!("p{k}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    Model::new("curve_fit", &refs, p0.to_vec(), f)
}
