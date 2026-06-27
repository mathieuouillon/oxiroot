# Installation

oxiroot is pure Rust. It needs a **Rust 1.95+** toolchain and nothing else — no
libROOT, no Python, no system libraries. All compression codecs are pure-Rust
crates pulled in automatically.

## Add the dependency

oxiroot is experimental (`0.0.x`) and not yet published to crates.io, so depend
on it via git. Pull in everything through the **facade**, or just the one crate
you need — the histogram, tree, and RNTuple crates are independent, so a
histogram-only project never compiles the others.

```toml
[dependencies]
# Everything — histograms, graphs, TTree, RNTuple — through the facade:
oxiroot = { git = "https://github.com/mathieuouillon/oxiroot" }

# …or depend on just one crate from the same repo:
oxiroot-hist    = { git = "https://github.com/mathieuouillon/oxiroot" }  # histograms + graphs
oxiroot-tree    = { git = "https://github.com/mathieuouillon/oxiroot" }  # TTree
oxiroot-rntuple = { git = "https://github.com/mathieuouillon/oxiroot" }  # RNTuple
```

Then bring the common types into scope with the prelude:

```rust
use oxiroot::prelude::*;
```

## Optional features

Everything off by default builds as pure, safe Rust with a minimal dependency
set. Turn on extras à la carte:

| Feature | Effect |
|---------|--------|
| `mmap` | Memory-mapped read path (`RFile::open_mmap`) for large files; adds `memmap2`. |
| `rayon` | Data-parallel histogram fill (`hist::fill_par`); adds `rayon`. |
| `fit` | Curve fitting (`oxiroot::fit`, `TH1::fit`) via the pure-Rust Minuit2 port; adds `minuit2`. |
| `argmin` | Adds the gradient-free Nelder–Mead minimizer backend (`Minimizer::NelderMead`); implies `fit`, adds `argmin`. |

```toml
[dependencies]
oxiroot = { git = "https://github.com/mathieuouillon/oxiroot", features = ["fit", "rayon"] }
```

!!! tip "Fitting on the command line"
    Examples that fit need the feature flag, e.g.
    `cargo run -p oxiroot --example fit --features fit`.

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
