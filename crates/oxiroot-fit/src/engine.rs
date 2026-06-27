//! The Minuit2-backed minimizer: turns a set of [`Point`]s, a [`Model`], and
//! [`FitOptions`] into a [`FitResult`]. Data-source agnostic — it only ever sees
//! `(x, y, σ)` triples, so it fits histograms, graphs, and custom data alike.

use minuit2::{MnMigrad, MnMinos};

use crate::data::Point;
use crate::model::Model;
use crate::result::{FitMethod, FitOptions, FitResult};

/// Fit `model` to `all` under `opts`. The points entering the fit are those with
/// `x` in `opts.range` (default: all); Neyman chi-square additionally drops
/// points with no usable error (`sigma <= 0`).
pub(crate) fn run_fit(all: &[Point], model: &Model, opts: &FitOptions) -> FitResult {
    let np = model.params.len();
    // Neyman chi-square uses only points with an error; the others use every one.
    let drop_empty = matches!(opts.method, FitMethod::Chi2);
    let points: Vec<(f64, f64, f64)> = all
        .iter()
        .filter_map(|p| {
            if let Some((lo, hi)) = opts.range {
                if p.x < lo || p.x > hi {
                    return None;
                }
            }
            (!drop_empty || p.sigma > 0.0).then_some((p.x, p.y, p.sigma))
        })
        .collect();

    // Free (non-fixed) parameters determine the degrees of freedom.
    let n_free = (0..np)
        .filter(|&i| !model.constraints.get(i).map(|c| c.fixed).unwrap_or(false))
        .count();

    // Too few data points to determine the free parameters: report failure
    // rather than a meaningless minimum of a flat cost.
    if np == 0 || points.len() < n_free.max(1) {
        return FitResult {
            params: model.params.clone(),
            errors: vec![f64::NAN; np],
            minos: None,
            covariance: None,
            chi2: f64::NAN,
            ndf: 0,
            valid: false,
        };
    }

    let func = &model.func;
    let method = opts.method;
    let cost = |p: &[f64]| -> f64 {
        match method {
            FitMethod::Chi2 => points
                .iter()
                .map(|&(x, y, e)| {
                    let d = (y - func(x, p)) / e;
                    d * d
                })
                .sum(),
            FitMethod::PearsonChi2 => points
                .iter()
                .map(|&(x, y, _)| {
                    let f = func(x, p).max(1e-300); // expected (model) variance
                    (y - f) * (y - f) / f
                })
                .sum(),
            FitMethod::Likelihood => {
                2.0 * points
                    .iter()
                    .map(|&(x, y, _)| {
                        let f = func(x, p).max(1e-300);
                        if y > 0.0 {
                            f - y + y * (y / f).ln()
                        } else {
                            f
                        }
                    })
                    .sum::<f64>()
            }
        }
    };

    let mut migrad = MnMigrad::new();
    for (i, (name, &init)) in model.param_names.iter().zip(&model.params).enumerate() {
        let c = model.constraints.get(i).copied().unwrap_or_default();
        // Initial Minuit2 step: the hint, else 10 % of the value with a floor.
        let step = c.step.unwrap_or_else(|| (init.abs() * 0.1).max(0.01));
        migrad = if c.fixed {
            migrad.add_const(name, init)
        } else {
            match (c.lower, c.upper) {
                (Some(lo), Some(hi)) => migrad.add_limited(name, init, step, lo, hi),
                (Some(lo), None) => migrad.add_lower_limited(name, init, step, lo),
                (None, Some(hi)) => migrad.add_upper_limited(name, init, step, hi),
                (None, None) => migrad.add(name, init, step),
            }
        };
    }
    let min = migrad.minimize(&cost);

    // Read results by index (external/user space) — robust against duplicate
    // parameter names.
    let params = min.params();
    let errors = min.user_state().errors();

    // Covariance of the free parameters (row-major), when Minuit2 has one.
    let covariance = min.user_state().covariance().map(|cov| {
        let m = cov.nrow();
        (0..m)
            .map(|i| (0..m).map(|j| cov.get(i, j)).collect())
            .collect()
    });

    // Asymmetric MINOS errors per parameter, on request and only at a valid
    // minimum. A fixed parameter is pinned, so its error is (0, 0); a free one
    // gets a likelihood scan (`lower_error` is ≤ 0, `upper_error` is ≥ 0).
    let minos = (opts.minos && min.is_valid()).then(|| {
        let scan = MnMinos::new(&cost, &min);
        (0..np)
            .map(|par| {
                if model.constraints.get(par).map(|c| c.fixed).unwrap_or(false) {
                    (0.0, 0.0)
                } else {
                    let e = scan.minos(par);
                    (e.lower_error(), e.upper_error())
                }
            })
            .collect()
    });

    FitResult {
        params,
        errors,
        minos,
        covariance,
        chi2: min.fval(),
        ndf: points.len() - n_free,
        valid: min.is_valid(),
    }
}
