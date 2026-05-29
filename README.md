# root-rs

Pure-Rust IO for the [CERN ROOT](https://root.cern) file format — read and write
**RNTuple** and **classic histograms** (TH1/TH2/TH3/TProfile) in the ROOT (TFile)
container, with **no C++/libROOT dependency**.

> Status: early development. See [the implementation plan](../.claude/plans/i-want-to-implement-melodic-teacup.md)
> for the full design and milestone roadmap.

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
- **M4** — TFile write + a TH1D ROOT can read. _In progress: `TH1D` object
  serialization is byte-identical to ROOT (`write_th1d`); next is the TFile
  container (header, first key/TDirectory, key list, free list, streamer info)._
- **M5** — RNTuple write.
- **M6** — Round-trip / interop hardening.

## License

Licensed under either of MIT or Apache-2.0 at your option.
