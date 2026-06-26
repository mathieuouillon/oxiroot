//! Histogram fitting — a lightweight [`TF1`] parametric function fitted to a
//! [`TH1`] by chi-square minimization. Behind the `fit` feature (it pulls in the
//! pure-Rust [Minuit2](https://crates.io/crates/minuit2) port, the same
//! algorithm ROOT uses; [argmin](https://crates.io/crates/argmin) is an
//! alternative backend a downstream could substitute).
//!
//! ```ignore
//! let model = TF1::gaussian("g").with_params(vec![h.maximum(), h.mean(), h.std_dev()]);
//! let fit = h.fit(&model);
//! println!("mean = {} ± {}", fit.params[1], fit.errors[1]);
//! ```

use minuit2::MnMigrad;

use crate::th1::TH1;

/// A model evaluator: `f(x, params) -> y`.
type ModelFn = Box<dyn Fn(f64, &[f64]) -> f64>;

/// Optional fit constraints on one parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Constraint {
    lower: Option<f64>,
    upper: Option<f64>,
    fixed: bool,
    step: Option<f64>,
}

/// A parametric fit function (a minimal `TF1`): a closure `f(x, params)` plus
/// named parameters with their current/initial values and optional per-parameter
/// limits, fixing, and step hints.
pub struct TF1 {
    /// Function name.
    pub name: String,
    /// Parameter names (used as the Minuit2 parameter labels).
    pub param_names: Vec<String>,
    /// Current parameter values; these seed the fit.
    pub params: Vec<f64>,
    /// Per-parameter constraints (limits / fixed / step), one per parameter.
    constraints: Vec<Constraint>,
    func: ModelFn,
}

impl TF1 {
    /// Build a function from named parameters and a closure `f(x, params)`.
    pub fn new(
        name: &str,
        param_names: &[&str],
        params: Vec<f64>,
        func: impl Fn(f64, &[f64]) -> f64 + 'static,
    ) -> TF1 {
        let constraints = vec![Constraint::default(); params.len()];
        TF1 {
            name: name.to_string(),
            param_names: param_names.iter().map(|s| s.to_string()).collect(),
            params,
            constraints,
            func: Box::new(func),
        }
    }

    /// Index of the parameter named `name`.
    fn index_of(&self, name: &str) -> Option<usize> {
        self.param_names.iter().position(|n| n == name)
    }

    /// Mutable access to a parameter's constraints, growing the vector if a
    /// later `with_params` shrank it.
    fn constraint_mut(&mut self, i: usize) -> &mut Constraint {
        if self.constraints.len() < self.params.len() {
            self.constraints
                .resize(self.params.len(), Constraint::default());
        }
        &mut self.constraints[i]
    }

    /// Constrain parameter `name` to `[lower, upper]` during the fit.
    #[must_use]
    pub fn limit(mut self, name: &str, lower: f64, upper: f64) -> TF1 {
        if let Some(i) = self.index_of(name) {
            let c = self.constraint_mut(i);
            c.lower = Some(lower);
            c.upper = Some(upper);
        }
        self
    }

    /// Constrain parameter `name` to be `>= lower` (e.g. a positive width).
    #[must_use]
    pub fn lower_limit(mut self, name: &str, lower: f64) -> TF1 {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).lower = Some(lower);
        }
        self
    }

    /// Constrain parameter `name` to be `<= upper`.
    #[must_use]
    pub fn upper_limit(mut self, name: &str, upper: f64) -> TF1 {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).upper = Some(upper);
        }
        self
    }

    /// Hold parameter `name` fixed at its current value during the fit.
    #[must_use]
    pub fn fix(mut self, name: &str) -> TF1 {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).fixed = true;
        }
        self
    }

    /// Set the initial Minuit2 step for parameter `name` (default: 10 % of the
    /// value, with a small floor).
    #[must_use]
    pub fn step(mut self, name: &str, step: f64) -> TF1 {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).step = Some(step);
        }
        self
    }

    /// A Gaussian `[0]·exp(-½·((x-[1])/[2])²)` (ROOT's `"gaus"`); parameters are
    /// `constant`, `mean`, `sigma`. Seed sensible initials with
    /// [`with_params`](Self::with_params) (e.g. `[h.maximum(), h.mean(), h.std_dev()]`),
    /// and add `.lower_limit("sigma", 0.0)` to keep the width positive.
    pub fn gaussian(name: &str) -> TF1 {
        TF1::new(
            name,
            &["constant", "mean", "sigma"],
            vec![1.0, 0.0, 1.0],
            |x, p| {
                let s = if p[2] == 0.0 { f64::EPSILON } else { p[2] };
                p[0] * (-0.5 * ((x - p[1]) / s).powi(2)).exp()
            },
        )
    }

    /// An exponential `exp([0] + [1]·x)` (ROOT's `"expo"`).
    pub fn exponential(name: &str) -> TF1 {
        TF1::new(name, &["constant", "slope"], vec![0.0, -1.0], |x, p| {
            (p[0] + p[1] * x).exp()
        })
    }

    /// A degree-`n` polynomial `Σ p[k]·x^k` (ROOT's `"polN"`).
    pub fn polynomial(name: &str, degree: usize) -> TF1 {
        let names: Vec<String> = (0..=degree).map(|k| format!("p{k}")).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        TF1::new(name, &name_refs, vec![0.0; degree + 1], |x, p| {
            p.iter().rev().fold(0.0, |acc, &c| acc * x + c)
        })
    }

    /// Replace the (initial) parameter values.
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF1 {
        self.constraints.resize(params.len(), Constraint::default());
        self.params = params;
        self
    }

    /// Evaluate the function at `x` using its current parameters.
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        (self.func)(x, &self.params)
    }
}

/// Which cost a fit minimizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum FitMethod {
    /// Neyman chi-square `Σ (n − f)² / σ²` over non-empty bins, the per-bin error
    /// being the *observed* `√Sumw2`/`√content` (ROOT's default fit).
    #[default]
    Chi2,
    /// Pearson chi-square (ROOT's `"P"`): like [`Chi2`](Self::Chi2) but the
    /// per-bin variance is the *expected* (model) value `Σ (n − f)² / f` over
    /// every in-range bin — less biased than Neyman at low counts.
    PearsonChi2,
    /// Binned Poisson maximum likelihood (ROOT's `"L"`): minimize the
    /// likelihood-ratio `2·Σ [f − n + n·ln(n/f)]` over every in-range bin.
    /// Assumes a non-negative model `f`; a model that dips below zero is clamped
    /// to a tiny positive value (so it is heavily penalised, not rejected).
    Likelihood,
}

/// Options controlling a fit ([`TH1::fit_opts`]). Construct with [`new`](Self::new)
/// and the chainable setters; the defaults are a full-range chi-square fit.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct FitOptions {
    /// The cost to minimize.
    pub method: FitMethod,
    /// Restrict the fit to bins whose center lies in `[lo, hi]`.
    pub range: Option<(f64, f64)>,
}

impl FitOptions {
    /// Default options: a full-range chi-square fit.
    #[must_use]
    pub fn new() -> FitOptions {
        FitOptions::default()
    }
    /// Set the fit cost ([`FitMethod`]).
    #[must_use]
    pub fn method(mut self, method: FitMethod) -> FitOptions {
        self.method = method;
        self
    }
    /// Fit only the bins whose center lies in `[lo, hi]`.
    #[must_use]
    pub fn range(mut self, lo: f64, hi: f64) -> FitOptions {
        self.range = Some((lo, hi));
        self
    }
}

/// The outcome of [`TH1::fit`].
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    /// Best-fit parameter values (in the model's parameter order).
    pub params: Vec<f64>,
    /// Parabolic (Minuit2) uncertainties on each parameter.
    pub errors: Vec<f64>,
    /// Chi-square at the minimum.
    pub chi2: f64,
    /// Degrees of freedom: fitted bins − free parameters.
    pub ndf: usize,
    /// Whether Minuit2 reported a valid minimum.
    pub valid: bool,
}

impl FitResult {
    /// Reduced chi-square `chi2 / ndf`, or `NaN` when `ndf == 0` (an
    /// under-determined fit has no meaningful reduced chi-square).
    #[must_use]
    pub fn chi2_per_ndf(&self) -> f64 {
        if self.ndf == 0 {
            f64::NAN
        } else {
            self.chi2 / self.ndf as f64
        }
    }

    /// Goodness-of-fit p-value: the probability of a chi-square at least this
    /// large for `ndf` degrees of freedom (a good fit is near 1, a poor one near
    /// 0). For a likelihood fit this is the asymptotic value via the
    /// likelihood-ratio (Wilks' theorem). `NaN` for an invalid fit.
    #[must_use]
    pub fn p_value(&self) -> f64 {
        if !self.valid || self.ndf == 0 {
            f64::NAN
        } else {
            crate::compare::chi_square_prob(self.chi2, self.ndf)
        }
    }
}

impl TH1 {
    /// Fit `model` to this histogram by chi-square minimization (ROOT's default
    /// `Fit`); shorthand for [`fit_with`](Self::fit_with) with
    /// [`FitMethod::Chi2`].
    ///
    /// Requires the `fit` feature.
    #[must_use]
    pub fn fit(&self, model: &TF1) -> FitResult {
        self.fit_with(model, FitMethod::Chi2)
    }

    /// Fit `model` over the full range with the chosen [`FitMethod`]; shorthand
    /// for [`fit_opts`](Self::fit_opts).
    ///
    /// Requires the `fit` feature.
    #[must_use]
    pub fn fit_with(&self, model: &TF1, method: FitMethod) -> FitResult {
        self.fit_opts(model, &FitOptions::new().method(method))
    }

    /// Fit `model` to this histogram with full control over the cost and range
    /// ([`FitOptions`]), seeded from `model.params` and its per-parameter
    /// constraints. The bins entering the fit are those whose center is in
    /// `opts.range` (default: all in-range bins); Neyman chi-square additionally
    /// drops empty bins. `FitResult::chi2` is the cost at the minimum — a
    /// chi-square or the likelihood-ratio per the method.
    ///
    /// Requires the `fit` feature.
    #[must_use]
    pub fn fit_opts(&self, model: &TF1, opts: &FitOptions) -> FitResult {
        let np = model.params.len();
        let n = self.xaxis.nbins.max(0) as usize;
        // Neyman chi-square uses only non-empty bins; the others use every bin.
        let drop_empty = matches!(opts.method, FitMethod::Chi2);
        let points: Vec<(f64, f64, f64)> = (1..=n)
            .filter_map(|i| {
                let x = self.bin_center(i);
                if let Some((lo, hi)) = opts.range {
                    if x < lo || x > hi {
                        return None;
                    }
                }
                let y = self.contents.get(i).copied().unwrap_or(0.0);
                let err = self.bin_error(i);
                (!drop_empty || err > 0.0).then_some((x, y, err))
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

        FitResult {
            params,
            errors,
            chi2: min.fval(),
            ndf: points.len() - n_free,
            valid: min.is_valid(),
        }
    }
}
