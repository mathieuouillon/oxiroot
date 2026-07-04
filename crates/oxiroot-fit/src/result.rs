//! Fit configuration ([`FitMethod`], [`FitOptions`]) and outcome ([`FitResult`]).

/// Which cost a fit minimizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum FitMethod {
    /// Neyman chi-square `Σ (y − f)² / σ²` over points with an error, the
    /// per-point error being the *observed* `σ` (ROOT's default fit). For a
    /// histogram this is `√Sumw2`/`√content`; empty (`σ ≤ 0`) points are dropped.
    #[default]
    Chi2,
    /// Pearson chi-square (ROOT's `"P"`): like [`Chi2`](Self::Chi2) but the
    /// per-point variance is the *expected* (model) value `Σ (y − f)² / f` over
    /// every point — less biased than Neyman at low counts.
    PearsonChi2,
    /// Binned Poisson maximum likelihood (ROOT's `"L"`): minimize the
    /// likelihood-ratio `2·Σ [f − y + y·ln(y/f)]` over every point (which it
    /// treats as a count). Assumes a non-negative model `f`; a model that dips
    /// below zero is clamped to a tiny positive value (heavily penalised, not
    /// rejected). Meaningful for binned counts (histograms), not arbitrary `y`.
    Likelihood,
}

/// Which optimizer minimizes the cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Minimizer {
    /// The pure-Rust [Minuit2](https://crates.io/crates/minuit2) port (MIGRAD) —
    /// ROOT's algorithm. Gives parabolic errors, the covariance matrix, and (on
    /// request) asymmetric MINOS errors. The default, always available.
    #[default]
    Minuit2,
    /// Gradient-free Nelder–Mead simplex via the
    /// [`argmin`](https://crates.io/crates/argmin) crate (requires the `argmin`
    /// feature). Parameter errors come from a numerical Hessian at the minimum;
    /// MINOS is not available. A useful independent cross-check of Minuit2.
    #[cfg(feature = "argmin")]
    NelderMead,
}

/// A robust loss `ρ` applied to each squared residual of a least-squares fit
/// ([`Chi2`](FitMethod::Chi2) / [`PearsonChi2`](FitMethod::PearsonChi2)) — the
/// `loss` of [`scipy.optimize.least_squares`](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.least_squares.html).
/// [`Linear`](Self::Linear) (the default) is an ordinary least-squares fit; the
/// others grow sub-quadratically, so a far-out point (an outlier) pulls on the
/// fit far less than its squared residual would suggest. Set the residual scale
/// at which a point starts to count as an outlier with
/// [`FitOptions::f_scale`](FitOptions::f_scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Loss {
    /// Ordinary least squares, `ρ(z) = z` (no down-weighting). The default.
    #[default]
    Linear,
    /// Smooth-L1, `ρ(z) = 2(√(1 + z) − 1)` — a gentle, everywhere-smooth robustness.
    SoftL1,
    /// Huber, `ρ(z) = z` for `z ≤ 1`, else `2√z − 1` — a quadratic core with
    /// linear tails.
    Huber,
    /// Cauchy / Lorentzian, `ρ(z) = ln(1 + z)` — strong outlier suppression.
    Cauchy,
    /// Arctan, `ρ(z) = arctan(z)` — the most aggressive (bounded influence).
    Arctan,
}

/// Options controlling a fit ([`FitExt::fit_opts`](crate::FitExt::fit_opts)).
/// Construct with [`new`](Self::new) and the chainable setters; the defaults are
/// a full-range chi-square fit minimized with [`Minimizer::Minuit2`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct FitOptions {
    /// The cost to minimize.
    pub method: FitMethod,
    /// Restrict the fit to points whose `x` lies in `[lo, hi]`.
    pub range: Option<(f64, f64)>,
    /// Also compute asymmetric [MINOS](https://root.cern/doc/master/classTMinuit.html)
    /// errors for each free parameter (a likelihood scan — more accurate than the
    /// parabolic errors near a non-quadratic minimum, but extra work).
    /// Ignored by the [`NelderMead`](Minimizer::NelderMead) backend.
    pub minos: bool,
    /// Which optimizer backend to use.
    pub minimizer: Minimizer,
    /// Robust [`Loss`] applied to each squared residual (least-squares costs
    /// only; ignored by [`Likelihood`](FitMethod::Likelihood)).
    pub loss: Loss,
    /// The residual scale `C` for a robust [`loss`](Self::loss): a point begins to
    /// be treated as an outlier once its residual exceeds ~`C`. Default `1.0`;
    /// irrelevant to [`Loss::Linear`].
    pub f_scale: f64,
}

impl Default for FitOptions {
    fn default() -> FitOptions {
        FitOptions {
            method: FitMethod::default(),
            range: None,
            minos: false,
            minimizer: Minimizer::default(),
            loss: Loss::default(),
            f_scale: 1.0,
        }
    }
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
    /// Fit only the points whose `x` lies in `[lo, hi]`.
    #[must_use]
    pub fn range(mut self, lo: f64, hi: f64) -> FitOptions {
        self.range = Some((lo, hi));
        self
    }
    /// Also compute asymmetric MINOS errors (see [`minos`](Self::minos) field).
    #[must_use]
    pub fn with_minos(mut self, on: bool) -> FitOptions {
        self.minos = on;
        self
    }
    /// Choose the optimizer backend ([`Minimizer`]).
    #[must_use]
    pub fn minimizer(mut self, minimizer: Minimizer) -> FitOptions {
        self.minimizer = minimizer;
        self
    }
    /// Use a robust [`Loss`] to down-weight outliers (least-squares costs only).
    #[must_use]
    pub fn loss(mut self, loss: Loss) -> FitOptions {
        self.loss = loss;
        self
    }
    /// Set the robust-loss residual scale (see [`f_scale`](Self::f_scale)).
    #[must_use]
    pub fn f_scale(mut self, f_scale: f64) -> FitOptions {
        self.f_scale = f_scale;
        self
    }
}

/// The outcome of a fit ([`FitExt::fit`](crate::FitExt::fit)).
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    /// Best-fit parameter values (in the model's parameter order).
    pub params: Vec<f64>,
    /// Parabolic (Minuit2) uncertainties on each parameter.
    pub errors: Vec<f64>,
    /// Asymmetric MINOS errors `(lower, upper)` per parameter (`lower ≤ 0 ≤ upper`),
    /// when requested via [`FitOptions::minos`]; `None` otherwise. A fixed
    /// parameter reports `(0.0, 0.0)`.
    pub minos: Option<Vec<(f64, f64)>>,
    /// Covariance matrix of the *free* (non-fixed) parameters, in their parameter
    /// order (row-major), when Minuit2 produced one; `None` otherwise. With no
    /// fixed parameters this is the full parameter covariance.
    pub covariance: Option<Vec<Vec<f64>>>,
    /// Chi-square at the minimum.
    pub chi2: f64,
    /// Degrees of freedom: fitted points − free parameters.
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
            oxiroot_stat::chi_square_prob(self.chi2, self.ndf)
        }
    }
}
