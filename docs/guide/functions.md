# Functions (`TF1`/`TF2`/`TF3`)

A **function** is a formula, its parameter values, and a range. oxiroot's `TF1`
(1-D), `TF2` (2-D), and `TF3` (3-D) are ROOT's parametric functions: they
evaluate in pure Rust — `eval`, `integral`, `derivative` — and read and write as
ordinary ROOT `TF1`/`TF2`/`TF3` keys (each embeds a `TFormula`), so ROOT C++ and
uproot read what oxiroot writes and vice versa.

The functions live in the `oxiroot-hist-func` crate and are re-exported as
`oxiroot::hist::{TF1, TF2, TF3}` and in the prelude. The expression engine
behind them is the dependency-free
[`oxiroot-formula`](../reference/crates.md) crate; the same engine powers
[`Model::from_formula`](fitting.md) so any formula is also fittable.

## Building and evaluating

Construct a `TF1` from a name, a formula, and an `[xmin, xmax]` range, then set
the parameters:

```rust
use oxiroot::prelude::*;

let f = TF1::new("f", "[0]*sin([1]*x) + [2]", 0.0, 6.283)?
    .with_params(vec![2.0, 1.5, 0.5]);

f.eval(1.0);                       // 2*sin(1.5) + 0.5  = 2.494990
f.integral(0.0, 3.14159);         // ∫ over [0, π]     = 2.904130
f.derivative(1.0);                // 3*cos(1.5)        = 0.212212
# Ok::<(), oxiroot::Error>(())
```

`integral` is an adaptive Gauss–Kronrod quadrature and `derivative` a
Richardson-extrapolated central difference, matching `TF1::Integral` /
`TF1::Derivative` to ~10 significant figures.

`TF2`/`TF3` add coordinates — the formula gains `y` (and `z`):

```rust
use oxiroot::prelude::*;
let f2 = TF2::new("f2", "[0]*sin(x) + [1]*y*y", -3.0, 3.0, -2.0, 2.0)?
    .with_params(vec![1.5, 0.7]);
f2.eval(1.0, 1.0);                            // 1.5*sin(1) + 0.7

let f3 = TF3::new("f3", "[0]*x + y*z", 0.0, 2.0, 0.0, 2.0, 0.0, 2.0)?
    .with_params(vec![2.0]);
f3.eval(1.0, 1.0, 1.0);                       // 3.0
# Ok::<(), oxiroot::Error>(())
```

## Formula syntax

Parameters are `[0]`, `[1]`, … and the variables are `x`, `y`, `z`. The engine
supports:

- **Operators** `+ - * / ^` (and `**` for power), with the usual precedence;
  comparisons (`< > <= >= == !=`) and `&& || ` yield `1.0`/`0.0`, and there is a
  `cond ? a : b` ternary.
- **Functions** `sin cos tan asin acos atan atan2 sinh cosh tanh exp log log10
  log2 sqrt abs pow min max floor ceil …`, each with or without a `TMath::`
  prefix (`TMath::Sqrt(x)` = `sqrt(x)`). ROOT's `log` is the natural log.
- **Constants** `pi` (and `TMath::Pi()`).
- **ROOT shortcuts** that expand to parameterised templates:
  - `gaus` → `[0]*exp(-0.5*((x-[1])/[2])^2)`
  - `expo` → `exp([0]+[1]*x)`
  - `pol0`…`polN` → `[0]+[1]*x+…+[N]*x^N`

  A shortcut takes an optional parameter offset — `gaus(0)+pol1(3)` uses `[0..2]`
  for the Gaussian and `[3..4]` for the line.

```rust
use oxiroot::prelude::*;
let g = TF1::new("g", "gaus", -5.0, 5.0)?.with_params(vec![2.0, 0.0, 1.0]);
assert_eq!(g.eval(0.0), 2.0);
# Ok::<(), oxiroot::Error>(())
```

## Precision and parameters

`npar()` is the number of parameters (inferred from the highest `[i]`), `ndim()`
the dimensionality. Read parameters with `params()`/`param(i)`, set them with
`set_param(i, v)`, `set_params(&[…])`, or the `with_params(vec)` builder.
`title()` is the formula as written (ROOT's `fTitle`); `formula()` is ROOT's
canonical `[pN]` form.

## Read and write

Functions read and write through the same `WriteRoot`/`ReadRoot` traits as every
other object — [Reading & writing](reading-writing.md):

```rust
use oxiroot::prelude::*;
let f = TF1::new("resp", "[0]*exp(-[1]*x)", 0.0, 5.0)?.with_params(vec![10.0, 0.5]);
f.write_root("func.root", Compression::None)?;             // a standalone TF1 key
let back = TF1::read_root(&RFile::open("func.root")?, "resp")?;
assert!((back.eval(2.0) - f.eval(2.0)).abs() < 1e-12);
# Ok::<(), oxiroot::Error>(())
```

They also go into a multi-object file or a subdirectory via the `RootFile`
builder, and a graph's attached fitted functions (`fFunctions`) use the same
`TF1`/`TFormula` serialization — see [Graphs](graphs.md).

A `TF1` and a graph's `GraphFunction` are the same ROOT record, so they convert
both ways: `to_graph_function()` attaches a function to a graph, and
`TF1::from_graph_function` turns an attached function back into something you
can evaluate:

```rust
use oxiroot::prelude::*;
let f = TF1::new("line", "[0]+[1]*x", 0.0, 2.0)?.with_params(vec![1.0, 2.0]);
let g = TGraph::new(vec![0.0, 1.0, 2.0], vec![1.1, 2.9, 5.2])
    .named("g")
    .with_function(f.to_graph_function());
let attached = TF1::from_graph_function(g.functions[0].clone())?;
assert_eq!(attached.eval(1.5), f.eval(1.5));
# Ok::<(), oxiroot::Error>(())
```

ROOT C++ and uproot both read oxiroot's `TF1`, `TF2`, and `TF3` and re-evaluate
them: oxiroot embeds the `TF1`/`TF2`/`TF3`/`TFormula` `TStreamerInfo` (including
the `std::vector<double>`/`std::map` members) so uproot builds a model for each.

## Fitting to a function's shape

To fit data to a formula, build a [`Model`](fitting.md) from it — either directly
with `Model::from_formula`, or from an existing `TF1` with `to_model()` (the
`fit` feature of `oxiroot` or `oxiroot-hist-func`) — and fit any histogram,
graph, or point set:

```rust
use oxiroot::prelude::*;
let mut h = Hist::reg(60, 80.0, 100.0).double().named("mass");
// … fill h …
let model = Model::from_formula("peak", "gaus(0) + pol1(3)")?
    .with_params(vec![100.0, 91.0, 2.5, 10.0, 0.0]);
let result = h.fit(&model);
println!("chi2/ndf = {:.2}", result.chi2_per_ndf());
# Ok::<(), Box<dyn std::error::Error>>(())
```
