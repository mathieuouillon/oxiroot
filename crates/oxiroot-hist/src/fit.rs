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

/// A parametric fit function (a minimal `TF1`): a closure `f(x, params)` plus
/// named parameters with their current/initial values.
pub struct TF1 {
    /// Function name.
    pub name: String,
    /// Parameter names (used as the Minuit2 parameter labels).
    pub param_names: Vec<String>,
    /// Current parameter values; these seed the fit.
    pub params: Vec<f64>,
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
        TF1 {
            name: name.to_string(),
            param_names: param_names.iter().map(|s| s.to_string()).collect(),
            params,
            func: Box::new(func),
        }
    }

    /// A Gaussian `[0]·exp(-½·((x-[1])/[2])²)` (ROOT's `"gaus"`); parameters are
    /// `constant`, `mean`, `sigma`. Seed sensible initials with
    /// [`with_params`](Self::with_params) (e.g. `[h.maximum(), h.mean(), h.std_dev()]`).
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
pub enum FitMethod {
    /// Neyman chi-square `Σ (n − f)² / σ²` over non-empty bins (ROOT's default).
    #[default]
    Chi2,
    /// Binned Poisson maximum likelihood (ROOT's `"L"`): minimize the
    /// likelihood-ratio `2·Σ [f − n + n·ln(n/f)]` over every in-range bin.
    Likelihood,
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
    /// Reduced chi-square `chi2 / ndf` (0 when `ndf == 0`).
    #[must_use]
    pub fn chi2_per_ndf(&self) -> f64 {
        if self.ndf == 0 {
            0.0
        } else {
            self.chi2 / self.ndf as f64
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

    /// Fit `model` to this histogram with the chosen [`FitMethod`], seeded from
    /// `model.params`. Chi-square minimizes `Σ (n − f)² / σ²` over non-empty
    /// bins; [`FitMethod::Likelihood`] minimizes the binned Poisson
    /// likelihood-ratio over every in-range bin (handling empty bins, where it
    /// contributes `2·f`). `FitResult::chi2` is the cost at the minimum — a true
    /// chi-square or the likelihood-ratio respectively.
    ///
    /// Requires the `fit` feature.
    #[must_use]
    pub fn fit_with(&self, model: &TF1, method: FitMethod) -> FitResult {
        let n = self.xaxis.nbins.max(0) as usize;
        // Chi-square uses non-empty bins (error > 0); likelihood uses all of them.
        let points: Vec<(f64, f64, f64)> = (1..=n)
            .filter_map(|i| {
                let (y, err) = (self.contents[i], self.bin_error(i));
                match method {
                    FitMethod::Chi2 => (err > 0.0).then(|| (self.bin_center(i), y, err)),
                    FitMethod::Likelihood => Some((self.bin_center(i), y, err)),
                }
            })
            .collect();

        let func = &model.func;
        let cost = |p: &[f64]| -> f64 {
            match method {
                FitMethod::Chi2 => points
                    .iter()
                    .map(|&(x, y, e)| {
                        let d = (y - func(x, p)) / e;
                        d * d
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
        for (name, &init) in model.param_names.iter().zip(&model.params) {
            // Minuit2's initial step; a fraction of the value, with a floor.
            let step = (init.abs() * 0.1).max(0.01);
            migrad = migrad.add(name, init, step);
        }
        let min = migrad.minimize(&cost);
        let state = min.user_state();

        let params: Vec<f64> = model
            .param_names
            .iter()
            .map(|nm| state.value(nm).unwrap_or(f64::NAN))
            .collect();
        let errors: Vec<f64> = model
            .param_names
            .iter()
            .map(|nm| state.error(nm).unwrap_or(f64::NAN))
            .collect();

        FitResult {
            params,
            errors,
            chi2: min.fval(),
            ndf: points.len().saturating_sub(model.params.len()),
            valid: min.is_valid(),
        }
    }
}
