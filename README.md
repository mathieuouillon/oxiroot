# oxiroot

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)
[![No libROOT](https://img.shields.io/badge/dependency-no%20libROOT-success.svg)](#)

Pure-Rust IO for the [CERN ROOT](https://root.cern) file format — **read and
write** RNTuple, `TTree`, the classic histogram family, and graphs in the ROOT
(`TFile`) container, with **no C++/libROOT or Python dependency**. Files written
by oxiroot open in official ROOT and uproot, and oxiroot reads files they write.

> The name is *ROOT + oxide* — Rust is oxidized iron.

## Highlights

- 🦀 **Pure Rust** — the on-disk format reimplemented from the official specs.
  No libROOT, no Python; builds and runs anywhere Rust does, and the default
  build pulls in only three small pure-Rust crates.
- 🔄 **Two-way interop** — every reader and writer is validated against both
  official ROOT (C++) and uproot, in both directions.
- 📊 **Histograms & profiles** — `TH1`/`TH2`/`TH3` (every precision),
  `TProfile`/`TProfile2D`/`TProfile3D`, `TEfficiency`, N-dimensional `THnSparse`,
  and polygon-binned `TH2Poly` — all read **and** write.
- 📈 **Graphs** — `TGraph`, `TGraphErrors`, `TGraphAsymmErrors`, read and write.
- 🌳 **`TTree`** — read and write scalar, fixed/variable-length array, string,
  `std::vector<T>`, and **split `std::vector<MyStruct>`** branches.
- 🧱 **RNTuple** — read and write ROOT's columnar format (scalars, strings,
  vectors), Zstd-compressed, multi-cluster via a streaming writer.
- 🧵 **Multithreaded fill** — `ThreadedHist`, the pure-std analog of ROOT's
  `TThreadedObject<TH1>`; optional one-call `rayon` parallel fill.
- 🛡 **Robust by construction** — readers never panic on malformed input
  (fuzz-tested), and writers refuse to silently corrupt a file past the 2 GiB
  32-bit limit.

## Quick start

Not yet on crates.io — depend on it via git. Pull in everything through the
facade, or just the part you need: the histogram, tree, and RNTuple crates are
independent, so a histogram-only project never compiles the others.

```toml
[dependencies]
# Everything — histograms, graphs, TTree, RNTuple — through the facade:
oxiroot = { git = "https://github.com/mathieuouillon/oxiroot" }

# …or depend on just one crate from the same repo:
oxiroot-hist    = { git = "https://github.com/mathieuouillon/oxiroot" }  # histograms + graphs
oxiroot-tree    = { git = "https://github.com/mathieuouillon/oxiroot" }  # TTree
oxiroot-rntuple = { git = "https://github.com/mathieuouillon/oxiroot" }  # RNTuple
```

```rust
use oxiroot::prelude::*;

// Fill and save a histogram (weighted errors + variable bins supported).
let mut h = TH1::new("pt", "p_{T}", 50, 0.0, 100.0);
h.sumw2();
h.fill_weight(42.0, 1.5);
write_th1d_file("hist.root", &h, Compression::Zstd(5))?;

// Write a TTree, then read a branch back.
let branches = vec![
    Branch::i32("n", vec![1, 2, 3]),
    Branch::f64("pt", vec![10.5, 20.1, 33.7]),
];
write_tree_file("tree.root", "Events", &branches, Compression::None)?;
let f = RFile::open("tree.root")?;
let t = TTree::open(&f, "Events")?;
let BranchValues::F64(pt) = t.read_branch(&f, "pt")? else { panic!() };

// Write a columnar RNTuple, then read it back.
let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
write_rntuple_file("data.root", "events", &fields, Compression::None)?;
let n = RNTuple::open(&RFile::open("data.root")?, "events")?.num_entries();
```

The [`analysis` example](crates/oxiroot/examples/analysis.rs) is an end-to-end
mini analysis — weighted/variable-bin histograms → scale/merge/normalize →
per-region subdirectories → a columnar event dataset → read-back:

```sh
cargo run -p oxiroot --example analysis
```

## Features

### Histograms & profiles (`oxiroot::hist`)

- **Read & write** `TH1`/`TH2`/`TH3` in every precision (`D`/`F`/`I`/`S`/`C`/`L`),
  `TProfile`/`TProfile2D`/`TProfile3D`, `TEfficiency`, N-dimensional `THnSparse`,
  and polygon-binned `TH2Poly` (arbitrary-shape bins, with a builder API).
- Create and `fill`/`fill_weight` with ROOT's exact `Fill` semantics; uniform or
  variable (`new_variable`) bins; `sumw2()` for weighted per-bin errors
  (`bin_error`).
- Arithmetic with `Sumw2` error propagation: `scale`, `add` (the bin-by-bin
  merge used to combine job outputs), `multiply`, `divide`, `integral`.
- Statistics & shape accessors: `mean`/`std_dev`/`rms`, `maximum`/`minimum`/
  `maximum_bin`, `find_bin`, `bin_center`/`bin_width`/`bin_low_edge`,
  `effective_entries`, `reset`; derived histograms `rebin`/`rebin2d`/`rebin3d`,
  `cumulative`, projections (`TH2`→`TH1`; `TH3`→`TH1`/`TH2`), and
  `profile_x`/`profile_y` — all carrying the statistical moment sums so the
  results' `mean`/`std_dev` stay correct.
- **Multithreaded fill** — `ThreadedHist`, the pure-Rust analog of ROOT's
  `TThreadedObject<TH1>`: each worker fills a private clone (lock-free), then
  `merge()` combines them exactly (contents + `Sumw2` + every moment sum). Works
  with `std::thread::scope` and needs no dependency; the optional `rayon` feature
  adds a one-call `fill_par(&template, &data, |h, x| h.fill(*x))`.
- Write one histogram per file, several per file (`write_histograms_file`), or
  organized into subdirectories (`write_histograms_dirs`); append to an existing
  file with `append_histograms_file`. Written files embed a `TStreamerInfo` list,
  so they are self-describing for any ROOT reader.

### Graphs (`oxiroot::hist`)

A single `TGraph` type covers all three ROOT classes, selected by its `errors`
field: plain (`TGraph`), symmetric (`TGraphErrors`), or asymmetric
(`TGraphAsymmErrors`). `read_tgraph` detects the class on read.

```rust
use oxiroot::prelude::*;
let g = TGraph::with_errors(
    "res", "resolution",
    vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0], // x, y
    vec![0.1, 0.1, 0.1], vec![1.0, 2.0, 1.5],    // ex, ey
);
write_tgraph_file("graph.root", &g, Compression::None)?;
```

### TTree (`oxiroot::tree`)

- **Read & write** scalar, fixed-size array (`x[N]`), variable-length / jagged
  (`x[n]`), string, and `std::vector<T>` branches.
- **Split `std::vector<MyStruct>`** branches written as per-member sub-branches
  (`TBranchElement`), with a generated `TStreamerInfo` for the element class —
  ROOT-C++- and uproot-verified, both directions.
- `Branch::{i32, f64, bools, strings, …}` for scalars, `Branch::vec_*` for
  fixed arrays, `Branch::jagged_*` for variable arrays, `Branch::vector_*` for
  `std::vector<T>`, and `Branch::split_vector` for split structs.

### RNTuple (`oxiroot::ntuple`)

- Read the binary spec v1.0.0.0: anchor → envelopes → schema → clusters → pages,
  with split/zigzag/delta encodings and Zstd-compressed pages.
- Typed field API (`read_field`) for scalars, `std::string`, and
  `std::vector<T>`, across multiple clusters.
- Write `bool`, 32/64-bit signed & unsigned ints, `f32`/`f64`, `std::string`,
  and `std::vector<T>` (bool/int/float) — optionally Zstd-compressed.
- `RNTupleWriter` streams one cluster per `write_batch`, so a large dataset is
  never fully held in memory.

### Compression

- **Read:** Zstd and zlib decode (the codecs real ROOT files use in practice).
  Uncompressed objects pass through directly. (LZ4/LZMA decode are not yet
  wired up — such a block reports an unavailable-codec error rather than
  corrupting silently.)
- **Write:** Zstd, at any level via `Compression::Zstd(level)`, or
  `Compression::None`.

### Robustness & large files

- Parsers are hardened against malformed input: every read path is bounds- and
  overflow-checked, capacity reservations are bounded by the remaining buffer,
  and histogram array lengths are validated against the axis geometry — so a
  crafted or truncated file yields an `Err`, never a panic. Byte-flip and
  truncation fuzz tests cover the container, RNTuple, `TTree`, and every
  histogram/graph reader.
- 64-bit (`> 2 GiB`) files are supported on read; the RNTuple writer
  auto-switches to the big format, and the `TFile`/`TTree` writers reject an
  over-2 GiB write instead of silently truncating their 32-bit seek pointers.
- `Error` is `#[non_exhaustive]` and preserves the underlying `io::ErrorKind`.

## Workspace layout

| Crate | Purpose |
|-------|---------|
| `oxiroot` | Facade: `prelude` + re-exports of everything below |
| `oxiroot-io-core` | `TFile` container, buffer primitives, streamer + object-reference engine, `Error` |
| `oxiroot-compress` | ROOT 9-byte block framing + Zstd/zlib codecs |
| `oxiroot-rntuple` | RNTuple reader/writer (spec v1.0.0.0) |
| `oxiroot-hist` | Histograms, profiles, `TEfficiency`/`THnSparse`/`TH2Poly`, and the `TGraph` family |
| `oxiroot-tree` | Classic `TTree` read/write |

Dependencies are pure Rust: [`xxhash-rust`](https://crates.io/crates/xxhash-rust)
(RNTuple XXH3), [`ruzstd`](https://crates.io/crates/ruzstd) (Zstd encode/decode),
and [`miniz_oxide`](https://crates.io/crates/miniz_oxide) (zlib decode).

### Optional features

| Feature | Effect |
|---------|--------|
| `mmap` | Memory-mapped read path (`RFile::open_mmap`) for large files; adds `memmap2`. |
| `rayon` | Data-parallel histogram fill (`hist::fill_par`); adds `rayon`. |

Both are off by default, so the default build stays pure safe Rust.

## Build & test

```sh
cargo build  --workspace
cargo test   --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt    --all --check
```

The committed tests are pure Rust (no ROOT or Python needed): they check
self-round-trips, byte-level agreement against committed reference files, and
malformed-input hardening. CI additionally round-trips every type both ways
against official ROOT (C++) and uproot — Rust writes files they read, and reads
files they write.

### Full local interop check

For a comprehensive cross-language check on your own machine (not CI), run:

```sh
bash scripts/interop_local.sh                 # full run; prints a PASS/FAIL matrix
bash scripts/interop_local.sh --no-fixtures   # skip the fixture-regen step (fast)
bash scripts/interop_local.sh --big           # also exercise the >2 GiB (64-bit) read path
```

It exercises oxiroot's full read+write surface against **both** ROOT C++ and
uproot, in both directions: the lean canonical round-trip, a manifest-driven
**matrix** (every histogram precision×dimension, `TProfile`, Sumw2, variable
bins, multi-object/subdirs/append, every RNTuple scalar+vector type +
multi-cluster, every `TTree` branch kind + scalar width + split
`std::vector<Struct>`), `cargo test --workspace`, and a drift check that
regenerates the committed fixtures from your *local* ROOT/uproot and re-tests.
Missing tools (no ROOT, or no uproot venv) degrade to `SKIP`, never `FAIL`, so a
machine with neither still gets a meaningful green from `cargo test` alone.
Needs a Python venv at `.venv` with `uproot numpy awkward`, and `root-config`
(+`rootcling`) on `PATH` for the ROOT-C++ side.

## Roadmap

Experimental (`0.0.x`). On the list — each item targets the same bar as what
already ships: byte-level round-trips verified against both ROOT and uproot.

- **Compression** — LZ4 and LZMA *decode* (today such a block reports an
  unavailable-codec error); non-Zstd *encode* (zlib / LZ4) for files matching
  older ROOT defaults.
- **Histogram analysis** — fitting (`TF1` + a minimizer); `Chi2Test` /
  `KolmogorovTest`; `GetQuantiles` / `Interpolate`; labelled / alphanumeric axes
  (`fLabels` is currently skipped on read).
- **Graphs** — `TGraph2D` and `TGraphMultiErrors`; persisting a graph's fitted
  functions (`fFunctions`) and display frame (`fHistogram`), written empty today.
- **RNTuple** — richer collection fields: `std::vector<std::string>`, nested
  `std::vector<std::vector<T>>`, and vectors of records; the remaining column
  encodings on the read path.
- **`TTree`** — object / nested branches beyond split `std::vector<MyStruct>`
  (nested structs, `std::vector<std::string>`, arrays of objects).
- **Append mode** — `update` into files that contain subdirectories or an
  RNTuple (currently rejected).

Out of scope: ROOT 7 `RHist` (no persistable on-disk format — its `Streamer`
throws) and graphics objects (`TCanvas` and friends).

## License

Licensed under the [MIT License](LICENSE).
