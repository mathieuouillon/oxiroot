# Crate layout

oxiroot is a Cargo workspace of small, focused crates. The `oxiroot` facade
re-exports the library crates and provides the `prelude`; depend on it for the
full surface, or pull in a single leaf crate to compile only what you use.
`oxiroot-formula` is the exception: it is the engine behind the functions and
formula fits, and is not re-exported, so depend on it directly to use
`Formula` on its own.

| Crate | Purpose |
|-------|---------|
| [`oxiroot`](../api/oxiroot/index.html) | Facade: `prelude` + re-exports of the crates below (except `oxiroot-formula`) |
| [`oxiroot-io-core`](../api/oxiroot_io_core/index.html) | `TFile` container, buffer primitives, streamer + object-reference engine, the `WriteRoot`/`ReadRoot` object framework, `Error` |
| [`oxiroot-compress`](../api/oxiroot_compress/index.html) | ROOT 9-byte block framing + Zstd/zlib/LZ4/LZMA codecs |
| [`oxiroot-rntuple`](../api/oxiroot_rntuple/index.html) | RNTuple reader/writer (spec v1.0.0.0) |
| [`oxiroot-hist`](../api/oxiroot_hist/index.html) | Histograms, profiles, `TEfficiency`/`THnSparse`/`TH2Poly`, and the `TGraph` family |
| [`oxiroot-hist-func`](../api/oxiroot_hist_func/index.html) | `TF1`/`TF2`/`TF3` parametric functions on `oxiroot-formula`, with ROOT read/write |
| [`oxiroot-formula`](../api/oxiroot_formula/index.html) | Dependency-free `TFormula` expression engine: parse, evaluate, integrate, differentiate |
| [`oxiroot-linalg`](../api/oxiroot_linalg/index.html) | ROOT linear-algebra objects — `TVectorD`/`TMatrixD`/`TMatrixDSym`, with byte-exact ROOT read/write |
| [`oxiroot-tree`](../api/oxiroot_tree/index.html) | Classic `TTree` read/write |
| [`oxiroot-fit`](../api/oxiroot_fit/index.html) | Minuit2 curve fitting for any 1-D data (`FitData`/`Model`); `fit` feature |
| [`oxiroot-stat`](../api/oxiroot_stat/index.html) | Dependency-free special functions (incomplete gamma, Kolmogorov) shared by hist + fit |
| [`oxiroot-particle`](../api/oxiroot_particle/index.html) | PDG particle data: the numbering-scheme decoder and a bundled particle table |
| [`oxiroot-plot`](../api/oxiroot_plot/index.html) | Matplotlib-style SVG/PNG plotting for histograms and graphs; `plot` feature |
| [`oxiroot-rex`](../api/oxiroot_rex/index.html) | Internal: the vendored ReX TeX math layout engine used by `oxiroot-plot` |

## Dependency graph

The leaf crates layer cleanly: `io-core` and `compress` underpin the format
crates (`rntuple`, `hist`, `tree`, `linalg`); the `WriteRoot`/`ReadRoot` object
framework lives in `io-core`, so each format crate registers its own objects.
`stat` and `formula` are dependency-free leaves: `stat` is shared by `hist`
(compatibility tests) and `fit` (goodness-of-fit), and `formula` by `hist-func`
(the `TF1`/`TF2`/`TF3` functions) and `fit` (formula models). Keeping the
functions in `hist-func` means a histogram-only build never compiles the formula
engine. `fit` is optional and only pulled in by the `fit` feature.

```text
oxiroot    -> io-core, compress, rntuple, hist, hist-func, linalg, tree,
              stat, particle, [fit], [plot]
hist-func  -> hist, io-core, formula, [fit]
hist       -> io-core, stat, [fit]
fit        -> formula, stat
plot       -> [hist], [fit], [rex]
tree       -> io-core
rntuple    -> io-core
linalg     -> io-core
io-core    -> compress
formula, stat, particle, compress, rex: no oxiroot dependencies
oxiroot-cli (oxroot) -> oxiroot

[x] = optional, behind a feature. Only oxiroot crates are listed.
```

## Third-party dependencies

All pure Rust — the no-libROOT promise holds end to end:

- [`ruzstd`](https://crates.io/crates/ruzstd) — Zstd
- [`miniz_oxide`](https://crates.io/crates/miniz_oxide) — zlib
- [`lz4_flex`](https://crates.io/crates/lz4_flex) — LZ4
- [`lzma-rust2`](https://crates.io/crates/lzma-rust2) — LZMA / XZ (encode + decode)
- [`xxhash-rust`](https://crates.io/crates/xxhash-rust) — RNTuple XXH3 + LZ4 XXH64
- [`minuit2`](https://crates.io/crates/minuit2) — Minuit2 MIGRAD (with the `fit` feature)
- [`argmin`](https://crates.io/crates/argmin) — Nelder–Mead backend (with the `argmin` feature)
- [`rayon`](https://crates.io/crates/rayon) — data-parallel fill / basket decode (with the `rayon` feature)
- [`memmap2`](https://crates.io/crates/memmap2) — memory-mapped reads (with the `mmap` feature)
- [`bytes`](https://crates.io/crates/bytes) — zero-copy byte ranges behind the reader
- [`ureq`](https://crates.io/crates/ureq) — HTTP(S) client (rustls) for remote range reads (with the `http` feature)
- [ReX](https://github.com/KenyC/ReX) — TeX math layout (with the `plot` feature); not on crates.io, so a trimmed copy is vendored as `oxiroot-rex` (MIT, see its `LICENSE-3rdparty`)
- [`ttf-parser`](https://crates.io/crates/ttf-parser), [`ab_glyph`](https://crates.io/crates/ab_glyph), [`tiny-skia`](https://crates.io/crates/tiny-skia) — font parsing, text outlines and PNG rasterization (with the `plot` feature)

## API reference

The complete, type-level documentation is the rustdoc, browsable here under
**[API reference](../api/oxiroot/index.html)**. Locally you can regenerate and
open it with:

```sh
cargo doc --no-deps --all-features --workspace --open
```
