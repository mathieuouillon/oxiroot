# Crate layout

oxiroot is a Cargo workspace of small, focused crates. The `oxiroot` facade
re-exports everything and provides the `prelude`; depend on it for the full
surface, or pull in a single leaf crate to compile only what you use.

| Crate | Purpose |
|-------|---------|
| [`oxiroot`](../api/oxiroot/index.html) | Facade: `prelude` + re-exports of everything below |
| [`oxiroot-io-core`](../api/oxiroot_io_core/index.html) | `TFile` container, buffer primitives, streamer + object-reference engine, `Error` |
| [`oxiroot-compress`](../api/oxiroot_compress/index.html) | ROOT 9-byte block framing + Zstd/zlib/LZ4/LZMA codecs |
| [`oxiroot-rntuple`](../api/oxiroot_rntuple/index.html) | RNTuple reader/writer (spec v1.0.0.0) |
| [`oxiroot-hist`](../api/oxiroot_hist/index.html) | Histograms, profiles, `TEfficiency`/`THnSparse`/`TH2Poly`, and the `TGraph` family |
| [`oxiroot-tree`](../api/oxiroot_tree/index.html) | Classic `TTree` read/write |
| [`oxiroot-fit`](../api/oxiroot_fit/index.html) | Minuit2 curve fitting for any 1-D data (`FitData`/`Model`); `fit` feature |
| [`oxiroot-stat`](../api/oxiroot_stat/index.html) | Dependency-free special functions (incomplete gamma, Kolmogorov) shared by hist + fit |
| [`oxiroot-plot`](../api/oxiroot_plot/index.html) | Matplotlib-style SVG/PNG plotting for histograms and graphs; `plot` feature |

## Dependency graph

The leaf crates layer cleanly: `io-core` and `compress` underpin the format
crates (`rntuple`, `hist`, `tree`); `stat` is a dependency-free leaf shared by
`hist` (compatibility tests) and `fit` (goodness-of-fit); `fit` is optional and
only pulled in by the `fit` feature.

```text
                       oxiroot  (facade + prelude)
                          │
   ┌──────────────┬───────┼────────┬──────────────┐
 hist            tree   rntuple   fit*           (re-exports)
   │  │           │        │        │
   │  └── stat ───┼────────┼────────┘
   │              │        │
   └── io-core ── ┴── compress
                          ▲
                  (io-core also uses compress)

   * fit is gated behind the `fit` feature; argmin adds a second backend.
```

## Third-party dependencies

All pure Rust — the no-libROOT promise holds end to end:

- [`ruzstd`](https://crates.io/crates/ruzstd) — Zstd
- [`miniz_oxide`](https://crates.io/crates/miniz_oxide) — zlib
- [`lz4_flex`](https://crates.io/crates/lz4_flex) — LZ4
- [`lzma-rs`](https://crates.io/crates/lzma-rs) — LZMA / XZ (decode only)
- [`xxhash-rust`](https://crates.io/crates/xxhash-rust) — RNTuple XXH3 + LZ4 XXH64
- [`minuit2`](https://crates.io/crates/minuit2) — Minuit2 MIGRAD (with the `fit` feature)
- [`argmin`](https://crates.io/crates/argmin) — Nelder–Mead backend (with the `argmin` feature)
- [`rayon`](https://crates.io/crates/rayon) — data-parallel fill / basket decode (with the `rayon` feature)
- [`memmap2`](https://crates.io/crates/memmap2) — memory-mapped reads (with the `mmap` feature)

## API reference

The complete, type-level documentation is the rustdoc, browsable here under
**[API reference](../api/oxiroot/index.html)**. Locally you can regenerate and
open it with:

```sh
cargo doc --no-deps --all-features --workspace --open
```
