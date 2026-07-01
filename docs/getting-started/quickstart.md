# Quick start

This page writes and reads each of the four main object families — a histogram,
a multi-object file, a `TTree`, and an RNTuple — in a handful of lines. Every
file it produces opens in official ROOT and uproot.

Bring the common types into scope first:

```rust
use oxiroot::prelude::*;
```

## One histogram

A histogram is just data. Construct it with `Hist::reg(nbins, lo, hi).double()`, fill it,
and name it only when you persist it. The `WriteRoot` / `ReadRoot` traits give
every writable object a `write_root` and every readable object a `read_root`.

```rust
let mut h = Hist::reg(50, 0.0, 100.0).double().named("pt").titled("p_{T}");
h.sumw2();                                  // per-bin (weighted) errors
h.fill_weight(42.0, 1.5);
h.write_root("hist.root", Compression::Zstd(5))?;        // any single writable object

let same = TH1::read_root(&RFile::open("hist.root")?, "pt")?;  // any readable object
```

## Several objects, subdirectories, appending

For more than one object, a `TDirectory`, or appending to an existing file, use
the `RootFile` builder — the single entry point for file composition.

```rust
let prof = Hist::reg(5, 0.0, 5.0).profile().named("prof").titled("<pt> per region");
RootFile::create("out.root")
    .add(&h)                              // any &dyn WriteRoot: hist, profile, graph…
    .dir("by_region", |d| d.add(&prof))   // a TDirectory
    .write(Compression::Zstd(5))?;

let g = RFile::open("out.root")?;
let p = TProfile::read_root_in(&g, "by_region", "prof")?;   // read from a subdirectory
```

## A TTree

```rust
let branches = vec![
    Branch::i32("n", vec![1, 2, 3]),
    Branch::f64("pt", vec![10.5, 20.1, 33.7]),
];
Tree::new("Events", branches).write_root("tree.root", Compression::None)?;

let f = RFile::open("tree.root")?;
let t = TTree::open(&f, "Events")?;
let BranchValues::F64(pt) = t.read_branch(&f, "pt")? else { panic!() };
```

## A columnar RNTuple

```rust
let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
Ntuple::new("events", fields).write_root("data.root", Compression::None)?;

let n = RNTuple::open(&RFile::open("data.root")?, "events")?.num_entries();
```

## Run the worked example

The [`analysis` example](https://github.com/mathieuouillon/oxiroot/blob/main/crates/oxiroot/examples/analysis.rs)
is an end-to-end mini analysis — weighted/variable-bin histograms →
scale/merge/normalize → per-region subdirectories → a columnar event dataset →
read-back:

```sh
cargo run -p oxiroot --example analysis
```

## Next

Dive into any area:

- **[Histograms](../guide/histograms.md)** — the full `TH1`/`TH2`/`TH3` family,
  fill semantics, arithmetic, statistics, and derived histograms.
- **[Graphs](../guide/graphs.md)** — `TGraph` and its error variants.
- **[TTree](../guide/ttree.md)** and **[RNTuple](../guide/rntuple.md)** — the two
  event-data formats.
- **[Fitting](../guide/fitting.md)** — fit any 1-D data.
- **[Multithreaded fill](../guide/multithreading.md)**,
  **[Compression](../guide/compression.md)**, and
  **[interop](../guide/interop.md)**.
