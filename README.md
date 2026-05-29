# root-rs

Pure-Rust IO for the [CERN ROOT](https://root.cern) file format — read and write
**RNTuple** and **classic histograms** (TH1/TH2/TH3/TProfile) in the ROOT (TFile)
container, with **no C++/libROOT dependency**.

> Status: **reads and writes** RNTuple and classic histograms in the ROOT
> (TFile) container — validated against official ROOT and uproot. Hardening and
> broader type coverage are ongoing.

## Scope

- **Pure Rust** — reimplements the on-disk format from the official specs; ROOT is
  used only as an optional dev/test oracle.
- **Two-way interop with official ROOT** is a hard requirement: files we write open
  in ROOT, and we read files ROOT writes.
- **RNTuple**: the columnar event-data format, binary spec v1.0.0.0 (ROOT v6.34).
- **Classic histograms** (TH1D/F, TH2*, TH3*, TProfile) via `TStreamerInfo`.
  (ROOT 7 `RHist` is intentionally out of scope — it has no persistable on-disk
  format; its `Streamer` throws.)
- **Compression**: Zstd (+ uncompressed) for writing; Zstd/zlib/LZ4 decode for
  reading real-world files.

## Workspace layout

| Crate | Purpose |
|-------|---------|
| `root-io-core` | TFile container + buffer primitives + streamer engine |
| `root-compress` | ROOT 9-byte block framing + codec backends |
| `root-rntuple` | RNTuple reader/writer (spec v1.0.0.0) |
| `root-hist` | Classic TH1/TH2/TH3/TProfile read/write |
| `root-rs` | Facade crate: high-level `RFile` API + re-exports |

## Build & test

```sh
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

No external crates are required for the current foundation; codec and checksum
dependencies are added as later milestones land.

## Roadmap

- **M0** — Workspace skeleton; compression block framing; `RBuffer`/`WBuffer`
  primitives. ✅ _done_
- **M1** — TFile container read + list keys (`RFile::open`, `keys()`). ✅ _done_
- **M2** — Decompress (Zstd/zlib) + read the classic histogram family — all
  precisions `TH1/2/3{D,F,I,S,C,L}` plus `TProfile`, via the streamer engine;
  parse the `TStreamerInfo` list. ✅ _done_
- **M3** — RNTuple read. ✅ _End-to-end, validated against uproot: anchor →
  envelopes → schema → cluster groups → page list → page decode, including
  split/zigzag/delta encodings and Zstd-compressed pages, plus a typed field
  API (`read_field`) for scalars, `std::string`, and `std::vector<T>`._
- **M4** — TFile write + a TH1D ROOT can read. ✅ _root-rs writes a complete
  TFile (header, TDirectory, object keys, key list) holding a byte-identical
  `TH1D` object; both uproot and official ROOT read it back with correct
  bins/stats. (Streamer-info emission + write compression are follow-ups.)_
- **M5** — RNTuple write. ✅ _root-rs writes a scalar RNTuple
  (`Int32`/`Real32`/`Real64`) — header/page-list/footer envelopes + XXH3 anchor
  — that both official ROOT (`RNTupleReader`) and uproot read with correct
  values._
- **M6** — Round-trip / interop hardening. _In progress: histogram + RNTuple
  Zstd compression ✅; self-describing `TStreamerInfo` emission ✅ (ROOT reads
  our files with no "no StreamerInfo" warning); `update` mode ✅
  (`append_histograms_file` adds objects to an existing file, with cycle bumps);
  streaming multi-cluster RNTuple write ✅ (`RNTupleWriter`, scalars +
  `std::string`/`std::vector<T>`, with reader-side cluster-offset globalization
  so real ROOT multi-cluster collection files read correctly). Remaining:
  >2 GiB._

## Analysis API

Build and fill objects from scratch, then save them — and read them back in
ROOT or uproot.

- **Histograms** — `TH1/TH2/TH3::new` (+ `new_variable` for non-uniform bins)
  and `TProfile::new`, with `fill`/`fill_weight` following ROOT's `Fill`
  semantics. `sumw2()` enables per-bin weighted errors (`bin_error`). Arithmetic:
  `scale`, `add` (the bin-by-bin merge for combining job outputs), `multiply`,
  `divide`, `integral` — all with Sumw2 error propagation. Save with
  `write_th{1,2,3}d_file`/`write_tprofile_file`, `write_histograms_file` (several
  at once), or `write_histograms_dirs` (organized into subdirectories). Files
  embed a `TStreamerInfo` list and read back in official ROOT (with correct bin
  errors and variable edges) and uproot. ✅
- **RNTuple** — `write_rntuple_file(path, name, &[Field], compression)` writes
  `bool`, 32/64-bit signed/unsigned ints, `f32`/`f64`, `std::string`, and
  `std::vector<T>` (bool/int/float) columns, optionally Zstd-compressed.
  `RNTupleWriter` streams those types one cluster per `write_batch` for large
  datasets. Validated with `RNTupleReader` and uproot. ✅
- _Follow-ups: float-precision (`TH1F`/`TH2F`/…) write, >2 GiB files, TTree
  read._

## License

Licensed under either of MIT or Apache-2.0 at your option.
