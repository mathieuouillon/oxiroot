# Changelog

Notable changes to oxiroot, by release. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): before 1.0, a minor release may
change the API.

## [0.1.0] — Unreleased

The first release: pure-Rust reading and writing of the CERN ROOT file format,
with no C++/libROOT or Python dependency. Files written by oxiroot open in ROOT
and uproot, and CI checks both directions against ROOT C++ and uproot.

### Crates

Depend on `oxiroot` for everything, or on one crate for a part of it.

| Crate | Provides |
|---|---|
| `oxiroot` | Everything below through one dependency, a prelude, and `hadd`-style file merging |
| `oxiroot-io-core` | The `TFile` container: keys, directories, streamer info, 64-bit files, and reads from memory, disk ranges, mmap, HTTP(S) and XRootD (`FileReader`, `FileWriter`); `TList`/`TMap` collections; a generic reader for any class |
| `oxiroot-compress` | ROOT's compression framing, with pure-Rust Zstd, zlib, LZ4 and LZMA |
| `oxiroot-tree` | `TTree` reading and writing, including split `std::vector<MyStruct>`, a streaming `TreeWriter`, `ChainReader`, friend trees and `TEntryList` |
| `oxiroot-rntuple` | RNTuple reading and writing, including nested collections, schema extension and a streaming `NtupleWriter` |
| `oxiroot-hist` | Histograms (`TH1`/`TH2`/`TH3` in every precision), profiles, graphs, `TEfficiency`, `THnSparse`, `TH2Poly`, `THStack`, `TMultiGraph` |
| `oxiroot-hist-func` | `TF1`/`TF2`/`TF3` |
| `oxiroot-formula` | `TFormula` parsing and evaluation |
| `oxiroot-fit` | Fitting histograms, graphs and points with Minuit2, or Nelder–Mead through argmin |
| `oxiroot-stat` | Special functions, distributions, tests and HEP lineshapes, checked against scipy |
| `oxiroot-linalg` | `TVectorD`, `TMatrixD`, `TMatrixDSym` |
| `oxiroot-particle` | PDG particle data and the Monte Carlo numbering scheme |
| `oxiroot-plot` | Plots to SVG, PNG and PDF, with a matplotlib-like API, the mplhep style and TeX math |
| `oxiroot-rex` | The TeX math layout engine `oxiroot-plot` uses: a trimmed copy of ReX |
| `oxiroot-cli` | `oxroot`, a command-line inspector for ROOT files |

### Naming

A type that reads a file ends in `Reader` (`FileReader`, `TreeReader`,
`NtupleReader`, `ChainReader`), and one that writes a file ends in `Writer`
(`FileWriter`, `TreeWriter`, `NtupleWriter`). ROOT's class names (`TFile`,
`TTree`, `RNTuple`, `TChain`) are doc aliases for them.

### Compatibility

- Rust 1.95 or later.
- Histograms and profiles written by older ROOT releases read, back to the
  first class versions ROOT stores through streamer info.
- Written files are checked with ROOT 6.40 and uproot 5.

### Licence

MIT. `oxiroot-particle` also bundles data under BSD-3-Clause
(`MIT AND BSD-3-Clause`), and `oxiroot-rex` carries the original ReX authors'
copyright notices in its `LICENSE-3rdparty`.

[0.1.0]: https://github.com/mathieuouillon/oxiroot/releases/tag/v0.1.0
