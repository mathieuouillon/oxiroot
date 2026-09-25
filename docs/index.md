# oxiroot

**Pure-Rust IO for the [CERN ROOT](https://root.cern) file format** — read and
write RNTuple, `TTree`, the classic histogram family, and graphs in the ROOT
(`TFile`) container, with **no C++/libROOT or Python dependency**. Files written
by oxiroot open in official ROOT and uproot, and oxiroot reads the files they
write.

> The name is *ROOT + oxide* — Rust is oxidized iron.

```rust
use oxiroot::prelude::*;

// Fill a histogram and save it — readable by ROOT and uproot.
let mut h = Hist::reg(50, 0.0, 100.0).double().named("pt").titled("p_{T}");
h.fill_weight(42.0, 1.5);
h.write_root("hist.root", Compression::Zstd(5))?;

// Read it back.
let h = Hist1D::read_root(&FileReader::open("hist.root")?, "pt")?;
```

## Why oxiroot

- **Pure Rust.** The on-disk format is reimplemented from the official specs —
  no libROOT, no Python. It builds and runs anywhere Rust does, depending only on
  a handful of small pure-Rust crates (compression codecs and a hasher).
- **Two-way interop.** Every reader and writer is validated against both official
  ROOT (C++) and uproot, in both directions.
- **Robust by construction.** Readers never panic on malformed input
  (fuzz-tested); writers refuse to silently corrupt a file past the 2 GiB 32-bit
  limit; same-name key collisions are a loud error, not a silent shadow.
- **Idiomatic.** A histogram is just data you name when you persist it; one trait
  per direction (`WriteRoot` / `ReadRoot`); a `FileWriter` for composing
  files; fitting that works on *any* 1-D data.

## What's covered

| Area | Read | Write |
|------|:----:|:-----:|
| Histograms `Hist1D`/`Hist2D`/`Hist3D` (every precision) | ✅ | ✅ |
| Profiles `Profile1D`/`Profile2D`/`Profile3D` | ✅ | ✅ |
| `Efficiency`, `SparseHist`, `PolyHist` | ✅ | ✅ |
| Graphs `Graph`/`TGraphErrors`/`TGraphAsymmErrors` | ✅ | ✅ |
| `TTree` (scalars, arrays, strings, `std::vector`, split structs) | ✅ | ✅ |
| RNTuple (scalars, strings, vectors, nested, records) | ✅ | ✅ |
| Compression Zstd / zlib / LZ4 / LZMA | ✅ | ✅ |
| Curve fitting (Minuit2, optional argmin) | — | — |
| Plotting → SVG / PNG (matplotlib look, LaTeX) | — | ✅ |

## Where to go next

- **[Installation](getting-started/installation.md)** — add oxiroot to your
  project (facade or à-la-carte crates; optional features).
- **[Quick start](getting-started/quickstart.md)** — write and read a histogram,
  a `TTree`, and an RNTuple in a few lines.
- **Guide** — a page per area:
  [Histograms](guide/histograms.md), [Graphs](guide/graphs.md),
  [TTree](guide/ttree.md), [RNTuple](guide/rntuple.md),
  [Fitting](guide/fitting.md), [Statistics](guide/statistics.md),
  [Plotting](guide/plotting.md),
  [Multithreaded fill](guide/multithreading.md),
  [Compression](guide/compression.md), and
  [ROOT / uproot interop](guide/interop.md).
- **[API reference (rustdoc)](api/oxiroot/index.html)** — the full type-level
  documentation generated from the source.
