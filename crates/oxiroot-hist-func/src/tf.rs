//! The function types: [`TF1`], [`TF2`] and [`TF3`], their evaluation, and the
//! shared [`FuncCore`] (name, formula, parameters, fit metadata).

use oxiroot_formula::{derivative, integrate, Formula};
use oxiroot_hist::GraphFunction;
use oxiroot_io_core::error::{Error, Result};

/// The data shared by [`TF1`]/[`TF2`]/[`TF3`]: a name and title, the parsed
/// formula, the parameter values, and the fit-result metadata ROOT stores
/// (`fParErrors`/`fParMin`/`fParMax`/`fChisquare`/`fNDF`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FuncCore {
    name: String,
    /// ROOT's `fTitle` — the formula as written by the user (`[0]` form).
    title: String,
    formula: Formula,
    params: Vec<f64>,
    par_errors: Vec<f64>,
    par_min: Vec<f64>,
    par_max: Vec<f64>,
    chi2: f64,
    ndf: i32,
}

impl FuncCore {
    fn build(name: &str, source: &str) -> Result<FuncCore> {
        let formula = Formula::parse(source)
            .map_err(|e| Error::Format(format!("bad formula {source:?}: {e}")))?;
        let npar = formula.npar();
        Ok(FuncCore {
            name: name.to_string(),
            title: source.to_string(),
            params: vec![0.0; npar],
            par_errors: vec![0.0; npar],
            par_min: vec![0.0; npar],
            par_max: vec![0.0; npar],
            chi2: 0.0,
            ndf: 0,
            formula,
        })
    }

    /// The `TF1` record ROOT stores for this function over `[xmin, xmax]`.
    pub(crate) fn record(&self, xmin: f64, xmax: f64) -> GraphFunction {
        GraphFunction {
            name: self.name.clone(),
            title: self.title.clone(),
            formula: self.formula.root_formula().to_owned(),
            params: self.params.clone(),
            par_errors: self.par_errors.clone(),
            par_min: self.par_min.clone(),
            par_max: self.par_max.clone(),
            xmin,
            xmax,
            chi2: self.chi2,
            ndf: self.ndf,
        }
    }

    /// Parse a `TF1` record's formula back into a function core.
    pub(crate) fn from_record(f: GraphFunction) -> Result<FuncCore> {
        let formula = Formula::parse(&f.formula)
            .map_err(|e| Error::Format(format!("bad formula {:?}: {e}", f.formula)))?;
        Ok(FuncCore {
            name: f.name,
            title: f.title,
            formula,
            params: f.params,
            par_errors: f.par_errors,
            par_min: f.par_min,
            par_max: f.par_max,
            chi2: f.chi2,
            ndf: f.ndf,
        })
    }
}

/// The accessors and parameter mutators common to `TF1`/`TF2`/`TF3`.
macro_rules! accessors {
    () => {
        /// The key name (`fName`).
        #[must_use]
        pub fn name(&self) -> &str {
            &self.core.name
        }
        /// The formula expression as written (`fTitle`).
        #[must_use]
        pub fn title(&self) -> &str {
            &self.core.title
        }
        /// The formula in ROOT's canonical `[pN]` form.
        #[must_use]
        pub fn formula(&self) -> &str {
            self.core.formula.root_formula()
        }
        /// The number of parameters.
        #[must_use]
        pub fn npar(&self) -> usize {
            self.core.params.len()
        }
        /// The current parameter values.
        #[must_use]
        pub fn params(&self) -> &[f64] {
            &self.core.params
        }
        /// Parameter `i` (or `0.0` if out of range).
        #[must_use]
        pub fn param(&self, i: usize) -> f64 {
            self.core.params.get(i).copied().unwrap_or(0.0)
        }
        /// Set parameter `i` (ignored if out of range).
        pub fn set_param(&mut self, i: usize, value: f64) {
            if let Some(p) = self.core.params.get_mut(i) {
                *p = value;
            }
        }
        /// Set all parameter values (truncated/zero-padded to the parameter count).
        pub fn set_params(&mut self, params: &[f64]) {
            let n = self.core.params.len();
            self.core.params = params
                .iter()
                .copied()
                .chain(std::iter::repeat(0.0))
                .take(n)
                .collect();
        }
        /// The fit χ² stored on the function (`fChisquare`), `0.0` if unfitted.
        #[must_use]
        pub fn chi2(&self) -> f64 {
            self.core.chi2
        }
        /// The fit degrees of freedom (`fNDF`).
        #[must_use]
        pub fn ndf(&self) -> i32 {
            self.core.ndf
        }
    };
}

/// A `TF1` — a 1-D parametric function `f(x; p)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TF1 {
    pub(crate) core: FuncCore,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
}

impl TF1 {
    /// Build a `TF1` named `name` from a ROOT `formula` over `[xmin, xmax]`, with
    /// all parameters initialised to zero.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    pub fn new(name: &str, formula: &str, xmin: f64, xmax: f64) -> Result<TF1> {
        Ok(TF1 {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
        })
    }

    /// Set all parameter values (a builder; `[0], [1], …`).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF1 {
        self.set_params(&params);
        self
    }

    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TF1 {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        self.core.formula.eval1(x, &self.core.params)
    }

    /// The definite integral `∫ₐᵇ f(x) dx` (adaptive Gauss–Kronrod), as
    /// `TF1::Integral`.
    #[must_use]
    pub fn integral(&self, a: f64, b: f64) -> f64 {
        integrate(|x| self.eval(x), a, b)
    }

    /// The derivative `f'(x)` (Richardson central difference), as
    /// `TF1::Derivative`.
    #[must_use]
    pub fn derivative(&self, x: f64) -> f64 {
        derivative(|x| self.eval(x), x)
    }

    accessors!();

    /// The lower/upper bounds of the function's range (`fXmin`, `fXmax`).
    #[must_use]
    pub fn range(&self) -> (f64, f64) {
        (self.xmin, self.xmax)
    }

    /// This function as the `TF1` record a graph's `fFunctions` list holds, for
    /// example to attach it to a graph with
    /// [`TGraph::with_function`](oxiroot_hist::TGraph::with_function). The
    /// record's `formula` is the canonical `[pN]` form.
    ///
    /// ```
    /// use oxiroot_hist::TGraph;
    /// use oxiroot_hist_func::TF1;
    /// let f = TF1::new("line", "[0]+[1]*x", 0.0, 2.0)
    ///     .unwrap()
    ///     .with_params(vec![1.0, 2.0]);
    /// let g = TGraph::new(vec![0.0, 1.0], vec![1.0, 3.0])?.with_function(f.to_graph_function());
    /// assert_eq!(g.functions[0].formula, "[p0]+[p1]*x");
    /// # Ok::<(), oxiroot_io_core::Error>(())
    /// ```
    #[must_use]
    pub fn to_graph_function(&self) -> GraphFunction {
        self.core.record(self.xmin, self.xmax)
    }

    /// Rebuild a `TF1` from a `TF1` record, such as a function read from a
    /// graph's [`functions`](oxiroot_hist::TGraph::functions), so it can be
    /// evaluated. The range is the record's `xmin`/`xmax`.
    ///
    /// # Errors
    /// Returns an error if the record's formula is not a valid expression.
    pub fn from_graph_function(function: GraphFunction) -> Result<TF1> {
        let (xmin, xmax) = (function.xmin, function.xmax);
        Ok(TF1 {
            core: FuncCore::from_record(function)?,
            xmin,
            xmax,
        })
    }
}

#[cfg(feature = "fit")]
impl TF1 {
    /// Convert to a fittable [`Model`](oxiroot_fit::Model), seeded with this
    /// function's current parameters — so `data.fit(&tf1.to_model())` fits data
    /// to this function's shape. Requires the `fit` feature.
    ///
    /// The model evaluates the same formula as [`eval`](TF1::eval). Its
    /// parameter names come from the title when the title is a formula with the
    /// same number of parameters (so `gaus` keeps its named parameters);
    /// otherwise, as for a read function whose title is free text, from the
    /// canonical `[pN]` formula.
    #[must_use]
    pub fn to_model(&self) -> oxiroot_fit::Model {
        let formula = self.core.formula.clone();
        let names: Vec<String> = match Formula::parse(self.title()) {
            Ok(title) if title.npar() == formula.npar() => title.param_names().to_vec(),
            _ => formula.param_names().to_vec(),
        };
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        oxiroot_fit::Model::new(self.name(), &names, self.params().to_vec(), move |x, p| {
            formula.eval1(x, p)
        })
    }
}

/// A `TF2` — a 2-D parametric function `f(x, y; p)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TF2 {
    pub(crate) core: FuncCore,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
    pub(crate) ymin: f64,
    pub(crate) ymax: f64,
}

impl TF2 {
    /// Build a `TF2` from a formula in `x` and `y` over `[xmin, xmax] × [ymin, ymax]`.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        formula: &str,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
    ) -> Result<TF2> {
        Ok(TF2 {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
            ymin,
            ymax,
        })
    }

    /// Set all parameter values (a builder).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF2 {
        self.set_params(&params);
        self
    }
    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TF2 {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x, y)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64, y: f64) -> f64 {
        self.core.formula.eval(&[x, y], &self.core.params)
    }

    accessors!();

    /// The `x` and `y` ranges (`fXmin`/`fXmax`, `fYmin`/`fYmax`).
    #[must_use]
    pub fn range(&self) -> ((f64, f64), (f64, f64)) {
        ((self.xmin, self.xmax), (self.ymin, self.ymax))
    }
}

/// A `TF3` — a 3-D parametric function `f(x, y, z; p)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TF3 {
    pub(crate) core: FuncCore,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
    pub(crate) ymin: f64,
    pub(crate) ymax: f64,
    pub(crate) zmin: f64,
    pub(crate) zmax: f64,
}

impl TF3 {
    /// Build a `TF3` from a formula in `x`, `y`, `z` over the given box.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        formula: &str,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
        zmin: f64,
        zmax: f64,
    ) -> Result<TF3> {
        Ok(TF3 {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
            ymin,
            ymax,
            zmin,
            zmax,
        })
    }

    /// Set all parameter values (a builder).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF3 {
        self.set_params(&params);
        self
    }
    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TF3 {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x, y, z)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64, y: f64, z: f64) -> f64 {
        self.core.formula.eval(&[x, y, z], &self.core.params)
    }

    accessors!();

    /// The `x`, `y`, and `z` ranges.
    #[must_use]
    pub fn range(&self) -> ((f64, f64), (f64, f64), (f64, f64)) {
        (
            (self.xmin, self.xmax),
            (self.ymin, self.ymax),
            (self.zmin, self.zmax),
        )
    }
}
