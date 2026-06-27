# Fitting

Parametric curve fitting for **any 1-D data** — not just histograms — backed by
a pure-Rust port of [Minuit2](https://crates.io/crates/minuit2), the minimizer
ROOT uses. This page covers the standalone `oxiroot-fit` crate, the `FitData` /
`FitExt` abstraction, building models, choosing a cost and minimizer, and
reading back results.

Everything here is gated behind the **`fit`** feature.

```toml
# Cargo.toml
oxiroot = { version = "*", features = ["fit"] }
```

## The fitting abstraction

A fit needs only, per data point, an independent value `x`, a measured value
`y`, and a Gaussian uncertainty `sigma` on `y`. Anything that can produce those
implements the `FitData` trait, and a blanket `FitExt` impl gives every such
dataset the `.fit(...)` methods for free.

| Item | Role |
| --- | --- |
| `FitData` | Trait with one method, `fn points(&self) -> Vec<Point>` |
| `FitExt` | Blanket trait adding `fit` / `fit_with` / `fit_opts` / `fit_into` to any `FitData` |
| `Point` | One `(x, y, sigma)` triple |
| `Points` | A standalone collection of `Point`s |
| `Model` (alias `TF1`) | A named parametric function `f(x, params)` |
| `FitResult` | Best-fit parameters, errors, covariance, chi-square |

`TH1` and `TGraph` implement `FitData` (in `oxiroot-hist`, under the `fit`
feature), so histograms and graphs fit directly. `Points`, `&[Point]`,
`[Point; N]`, and `Vec<Point>` also implement it, and you can implement it on
your own type to fit anything.

!!! note
    `FitExt` is in scope via `oxiroot::prelude::*`. If you import individual
    items instead, bring it in with `use oxiroot::fit::FitExt;` (or
    `use oxiroot_fit::FitExt;`).

## A first fit

Fitting a Gaussian peak to a histogram. `estimate_from` seeds the
`(constant, mean, sigma)` parameters straight from the data, so no manual
moment loop is needed.

```rust
use oxiroot::prelude::*;

// needs --features fit
let mut peak = TH1::new(60, 80.0, 100.0).named("mass");
peak.sumw2(); // track per-bin errors for the chi-square
for _ in 0..10_000 {
    peak.fill(/* sample */ 91.2);
}

let model = TF1::gaussian("z").estimate_from(&peak);
let fit = peak.fit(&model); // chi-square (ROOT's default)

println!(
    "mean = {:.3} ± {:.3}, sigma = {:.3} ± {:.3}, chi2/ndf = {:.2}",
    fit.params[1], fit.errors[1],
    fit.params[2], fit.errors[2],
    fit.chi2_per_ndf(),
);
```

The same `data.fit(&model)` call works on a `TGraph` or on raw points:

```rust
use oxiroot::prelude::*;

// needs --features fit
// Raw (x, y, sigma) measurements.
let data = Points::new(&[0.0, 1.0, 2.0, 3.0], &[1.0, 3.0, 5.0, 7.0], &[0.1; 4]);
let line = Model::polynomial("line", 1).with_params(vec![0.0, 1.0]);
let fit = data.fit(&line);
assert!(fit.valid);
assert!((fit.params[0] - 1.0).abs() < 1e-6); // intercept ≈ 1
assert!((fit.params[1] - 2.0).abs() < 1e-6); // slope ≈ 2
```

## Building a model

`Model` (aliased `TF1` for ROOT-compatible code) is a named parametric function
`f(x, params)` with named parameters and optional per-parameter constraints.

### Built-in shapes

| Constructor | Function | Parameters |
| --- | --- | --- |
| `Model::gaussian(name)` | `[0]·exp(-½·((x-[1])/[2])²)` (ROOT `"gaus"`) | `constant`, `mean`, `sigma` |
| `Model::exponential(name)` | `exp([0] + [1]·x)` (ROOT `"expo"`) | `constant`, `slope` |
| `Model::polynomial(name, degree)` | `Σ p[k]·x^k` (ROOT `"polN"`) | `p0` … `p<degree>` |

### Custom closures

For anything else, pass a closure `f(x, params)` to `Model::new`. The example
below is a Gaussian signal on a flat background:

```rust
use oxiroot::prelude::*;

// needs --features fit
// params: [norm, mean, sigma, background-per-bin].
let sig_bkg = TF1::new(
    "sig+bkg",
    &["norm", "mean", "sigma", "bkg"],
    vec![100.0, 91.0, 2.0, 10.0], // initial values
    |x, q| q[0] * (-0.5 * ((x - q[1]) / q[2]).powi(2)).exp() + q[3],
);
```

The closure must be `Fn(f64, &[f64]) -> f64 + Send + Sync + 'static`, so a model
can be shared across threads.

### Seeding and constraints

These chainable methods return the `Model`, so they compose:

| Method | Effect |
| --- | --- |
| `with_params(values)` | Replace the initial parameter values |
| `estimate_from(&data)` | Data-driven seed (Gaussian shape only; no-op otherwise) |
| `limit(name, lo, hi)` | Constrain a parameter to `[lo, hi]` |
| `lower_limit(name, lo)` | Constrain a parameter to `>= lo` (e.g. a positive width) |
| `upper_limit(name, hi)` | Constrain a parameter to `<= hi` |
| `fix(name)` | Hold a parameter fixed at its current value |
| `step(name, step)` | Set the initial Minuit2 step (default: 10 % of the value) |

```rust
use oxiroot::prelude::*;

// needs --features fit
let model = Model::gaussian("g")
    .estimate_from(&data)        // seed (constant, mean, sigma) from the data
    .lower_limit("sigma", 0.0);  // keep the width non-negative
```

!!! tip
    `estimate_from` only knows the built-in Gaussian shape — it sets
    `(constant, mean, sigma)` to the peak height and the `y`-weighted mean and
    standard deviation of the data. For other shapes, supply initial values with
    `with_params` (or in `Model::new`). It is especially useful for histograms
    built with `set_bin_content` rather than `fill`, whose stored moment sums are
    zero.

`Model::eval(x)` evaluates the function at `x` with its current parameters —
handy after `fit_into` (below) to draw the fitted curve.

## Costs and the fit methods

`FitExt` gives a dataset four entry points; they layer on top of one another:

| Method | Signature | Behaviour |
| --- | --- | --- |
| `fit` | `fit(&model) -> FitResult` | Chi-square over the full range (ROOT's default) |
| `fit_with` | `fit_with(&model, method) -> FitResult` | Full range with a chosen `FitMethod` |
| `fit_opts` | `fit_opts(&model, &opts) -> FitResult` | Full control via `FitOptions` |
| `fit_into` | `fit_into(&mut model, &opts) -> FitResult` | Like `fit_opts`, but writes the best-fit parameters back into the model |

The cost being minimized is a `FitMethod`:

| `FitMethod` | Cost | Notes |
| --- | --- | --- |
| `Chi2` (default) | Neyman χ², `Σ (y − f)² / σ²` | Uses the *observed* per-point error; empty (`σ ≤ 0`) points are dropped |
| `PearsonChi2` | Pearson χ², `Σ (y − f)² / f` | Uses the *expected* (model) variance; less biased at low counts |
| `Likelihood` | Binned Poisson, `2·Σ [f − y + y·ln(y/f)]` | Maximum-likelihood for binned counts (histograms) |

```rust
use oxiroot::prelude::*;

// needs --features fit
let chi2 = peak.fit(&model);                          // χ², the default
let like = peak.fit_with(&model, FitMethod::Likelihood); // Poisson likelihood
```

!!! warning
    `Likelihood` treats each `y` as a count and assumes a non-negative model; it
    is meaningful for binned histograms, not arbitrary `y`. A model that dips
    below zero is clamped to a tiny positive value (heavily penalised, not
    rejected).

### Options

`FitOptions` carries the cost, an optional fit range, the MINOS toggle, and the
minimizer backend. Construct it with `FitOptions::new()` and chain setters:

| Setter | Effect |
| --- | --- |
| `method(FitMethod)` | The cost to minimize (default `Chi2`) |
| `range(lo, hi)` | Fit only points whose `x` lies in `[lo, hi]` |
| `with_minos(bool)` | Also compute asymmetric MINOS errors |
| `minimizer(Minimizer)` | Choose the optimizer backend |

```rust
use oxiroot::prelude::*;

// needs --features fit
let opts = FitOptions::new()
    .method(FitMethod::Chi2)
    .range(85.0, 97.0)
    .with_minos(true);
let fit = peak.fit_opts(&model, &opts);
```

`fit_into` writes the fitted parameters back into the model so it evaluates the
fitted curve afterwards:

```rust
use oxiroot::prelude::*;

// needs --features fit
let mut model = sig_bkg;
let fit = withbkg.fit_into(&mut model, &FitOptions::new());
// model.eval(x) now draws the *fitted* curve:
let height_at_peak = model.eval(fit.params[1]);
```

## Reading the result

`FitResult` holds the outcome. Fields are in the model's parameter order.

| Field / method | Meaning |
| --- | --- |
| `params: Vec<f64>` | Best-fit parameter values |
| `errors: Vec<f64>` | Parabolic (Minuit2) uncertainties |
| `minos: Option<Vec<(f64, f64)>>` | Asymmetric `(lower, upper)` MINOS errors, when requested |
| `covariance: Option<Vec<Vec<f64>>>` | Covariance of the *free* parameters (row-major) |
| `chi2: f64` | Cost at the minimum |
| `ndf: usize` | Degrees of freedom: fitted points − free parameters |
| `valid: bool` | Whether the minimizer reported a valid minimum |
| `chi2_per_ndf()` | Reduced chi-square `chi2 / ndf` (`NaN` if `ndf == 0`) |
| `p_value()` | Goodness-of-fit p-value (near 1 for a good fit, near 0 for a poor one) |

```rust
use oxiroot::prelude::*;

// needs --features fit
let fit = peak.fit_opts(&model, &FitOptions::new().with_minos(true));
if fit.valid {
    println!("mean   = {:.3} ± {:.3}", fit.params[1], fit.errors[1]);
    if let Some(minos) = &fit.minos {
        let (lo, hi) = minos[1];
        println!("  MINOS: {lo:+.3} / {hi:+.3}");
    }
    println!("chi2/ndf = {:.2}, p = {:.3}", fit.chi2_per_ndf(), fit.p_value());
}
```

!!! note
    The χ²-survival function behind `p_value` lives in the dependency-free
    `oxiroot-stat` leaf crate, shared with the histogram comparison tests.
    A `Chi2` fit drops empty points and a fixed parameter does not count toward
    `ndf`, so reduced-χ² and p-values reflect only what actually entered the fit.

## Minimizer backends

`FitOptions::minimizer(...)` selects the optimizer:

| `Minimizer` | Availability | Errors | MINOS |
| --- | --- | --- | --- |
| `Minuit2` (default) | Always | Parabolic + covariance | Yes (on request) |
| `NelderMead` | `argmin` feature | Numerical Hessian at the minimum | No |

`Minuit2` is the pure-Rust MIGRAD port — ROOT's algorithm — and is always
available. The gradient-free `NelderMead` simplex (from the
[`argmin`](https://crates.io/crates/argmin) crate) is a useful independent
cross-check; it optimizes the free parameters only, enforces limits with a soft
penalty, and derives errors from a central-difference Hessian. The
`Minimizer::NelderMead` variant only exists when the `argmin` feature is on.

```rust
use oxiroot::prelude::*;

// needs --features fit,argmin
let fit = peak.fit_opts(
    &TF1::gaussian("z").estimate_from(&peak),
    &FitOptions::new().minimizer(Minimizer::NelderMead),
);
```

The two backends agree on a Gaussian to displayed precision. `with_minos` is
ignored by `NelderMead`.

## See also

- [Histograms](histograms.md)
- [Graphs](graphs.md)
- [Compression](compression.md)
- [Quickstart](../getting-started/quickstart.md)
- [API reference](../api/oxiroot/index.html)
