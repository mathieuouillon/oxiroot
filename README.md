# oxiroot

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)
[![No libROOT](https://img.shields.io/badge/dependency-no%20libROOT-success.svg)](#)

Pure-Rust IO for the [CERN ROOT](https://root.cern) file format — **read and
write** RNTuple and classic histograms (`TH1`/`TH2`/`TH3`/`TProfile`) in the ROOT
(`TFile`) container, with **no C++/libROOT or Python dependency**. Files written
by oxiroot open in official ROOT and uproot, and oxiroot reads files they write.

> The name is *ROOT + oxide* — Rust is oxidized iron.

## Highlights

- 🦀 **Pure Rust** — the on-disk format reimplemented from the official specs.
  No libROOT, no Python; builds and runs anywhere Rust does.
- 🔄 **Two-way interop** — every reader and writer is validated against both
  official ROOT and uproot, in both directions.
- 📊 **Histograms** — read & fill `TH1`/`TH2`/`TH3`/`TProfile`; weighted errors
  (`Sumw2`), variable bins, arithmetic (scale / merge / divide), subdirectories.
- 🧱 **RNTuple** — read & write ROOT's columnar format (scalars, strings,
  vectors), Zstd-compressed, and multi-cluster via a streaming writer.
- 🗜 **Compression** — Zstd read **and** write; zlib/LZ4 decode for real-world
  files.

## Quick start

Not yet on crates.io — depend on it via git. Pull in everything through the
facade, or just the part you need: the histogram and RNTuple crates are
independent, so a histogram-only project never compiles the RNTuple code (and
vice versa).

```toml
[dependencies]
# Everything — histograms + RNTuple — through the facade:
oxiroot = { git = "https://github.com/mathieuouillon/oxiroot" }

# …or depend on just one crate from the same repo:
oxiroot-hist    = { git = "https://github.com/mathieuouillon/oxiroot" }  # histograms only
oxiroot-rntuple = { git = "https://github.com/mathieuouillon/oxiroot" }  # RNTuple only
```

```rust
use oxiroot::prelude::*;

// Fill and save a histogram (weighted errors + variable bins supported).
let mut h = TH1::new("pt", "p_{T}", 50, 0.0, 100.0);
h.sumw2();
h.fill_weight(42.0, 1.5);
write_th1d_file("out.root".as_ref(), &h, Compression::Zstd(5))?;

// Write a columnar dataset, then read it back.
let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
write_rntuple_file("data.root".as_ref(), "events", &fields, Compression::None)?;
let f = RFile::open("data.root")?;
let n = RNTuple::open(&f, "events")?.num_entries();
```

The [`analysis` example](crates/oxiroot/examples/analysis.rs) is an end-to-end
mini analysis — weighted/variable-bin histograms → scale/merge/normalize →
per-region subdirectories → a columnar event dataset → read-back. Run it with:

```sh
cargo run -p oxiroot --example analysis
```

## Features

### Histograms (`oxiroot::hist`)

- Read `TH1`/`TH2`/`TH3` in every precision (`D`/`F`/`I`/`S`/`C`/`L`) and
  `TProfile`/`TProfile2D`/`TProfile3D`.
- Create and `fill`/`fill_weight` with ROOT's exact `Fill` semantics; uniform or
  variable (`new_variable`) bins; `sumw2()` for weighted per-bin errors
  (`bin_error`).
- Arithmetic with `Sumw2` error propagation: `scale`, `add` (the bin-by-bin
  merge used to combine job outputs), `multiply`, `divide`, `integral`.
- Statistics & shape accessors: `mean`/`std_dev`/`rms`, `maximum`/`minimum`/
  `maximum_bin`, `find_bin`, `bin_center`/`bin_width`/`bin_low_edge`,
  `effective_entries`, `reset`; derived histograms `rebin`/`rebin2d`/`rebin3d`,
  `cumulative`, projections (`TH2`→`TH1` via `projection_x`/`_y`; `TH3`→`TH1`/`TH2`
  via `projection_x`/`_y`/`_z` and `projection_xy`/`_xz`/`_yz`), and
  `profile_x`/`profile_y` (`TH2`→`TProfile`) — all carrying the statistical moment
  sums so the results' `mean`/`std_dev` stay correct.
- **Multithreaded fill** — `ThreadedHist`, the pure-Rust analog of ROOT's
  `TThreadedObject<TH1>`: each worker fills a private clone (lock-free), then
  `merge()` combines them exactly (contents + `Sumw2` + every moment sum). Works
  with `std::thread::scope` and needs no dependency; the optional `rayon` feature
  adds a one-call `fill_par(&template, &data, |h, x| h.fill(*x))`.
- Write `TH1`/`TH2`/`TH3` in every precision — `D`/`F` (double/float) and
  `C`/`S`/`I`/`L` (char/short/int/long64) — plus `TProfile`/`TProfile2D`/`TProfile3D`; one per file,
  several per file (`write_histograms_file`), or organized into subdirectories
  (`write_histograms_dirs`); append to an existing file with
  `append_histograms_file`. Written files embed a `TStreamerInfo` list, so they
  are self-describing for any ROOT reader.

### RNTuple (`oxiroot::ntuple`)

- Read the binary spec v1.0.0.0: anchor → envelopes → schema → clusters → pages,
  with split/zigzag/delta encodings and Zstd-compressed pages.
- Typed field API (`read_field`) for scalars, `std::string`, and
  `std::vector<T>`, across multiple clusters.
- Write `bool`, 32/64-bit signed & unsigned ints, `f32`/`f64`, `std::string`,
  and `std::vector<T>` (bool/int/float) — optionally Zstd-compressed.
- `RNTupleWriter` streams one cluster per `write_batch`, so a large dataset is
  never fully held in memory.

## Workspace layout

| Crate | Purpose |
|-------|---------|
| `oxiroot` | Facade: `prelude` + re-exports of everything below |
| `oxiroot-io-core` | `TFile` container, buffer primitives, streamer engine |
| `oxiroot-compress` | ROOT 9-byte block framing + codec backends |
| `oxiroot-rntuple` | RNTuple reader/writer (spec v1.0.0.0) |
| `oxiroot-hist` | Classic `TH1`/`TH2`/`TH3`/`TProfile` read/write |

Dependencies are pure Rust: [`xxhash-rust`](https://crates.io/crates/xxhash-rust)
(RNTuple XXH3), [`ruzstd`](https://crates.io/crates/ruzstd) (Zstd encode/decode),
and [`miniz_oxide`](https://crates.io/crates/miniz_oxide) (zlib decode).

## Build & test

```sh
cargo build  --workspace
cargo test   --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt    --all --check
```

The committed tests are pure Rust (no ROOT or Python needed): they check
self-round-trips and byte-level agreement against committed reference files. CI
additionally round-trips histograms and RNTuple both ways against official ROOT
(C++) and uproot — Rust writes files they read, and reads files they write.

### Full local interop check

For a comprehensive cross-language check on your own machine (not CI), run:

```sh
bash scripts/interop_local.sh            # full run; prints a PASS/FAIL matrix
bash scripts/interop_local.sh --no-fixtures   # skip the fixture-regen step (fast)
bash scripts/interop_local.sh --big      # also exercise the >2 GiB (64-bit) read path
```

It exercises oxiroot's full read+write surface against **both** ROOT C++ and
uproot, in both directions: the lean canonical round-trip, a manifest-driven
**matrix** (~38 cases — every histogram precision×dimension, `TProfile`, Sumw2,
variable bins, multi-object/subdirs/append, every RNTuple scalar+vector type +
multi-cluster, every `TTree` branch kind + scalar width + split
`std::vector<Struct>`), `cargo test --workspace`, and a drift check that
regenerates the committed fixtures from your *local* ROOT/uproot and re-tests.
Missing tools (no ROOT, or no uproot venv) degrade to `SKIP`, never `FAIL`, so a
machine with neither still gets a meaningful green from `cargo test` alone.
Needs a Python venv at `.venv` with `uproot numpy awkward`, and `root-config`
(+`rootcling`) on `PATH` for the ROOT-C++ side.

## Status & roadmap

Experimental (`0.0.x`) but functional — reading and writing RNTuple and the
classic histogram family both work and interoperate with ROOT and uproot.

**Done:** `TFile` read/write · histogram family read + create/fill/ops/write ·
RNTuple read + write · **`TTree`** — read and write scalar, fixed/variable
array, string, unsplit `std::vector<T>`, and **split
`std::vector<MyStruct>`** (`TBranchElement`) branches as per-member
sub-branches, with a generated `TStreamerInfo` for the element class
(ROOT-C++- and uproot-verified, both directions) · Zstd
compression · self-describing `TStreamerInfo` · nested directories · `update`
(append) mode · streaming multi-cluster RNTuple · 64-bit (`> 2 GiB`) files —
read, plus RNTuple write (the one-shot writer auto-switches past 2 GiB; the
streaming writer via `create_large`) · ergonomic facade with a `prelude`.

> ROOT 7 `RHist` is intentionally out of scope — it has no persistable on-disk
> format (its `Streamer` throws).

## License

Licensed under the [MIT License](LICENSE).
