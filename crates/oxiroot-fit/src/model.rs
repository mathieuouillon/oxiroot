//! [`Model`] — a named parametric fit function `f(x, params)` with optional
//! per-parameter limits, fixing, and step hints, plus the built-in shapes
//! (Gaussian / exponential / polynomial).

use crate::data::{FitData, Point};

/// A model evaluator: `f(x, params) -> y`. `Send + Sync` so a [`Model`] can cross
/// thread boundaries (e.g. be shared by a parallel fit); an `Arc` so [`Model`] is
/// cheaply `Clone` (clones share the immutable closure).
type ModelFn = std::sync::Arc<dyn Fn(f64, &[f64]) -> f64 + Send + Sync>;

/// Optional fit constraints on one parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Constraint {
    pub(crate) lower: Option<f64>,
    pub(crate) upper: Option<f64>,
    pub(crate) fixed: bool,
    pub(crate) step: Option<f64>,
}

/// A parametric fit function (ROOT's `TF1`): a closure `f(x, params)` plus named
/// parameters with their current/initial values and optional per-parameter
/// limits, fixing, and step hints.
#[derive(Clone)]
pub struct Model {
    /// Function name.
    pub name: String,
    /// Parameter names (used as the Minuit2 parameter labels).
    pub param_names: Vec<String>,
    /// Current parameter values; these seed the fit.
    pub params: Vec<f64>,
    /// Per-parameter constraints (limits / fixed / step), one per parameter.
    pub(crate) constraints: Vec<Constraint>,
    pub(crate) func: ModelFn,
}

impl Model {
    /// Build a function from named parameters and a closure `f(x, params)`.
    pub fn new(
        name: &str,
        param_names: &[&str],
        params: Vec<f64>,
        func: impl Fn(f64, &[f64]) -> f64 + Send + Sync + 'static,
    ) -> Model {
        let constraints = vec![Constraint::default(); params.len()];
        Model {
            name: name.to_string(),
            param_names: param_names.iter().map(|s| s.to_string()).collect(),
            params,
            constraints,
            func: std::sync::Arc::new(func),
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
    pub fn limit(mut self, name: &str, lower: f64, upper: f64) -> Model {
        if let Some(i) = self.index_of(name) {
            let c = self.constraint_mut(i);
            c.lower = Some(lower);
            c.upper = Some(upper);
        }
        self
    }

    /// Constrain parameter `name` to be `>= lower` (e.g. a positive width).
    #[must_use]
    pub fn lower_limit(mut self, name: &str, lower: f64) -> Model {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).lower = Some(lower);
        }
        self
    }

    /// Constrain parameter `name` to be `<= upper`.
    #[must_use]
    pub fn upper_limit(mut self, name: &str, upper: f64) -> Model {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).upper = Some(upper);
        }
        self
    }

    /// Hold parameter `name` fixed at its current value during the fit.
    #[must_use]
    pub fn fix(mut self, name: &str) -> Model {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).fixed = true;
        }
        self
    }

    /// Set the initial Minuit2 step for parameter `name` (default: 10 % of the
    /// value, with a small floor).
    #[must_use]
    pub fn step(mut self, name: &str, step: f64) -> Model {
        if let Some(i) = self.index_of(name) {
            self.constraint_mut(i).step = Some(step);
        }
        self
    }

    /// A Gaussian `[0]·exp(-½·((x-[1])/[2])²)` (ROOT's `"gaus"`); parameters are
    /// `constant`, `mean`, `sigma`. Seed sensible initials with
    /// [`with_params`](Self::with_params) or [`estimate_from`](Self::estimate_from),
    /// and add `.lower_limit("sigma", 0.0)` to keep the width positive.
    pub fn gaussian(name: &str) -> Model {
        Model::new(
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
    pub fn exponential(name: &str) -> Model {
        Model::new(name, &["constant", "slope"], vec![0.0, -1.0], |x, p| {
            (p[0] + p[1] * x).exp()
        })
    }

    /// A degree-`n` polynomial `Σ p[k]·x^k` (ROOT's `"polN"`).
    pub fn polynomial(name: &str, degree: usize) -> Model {
        let names: Vec<String> = (0..=degree).map(|k| format!("p{k}")).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        Model::new(name, &name_refs, vec![0.0; degree + 1], |x, p| {
            p.iter().rev().fold(0.0, |acc, &c| acc * x + c)
        })
    }

    /// Replace the (initial) parameter values.
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> Model {
        self.constraints.resize(params.len(), Constraint::default());
        self.params = params;
        self
    }

    /// Evaluate the function at `x` using its current parameters.
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        (self.func)(x, &self.params)
    }

    /// Whether this is the built-in Gaussian shape (parameters
    /// `constant`/`mean`/`sigma`), the one shape that needs data-driven seeding.
    fn is_gaussian(&self) -> bool {
        self.param_names.len() == 3
            && self.param_names[0] == "constant"
            && self.param_names[1] == "mean"
            && self.param_names[2] == "sigma"
    }

    /// Seed the parameters from a dataset, for a good starting point even when a
    /// histogram's stored moment sums are zero (e.g. one built with
    /// `set_bin_content` rather than `fill`). Works for any [`FitData`] —
    /// histogram, graph, or raw points.
    ///
    /// For the [`gaussian`](Self::gaussian) shape this sets `(constant, mean,
    /// sigma)` to the peak height and the `y`-weighted mean and standard
    /// deviation of the data. For other shapes it is a no-op (seed those with
    /// [`with_params`](Self::with_params)).
    ///
    /// ```
    /// # use oxiroot_fit::{FitExt, Model, Points};
    /// let data = Points::new(&[-1.0, 0.0, 1.0], &[1.0, 4.0, 1.0], &[0.2; 3]);
    /// let model = Model::gaussian("g").estimate_from(&data); // no manual seed
    /// let fit = data.fit(&model);
    /// # assert!(fit.params[2] > 0.0);
    /// ```
    #[must_use]
    pub fn estimate_from(mut self, data: &impl FitData) -> Model {
        if self.is_gaussian() {
            let (constant, mean, sigma) = gaussian_seed(&data.points());
            self.params = vec![constant, mean, sigma];
            self.constraints.resize(3, Constraint::default());
        }
        self
    }
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("name", &self.name)
            .field("param_names", &self.param_names)
            .field("params", &self.params)
            .field("constraints", &self.constraints)
            .finish_non_exhaustive() // the model closure is not printable
    }
}

/// `y`-weighted `(peak, mean, sigma)` of the data points — the Gaussian seed.
/// Robust when the weights carry no spread (falls back to a unit width) or are
/// all zero (mean 0, width 1, the peak height).
fn gaussian_seed(points: &[Point]) -> (f64, f64, f64) {
    let (mut sw, mut swx, mut swx2) = (0.0, 0.0, 0.0);
    let mut peak = f64::NEG_INFINITY;
    for p in points {
        sw += p.y;
        swx += p.y * p.x;
        swx2 += p.y * p.x * p.x;
        peak = peak.max(p.y);
    }
    let peak = if points.is_empty() { 0.0 } else { peak };
    if sw <= 0.0 {
        return (peak, 0.0, 1.0);
    }
    let mean = swx / sw;
    let sigma = (swx2 / sw - mean * mean).max(0.0).sqrt();
    (peak, mean, if sigma > 0.0 { sigma } else { 1.0 })
}
