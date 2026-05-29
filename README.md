# oxroot

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)
[![No libROOT](https://img.shields.io/badge/dependency-no%20libROOT-success.svg)](#)

Pure-Rust IO for the [CERN ROOT](https://root.cern) file format — **read and
write** RNTuple and classic histograms (`TH1`/`TH2`/`TH3`/`TProfile`) in the ROOT
(`TFile`) container, with **no C++/libROOT or Python dependency**. Files written
by oxroot open in official ROOT and uproot, and oxroot reads files they write.

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

Not yet on crates.io — depend on it via git:

```toml
[dependencies]
oxroot = { git = "https://github.com/mathieuouillon/oxroot" }
```

```rust
use oxroot::prelude::*;

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

The [`analysis` example](crates/oxroot/examples/analysis.rs) is an end-to-end
mini analysis — weighted/variable-bin histograms → scale/merge/normalize →
per-region subdirectories → a columnar event dataset → read-back. Run it with:

```sh
cargo run -p oxroot --example analysis
```

## Features

### Histograms (`oxroot::hist`)

- Read `TH1`/`TH2`/`TH3` in every precision (`D`/`F`/`I`/`S`/`C`/`L`) and
  `TProfile`.
- Create and `fill`/`fill_weight` with ROOT's exact `Fill` semantics; uniform or
  variable (`new_variable`) bins; `sumw2()` for weighted per-bin errors
  (`bin_error`).
- Arithmetic with `Sumw2` error propagation: `scale`, `add` (the bin-by-bin
  merge used to combine job outputs), `multiply`, `divide`, `integral`.
- Write `TH1D`/`TH2D`/`TH3D`/`TProfile` — one per file, several per file
  (`write_histograms_file`), or organized into subdirectories
  (`write_histograms_dirs`); append to an existing file with
  `append_histograms_file`. Written files embed a `TStreamerInfo` list, so they
  are self-describing for any ROOT reader.

### RNTuple (`oxroot::ntuple`)

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
| `oxroot` | Facade: `prelude` + re-exports of everything below |
| `oxroot-io-core` | `TFile` container, buffer primitives, streamer engine |
| `oxroot-compress` | ROOT 9-byte block framing + codec backends |
| `oxroot-rntuple` | RNTuple reader/writer (spec v1.0.0.0) |
| `oxroot-hist` | Classic `TH1`/`TH2`/`TH3`/`TProfile` read/write |

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
self-round-trips and byte-level agreement against committed reference files.
Interop is additionally cross-checked against official ROOT and uproot during
development.

## Status & roadmap

Experimental (`0.0.x`) but functional — reading and writing RNTuple and the
classic histogram family both work and interoperate with ROOT and uproot.

**Done:** `TFile` read/write · histogram family read + create/fill/ops/write ·
RNTuple read + write · Zstd compression · self-describing `TStreamerInfo` ·
nested directories · `update` (append) mode · streaming multi-cluster RNTuple ·
ergonomic facade with a `prelude`.

**Not yet:** **`TTree`** read (most existing data is still in TTrees),
float-precision histogram *write* (`TH1F`/…), and `> 2 GiB` (64-bit) files.

> ROOT 7 `RHist` is intentionally out of scope — it has no persistable on-disk
> format (its `Streamer` throws).

## License

Dual-licensed under either the [MIT license](https://opensource.org/licenses/MIT)
or the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your
option.
