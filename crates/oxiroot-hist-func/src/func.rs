//! The function types: [`Func1D`], [`Func2D`] and [`Func3D`], their evaluation, and the
//! shared [`FuncCore`] (name, formula, parameters, fit metadata).

use oxiroot_formula::{derivative, integrate, Formula};
use oxiroot_hist::GraphFunction;
use oxiroot_io_core::{Error, Result};

/// The data shared by [`Func1D`]/[`Func2D`]/[`Func3D`]: a name and title, the parsed
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
            .map_err(|e| Error::InvalidInput(format!("bad formula {source:?}: {e}")))?;
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

    /// The `Func1D` record ROOT stores for this function over `[xmin, xmax]`.
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

    /// Parse a `Func1D` record's formula back into a function core.
    pub(crate) fn from_record(f: GraphFunction) -> Result<FuncCore> {
        let formula = Formula::parse(&f.formula).map_err(|e| {
            Error::Unsupported(format!("formula {:?} does not parse: {e}", f.formula))
        })?;
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

/// The accessors and parameter mutators common to `Func1D`/`Func2D`/`Func3D`.
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

/// A `Func1D` — a 1-D parametric function `f(x; p)`.
#[derive(Debug, Clone, PartialEq)]
#[doc(alias = "TF1")]
pub struct Func1D {
    pub(crate) core: FuncCore,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
}

impl Func1D {
    /// Build a `Func1D` named `name` from a ROOT `formula` over `[xmin, xmax]`, with
    /// all parameters initialised to zero.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    pub fn new(name: &str, formula: &str, xmin: f64, xmax: f64) -> Result<Func1D> {
        Ok(Func1D {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
        })
    }

    /// Set all parameter values (a builder; `[0], [1], …`).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> Func1D {
        self.set_params(&params);
        self
    }

    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Func1D {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        self.core.formula.eval1(x, &self.core.params)
    }

    /// The definite integral `∫ₐᵇ f(x) dx` (adaptive Gauss–Kronrod), as
    /// `Func1D::Integral`.
    #[must_use]
    pub fn integral(&self, a: f64, b: f64) -> f64 {
        integrate(|x| self.eval(x), a, b)
    }

    /// The derivative `f'(x)` (Richardson central difference), as
    /// `Func1D::Derivative`.
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

    /// This function as the `Func1D` record a graph's `fFunctions` list holds, for
    /// example to attach it to a graph with
    /// [`Graph::with_function`](oxiroot_hist::Graph::with_function). The
    /// record's `formula` is the canonical `[pN]` form.
    ///
    /// ```
    /// use oxiroot_hist::Graph;
    /// use oxiroot_hist_func::Func1D;
    /// let f = Func1D::new("line", "[0]+[1]*x", 0.0, 2.0)
    ///     .unwrap()
    ///     .with_params(vec![1.0, 2.0]);
    /// let g = Graph::new(vec![0.0, 1.0], vec![1.0, 3.0])?.with_function(f.to_graph_function());
    /// assert_eq!(g.functions[0].formula, "[p0]+[p1]*x");
    /// # Ok::<(), oxiroot_io_core::Error>(())
    /// ```
    #[must_use]
    pub fn to_graph_function(&self) -> GraphFunction {
        self.core.record(self.xmin, self.xmax)
    }

    /// Rebuild a `Func1D` from a `Func1D` record, such as a function read from a
    /// graph's [`functions`](oxiroot_hist::Graph::functions), so it can be
    /// evaluated. The range is the record's `xmin`/`xmax`.
    ///
    /// # Errors
    /// Returns an error if the record's formula is not a valid expression.
    pub fn from_graph_function(function: GraphFunction) -> Result<Func1D> {
        let (xmin, xmax) = (function.xmin, function.xmax);
        Ok(Func1D {
            core: FuncCore::from_record(function)?,
            xmin,
            xmax,
        })
    }
}

#[cfg(feature = "fit")]
impl Func1D {
    /// Convert to a fittable [`Model`](oxiroot_fit::Model), seeded with this
    /// function's current parameters — so `data.fit(&tf1.to_model())` fits data
    /// to this function's shape. Requires the `fit` feature.
    ///
    /// The model evaluates the same formula as [`eval`](Func1D::eval). Its
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

/// A `Func2D` — a 2-D parametric function `f(x, y; p)`.
#[derive(Debug, Clone, PartialEq)]
#[doc(alias = "TF2")]
pub struct Func2D {
    pub(crate) core: FuncCore,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
    pub(crate) ymin: f64,
    pub(crate) ymax: f64,
}

impl Func2D {
    /// Build a `Func2D` from a formula in `x` and `y` over `[xmin, xmax] × [ymin, ymax]`.
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
    ) -> Result<Func2D> {
        Ok(Func2D {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
            ymin,
            ymax,
        })
    }

    /// Set all parameter values (a builder).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> Func2D {
        self.set_params(&params);
        self
    }
    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Func2D {
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

/// A `Func3D` — a 3-D parametric function `f(x, y, z; p)`.
#[derive(Debug, Clone, PartialEq)]
#[doc(alias = "TF3")]
pub struct Func3D {
    pub(crate) core: FuncCore,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
    pub(crate) ymin: f64,
    pub(crate) ymax: f64,
    pub(crate) zmin: f64,
    pub(crate) zmax: f64,
}

impl Func3D {
    /// Build a `Func3D` from a formula in `x`, `y`, `z` over the given box.
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
    ) -> Result<Func3D> {
        Ok(Func3D {
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
    pub fn with_params(mut self, params: Vec<f64>) -> Func3D {
        self.set_params(&params);
        self
    }
    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Func3D {
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
