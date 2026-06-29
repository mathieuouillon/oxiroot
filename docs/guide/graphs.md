# Graphs

oxiroot reads and writes ROOT's graph family — `TGraph`, `TGraphErrors`, and
`TGraphAsymmErrors` — through a single Rust [`TGraph`](../api/oxiroot/index.html)
type. This page covers constructing the three kinds, naming and titling them,
writing them to a file, and reading them back.

## One type, three classes

A graph is an (x, y) scatter of points. ROOT splits it across three persistable
classes depending on the error bars attached; oxiroot collapses all three into
one `TGraph` whose `errors` field selects the concrete class:

| Rust value | ROOT class | Error bars |
| --- | --- | --- |
| `GraphErrors::None` | `TGraph` | none |
| `GraphErrors::Symmetric { ex, ey }` | `TGraphErrors` | symmetric x/y |
| `GraphErrors::Asymmetric { ex_low, ex_high, ey_low, ey_high }` | `TGraphAsymmErrors` | independent low/high per axis |

The class chosen for a given graph is reported by `TGraph::class_name()`, and it
is detected automatically on read.

```rust
use oxiroot::prelude::*;

let g = TGraph::new(vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0]);
assert_eq!(g.class_name(), "TGraph");
assert_eq!(g.len(), 3);
```

`TGraph` is a plain struct: `name`, `title`, `x`, `y`, and `errors` are all
public fields, so the `errors` enum can be inspected or matched directly after a
read.

## Constructing graphs

Each kind has a dedicated constructor. The coordinate and error vectors are
paired by index; a plain graph truncates `x`/`y` to the shorter length.

```rust
use oxiroot::prelude::*;

// Plain TGraph.
let plain = TGraph::new(
    vec![1.0, 2.0, 3.0],
    vec![10.0, 20.0, 30.0],
);

// TGraphErrors: symmetric x and y error bars.
let sym = TGraph::with_errors(
    vec![1.0, 2.0, 3.0],   // x
    vec![10.0, 20.0, 30.0], // y
    vec![0.1, 0.1, 0.1],    // ex
    vec![1.0, 2.0, 1.5],    // ey
);

// TGraphAsymmErrors: independent low/high errors on each axis.
let asym = TGraph::with_asymm_errors(
    vec![1.0, 2.0, 3.0],     // x
    vec![10.0, 20.0, 30.0],  // y
    vec![0.1, 0.1, 0.1],     // ex_low
    vec![0.2, 0.2, 0.2],     // ex_high
    vec![1.0, 2.0, 1.5],     // ey_low
    vec![1.5, 2.5, 2.0],     // ey_high
);
```

## Naming and titling

Like the histogram types, a graph carries no name until you give it one. A name
is the key it is written under in a ROOT file, so it is required before writing.
Set the name and title with the chainable `named` / `titled` builders:

```rust
use oxiroot::prelude::*;

let g = TGraph::with_errors(
    vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0],
    vec![0.1, 0.1, 0.1], vec![1.0, 2.0, 1.5],
)
.named("resolution")
.titled("Detector resolution vs. p_{T}");
```

!!! warning
    Writing an unnamed graph is a loud error, as is writing two objects with the
    same name into one directory. Give every graph a non-empty, unique key name
    before `write_root` / `RootFile::write`.

## Writing

`TGraph` implements the [`WriteRoot`](../api/oxiroot/index.html) trait, the one
way to write any single writable object. `write_root` writes the graph as the
sole content of a new file; `to_root_bytes` returns just the streamed object
payload (no file framing).

```rust
use oxiroot::prelude::*;

let g = TGraph::with_errors(
    vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0],
    vec![0.1, 0.1, 0.1], vec![1.0, 2.0, 1.5],
)
.named("resolution")
.titled("Detector resolution");

g.write_root("graph.root", Compression::None)?;
```

To put several objects (graphs, histograms, profiles — any mix) in one file, or
to use subdirectories, use the [`RootFile`](../api/oxiroot/index.html) builder.
`add` takes any `&dyn WriteRoot`:

```rust
use oxiroot::prelude::*;

let g = TGraph::new(vec![1.0, 2.0], vec![3.0, 4.0]).named("g");
let h = TH1::new(10, 0.0, 1.0).named("h");

RootFile::create("out.root")
    .add(&g)
    .add(&h)
    .dir("by_region", |d| d.add(&g)) // a TDirectory holding `g`
    .write(Compression::Zstd(5))?;
```

The resulting file reads back in ROOT, uproot, and oxiroot. See
[Reading & writing files](reading-writing.md) for the full `RootFile` and append
workflow.

## Reading

`TGraph` implements the [`ReadRoot`](../api/oxiroot/index.html) trait. The class
(`TGraph` / `TGraphErrors` / `TGraphAsymmErrors`) is detected from the file, and
the matching `errors` variant is filled in:

```rust
use oxiroot::prelude::*;
use oxiroot::RFile;

let f = RFile::open("graph.root")?;
let g = TGraph::read_root(&f, "resolution")?;

println!("{} points, class {}", g.len(), g.class_name());
match &g.errors {
    GraphErrors::Symmetric { ex, ey } => {
        println!("symmetric errors: ex={ex:?}, ey={ey:?}");
    }
    GraphErrors::Asymmetric { ey_low, ey_high, .. } => {
        println!("asymmetric y errors: low={ey_low:?}, high={ey_high:?}");
    }
    GraphErrors::None => println!("no error bars"),
}
```

Read a graph from inside a subdirectory with `read_root_in`:

```rust
use oxiroot::prelude::*;
use oxiroot::RFile;

let f = RFile::open("out.root")?;
let g = TGraph::read_root_in(&f, "by_region", "g")?;
```

## Fitting a graph

Under the `fit` feature, `TGraph` implements `FitData`, so the same `Model` and
`fit` method that work on a `TH1` also work on a graph. Each point becomes
`(x, y, σ)`, where σ is the y-error bar: the symmetric `ey`, the mean of the
asymmetric `(ey_low, ey_high)`, or `1.0` (an unweighted least-squares fit) when
the graph carries no errors.

```rust
use oxiroot::prelude::*;

let graph = TGraph::with_errors(
    vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0],
    vec![0.1, 0.1, 0.1], vec![1.0, 2.0, 1.5],
)
.named("g");

let line = graph.fit(&Model::polynomial("line", 1).with_params(vec![0.0, 0.0]));
```

!!! note
    Only the y-error is used as the fit weight; x-errors are not propagated into
    σ. See [Fitting](fitting.md) for models, options, and the choice of minimizer
    backend.

## 3-D graphs: `TGraph2D`

A `TGraph2D` is a set of `(x, y, z)` points — a 3-D scatter or the input to a
surface. It is a separate Rust type with the same read/write traits:

```rust
use oxiroot::prelude::*;
use oxiroot::RFile;

let g = TGraph2D::new(
    vec![1.0, 2.0, 3.0],       // x
    vec![10.0, 20.0, 30.0],    // y
    vec![100.0, 200.0, 300.0], // z
)
.named("surface")
.titled("a 3-D scatter");

g.write_root("g2d.root", Compression::Zstd(5))?;

let back = TGraph2D::read_root(&RFile::open("g2d.root")?, "surface")?;
assert_eq!(back.len(), 3);
```

The point data (`x`/`y`/`z`) round-trips with ROOT and uproot. ROOT's display
parameters (the binning of the lazily-built `fHistogram`, the Delaunay iteration
count) are written at ROOT's defaults, and the `fHistogram` frame itself is
transient in ROOT and not persisted.

## See also

- [Reading & writing files](reading-writing.md) — the `RootFile` builder, append mode, and `ReadRoot`.
- [Histograms](histograms.md) — the `TH1`/`TH2`/`TH3` family that shares the same write/read traits.
- [Fitting](fitting.md) — fitting graphs, histograms, and raw points with a shared `Model`.
- [Quickstart](../getting-started/quickstart.md) — a first end-to-end example.
