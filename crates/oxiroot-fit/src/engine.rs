//! The Minuit2-backed minimizer: turns a set of [`Point`]s, a [`Model`], and
//! [`FitOptions`] into a [`FitResult`]. Data-source agnostic — it only ever sees
//! `(x, y, σ)` triples, so it fits histograms, graphs, and custom data alike.

use minuit2::{MnMigrad, MnMinos};

use crate::data::Point;
use crate::model::Model;
use crate::result::{FitMethod, FitOptions, FitResult, Minimizer};

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

    let ndf = points.len() - n_free;
    match opts.minimizer {
        Minimizer::Minuit2 => minimize_minuit2(model, &cost, ndf, opts.minos),
        #[cfg(feature = "argmin")]
        Minimizer::NelderMead => minimize_argmin(model, &cost, ndf),
    }
}

/// Minimize with the Minuit2 (MIGRAD) backend — the default. Gives parabolic
/// errors, the free-parameter covariance, and (on request) MINOS.
fn minimize_minuit2(
    model: &Model,
    cost: &impl Fn(&[f64]) -> f64,
    ndf: usize,
    want_minos: bool,
) -> FitResult {
    let np = model.params.len();
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
    let min = migrad.minimize(cost);

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
    let minos = (want_minos && min.is_valid()).then(|| {
        let scan = MnMinos::new(cost, &min);
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
        ndf,
        valid: min.is_valid(),
    }
}

// --- Optional gradient-free Nelder–Mead backend (`argmin` feature) ----------

/// Scatter a free-parameter sub-vector back into the full parameter vector
/// (fixed parameters keep their initial value).
#[cfg(feature = "argmin")]
fn scatter(sub: &[f64], free: &[usize], init_full: &[f64]) -> Vec<f64> {
    let mut full = init_full.to_vec();
    for (k, &i) in free.iter().enumerate() {
        full[i] = sub[k];
    }
    full
}

/// An always-invalid result (e.g. the optimizer failed to start/converge).
#[cfg(feature = "argmin")]
fn invalid(model: &Model) -> FitResult {
    let np = model.params.len();
    FitResult {
        params: model.params.clone(),
        errors: vec![f64::NAN; np],
        minos: None,
        covariance: None,
        chi2: f64::NAN,
        ndf: 0,
        valid: false,
    }
}

/// Minimize with the gradient-free Nelder–Mead simplex from the `argmin` crate.
/// Optimizes the free parameters only (fixed ones are pinned); per-parameter
/// limits are enforced with a soft quadratic penalty. Parameter errors and the
/// covariance come from a numerical Hessian of the (unpenalized) cost at the
/// minimum (`cov = 2·H⁻¹`); MINOS is not provided.
#[cfg(feature = "argmin")]
fn minimize_argmin(model: &Model, cost: &impl Fn(&[f64]) -> f64, ndf: usize) -> FitResult {
    use argmin::core::{CostFunction, Error, Executor};
    use argmin::solver::neldermead::NelderMead;

    let np = model.params.len();
    let free: Vec<usize> = (0..np)
        .filter(|&i| !model.constraints.get(i).map(|c| c.fixed).unwrap_or(false))
        .collect();
    let init_full = model.params.clone();

    // The cost over the free subspace, with limits as a soft quadratic penalty
    // (large but finite, so the simplex is comparable and bounces back in-bounds;
    // for an interior minimum the penalty is inactive there).
    let penalized = |sub: &[f64]| -> f64 {
        let mut pen = 0.0;
        for (k, &i) in free.iter().enumerate() {
            let c = model.constraints.get(i).copied().unwrap_or_default();
            if let Some(lo) = c.lower {
                if sub[k] < lo {
                    pen += (lo - sub[k]) * (lo - sub[k]);
                }
            }
            if let Some(hi) = c.upper {
                if sub[k] > hi {
                    pen += (sub[k] - hi) * (sub[k] - hi);
                }
            }
        }
        cost(&scatter(sub, &free, &init_full)) + 1e30 * pen
    };

    /// Adapter wrapping our `Fn(&[f64]) -> f64` as an argmin problem.
    struct Prob<F>(F);
    impl<F: Fn(&[f64]) -> f64> CostFunction for Prob<F> {
        type Param = Vec<f64>;
        type Output = f64;
        fn cost(&self, p: &Vec<f64>) -> Result<f64, Error> {
            Ok((self.0)(p))
        }
    }

    // Initial simplex: the seed point plus one vertex per free parameter,
    // perturbed by its step (the hint, else 10 % of the value with a floor).
    let seed: Vec<f64> = free.iter().map(|&i| init_full[i]).collect();
    let mut simplex = vec![seed.clone()];
    for (k, &i) in free.iter().enumerate() {
        let c = model.constraints.get(i).copied().unwrap_or_default();
        let step = c.step.unwrap_or_else(|| (seed[k].abs() * 0.1).max(0.01));
        let mut v = seed.clone();
        v[k] += step;
        simplex.push(v);
    }

    let Ok(solver) = NelderMead::new(simplex).with_sd_tolerance(1e-10) else {
        return invalid(model);
    };
    let Ok(res) = Executor::new(Prob(&penalized), solver)
        .configure(|s| s.max_iters(10_000))
        .run()
    else {
        return invalid(model);
    };
    let Some(best) = res.state().best_param.as_ref().cloned() else {
        return invalid(model);
    };

    let params = scatter(&best, &free, &init_full);
    let chi2 = cost(&params); // the clean cost (no penalty) at the minimum
    let (errors, covariance) = hessian_errors(&best, &free, &init_full, cost);

    FitResult {
        params,
        errors,
        minos: None,
        covariance,
        chi2,
        ndf,
        valid: chi2.is_finite(),
    }
}

/// Parabolic errors + free-parameter covariance from a central-difference
/// Hessian of the cost at the minimum: `cov = 2·H⁻¹` (the `Δcost = 1` contour is
/// 1σ for a χ² / −2 ln L cost). Errors are full-length (fixed parameters → 0);
/// the covariance is the `nf×nf` free block (matching the Minuit2 backend).
#[cfg(feature = "argmin")]
fn hessian_errors(
    sub: &[f64],
    free: &[usize],
    init_full: &[f64],
    cost: &impl Fn(&[f64]) -> f64,
) -> (Vec<f64>, Option<Vec<Vec<f64>>>) {
    let np = init_full.len();
    let nf = sub.len();
    let f = |s: &[f64]| cost(&scatter(s, free, init_full));
    let h: Vec<f64> = sub.iter().map(|&v| (v.abs() * 1e-4).max(1e-6)).collect();
    let f0 = f(sub);

    let mut hess = vec![vec![0.0; nf]; nf];
    for i in 0..nf {
        let mut sp = sub.to_vec();
        sp[i] += h[i];
        let mut sm = sub.to_vec();
        sm[i] -= h[i];
        hess[i][i] = (f(&sp) - 2.0 * f0 + f(&sm)) / (h[i] * h[i]);
        for j in (i + 1)..nf {
            let eval = |di: f64, dj: f64| {
                let mut s = sub.to_vec();
                s[i] += di * h[i];
                s[j] += dj * h[j];
                f(&s)
            };
            let v = (eval(1.0, 1.0) - eval(1.0, -1.0) - eval(-1.0, 1.0) + eval(-1.0, -1.0))
                / (4.0 * h[i] * h[j]);
            hess[i][j] = v;
            hess[j][i] = v;
        }
    }

    let covariance = invert(&hess).map(|inv| {
        inv.iter()
            .map(|row| row.iter().map(|&x| 2.0 * x).collect())
            .collect::<Vec<Vec<f64>>>()
    });

    let mut errors = vec![0.0; np]; // fixed parameters report 0, as Minuit2 does
    match &covariance {
        Some(cov) => {
            for (k, &i) in free.iter().enumerate() {
                errors[i] = if cov[k][k] > 0.0 {
                    cov[k][k].sqrt()
                } else {
                    f64::NAN
                };
            }
        }
        None => {
            for &i in free {
                errors[i] = f64::NAN;
            }
        }
    }
    (errors, covariance)
}

/// Invert a small square matrix by Gauss–Jordan elimination with partial
/// pivoting; `None` if singular. Used only for the tiny (few-parameter)
/// covariance, so an O(n³) dense solve is plenty.
#[cfg(feature = "argmin")]
fn invert(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    if n == 0 {
        return Some(Vec::new());
    }
    // Augment with the identity: [m | I].
    let mut a: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            r
        })
        .collect();

    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&a_, &b_| a[a_][col].abs().total_cmp(&a[b_][col].abs()))
            .unwrap();
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        let d = a[col][col];
        for x in a[col].iter_mut() {
            *x /= d;
        }
        // The pivot row is unchanged while we eliminate the others, so clone it
        // once and subtract by iterator (no double-indexing of `a`).
        let pivot_row = a[col].clone();
        for (row, a_row) in a.iter_mut().enumerate() {
            if row != col {
                let factor = a_row[col];
                if factor != 0.0 {
                    for (dst, &src) in a_row.iter_mut().zip(&pivot_row) {
                        *dst -= factor * src;
                    }
                }
            }
        }
    }
    Some(a.into_iter().map(|row| row[n..].to_vec()).collect())
}
