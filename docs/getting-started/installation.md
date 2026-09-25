# Installation

oxiroot is pure Rust. It needs a **Rust 1.95+** toolchain and nothing else — no
libROOT, no Python, no system libraries. All compression codecs are pure-Rust
crates pulled in automatically.

## Add the dependency

oxiroot is on crates.io. It is pre-1.0 (`0.1`), so the API may still change
between minor versions. Pull in everything through the **facade**, or just the
one crate you need — the histogram, tree, and RNTuple crates are independent, so
a histogram-only project never compiles the others.

```toml
[dependencies]
# Everything — histograms, graphs, TTree, RNTuple — through the facade:
oxiroot = "0.3"

# …or depend on just one crate:
oxiroot-hist      = "0.3"  # histograms + graphs
oxiroot-hist-func = "0.3"  # Func1D/Func2D/Func3D
oxiroot-tree      = "0.3"  # TTree
oxiroot-rntuple   = "0.3"  # RNTuple
```

Then bring the common types into scope with the prelude:

```rust
use oxiroot::prelude::*;
```

## Optional features

The `oxiroot` facade is **batteries-included**: fitting, plotting and the mmap
read path are on by default, so there is usually nothing extra to enable. The
parallel helpers (`rayon`) and remote reads (`http`, `xrootd`) are opt-in. Opt
out with `default-features = false` (then re-enable à la carte) when you want
just the lean, pure-Rust format core.

| Feature | Default | Effect |
|---------|:---:|--------|
| `mmap` | ✅ | Memory-mapped read path (`FileReader::open_mmap`) for large files; adds `memmap2`. |
| `rayon` | — | Adds the data-parallel histogram fill (`hist::fill_par`) and the parallel TTree reads (`TreeReader::read_branch_par` and friends); adds `rayon`. Opt-in, so nothing spawns threads unless you ask. |
| `fit` | ✅ | Curve fitting (`oxiroot::fit`, `Hist1D::fit`) via the pure-Rust Minuit2 port; adds `minuit2`. |
| `argmin` | ✅ | Gradient-free Nelder–Mead minimizer backend (`Minimizer::NelderMead`); implies `fit`, adds `argmin`. |
| `plot` | ✅ | Plotting to SVG/PNG/PDF (`oxiroot::plot`); adds `tiny-skia`/`ab_glyph` and the ReX TeX engine. |

```toml
[dependencies]
# Batteries-included — fitting, plotting, mmap, and argmin all on:
oxiroot = "0.3"

# …or the lean format core only (drops the extra dependencies):
# oxiroot = { version = "0.3", default-features = false }
```

!!! tip "Leaner builds"
    The extras pull real dependencies (the Minuit2 port, the ReX TeX engine, …).
    If you only read and write ROOT files, `default-features = false` keeps the
    build minimal; re-enable individual features as you need them.

## Build & test

```sh
cargo build  --workspace
cargo test   --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt    --all --check
```

The committed tests are pure Rust — they check self-round-trips, byte-level
agreement against committed reference files, and malformed-input hardening, with
no ROOT or Python required.

For a full cross-language interop check against official ROOT (C++) **and**
uproot in both directions, see the [interop guide](../guide/interop.md).

## Next

→ **[Quick start](quickstart.md)** — write and read a histogram, a `TTree`, and
an RNTuple in a few lines.
