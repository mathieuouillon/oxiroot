# Changelog

Notable changes to oxiroot, by release. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): before 1.0, a minor release may
change the API.

## [Unreleased]

### Added

- **The generic reader decodes STL members.** A `std::vector`, `set`, `list`,
  `deque` or `map` holding numbers, strings, `TArray`s, nested containers,
  objects or pointers now decodes, whether ROOT streamed it objectwise or
  memberwise (a column per member), and so does a `TStreamerLoop` array of
  objects. `TF1`'s parameter-name map, `TEfficiency`'s per-bin beta parameters,
  `TGraphMultiErrors`'s error arrays and draw attributes, and `TH2Poly`'s cell
  grid were `Unsupported`; a `TH2Poly` read from a ROOT file now yields its
  bins. The shape comes from the member's C++ type name, and the container's
  byte count says whether that shape was right: a shape read wrong is reported
  as `Unsupported`, never passed off as values.
- `Value::Ref` — a slot holding an object written elsewhere in the same object,
  naming the class it points at. ROOT streams a shared object once and points
  at it from everywhere else it appears (a `TH2Poly`'s `fBins` points at the
  bins its `fCells` grid holds in full); those slots read as `Null` before, as
  if there were no object there. `ObjHeader::back_ref` says where the object
  was written, for a reader that wants to follow it.

### Changed

- **One offset convention.** An RNTuple's `FieldValues::Nested` and the writer's
  `Column::Nested` and `Column::Assoc` carry their cumulative `offsets` with a
  leading `0`, as a tree's `BranchValues::Nested` and `Jagged` do: element `i`
  spans `items[offsets[i]..offsets[i + 1]]`, and `offsets` holds one value more
  than there are elements. They held one cumulative end per element and no
  leading `0`, so the first element was the one you could not slice like the
  rest, and the two halves of the workspace disagreed. Code that reads or builds
  these offsets adds the leading `0`; the `vec_vec_*`, `set_*` and `map_*`
  constructors do it for you, and a file's bytes are unchanged, since the Index
  column on disk keeps holding one end per element.

### Fixed

- A `TStreamerLoop` element carries its counter member's name, like a
  `TStreamerBasicPointer`; it was parsed without one, so nothing could size the
  array it describes.

## [0.2.0] — 2026-09-23

A release of the follow-ups to the architecture review: errors a caller can act
on, one import path per item, files that describe only the classes they hold,
and a `hadd` that merges everything ROOT's does. Files written by 0.1.0 and by
0.2.0 read the same in ROOT, uproot and oxiroot.

### Added

- **`hadd` merges everything ROOT's does.** `merge_files` sums a `TEfficiency`'s
  passed and total histograms, a `TH2Poly`'s and a `THnSparse`'s bins, a
  `THStack`'s histograms (matched by name) and a `TParameter`'s value, and
  appends the points of `TGraph`, `TGraphErrors` and `TGraphAsymmErrors`, as
  ROOT's `hadd` does; they were copied from the first file. `TF1`/`TF2`/`TF3`,
  `TGraph2D`, `TGraphMultiErrors`, `TMultiGraph`, strings, maps and matrices are
  still copied, which is what ROOT's `hadd` does with them too — it writes one
  key per input, which oxiroot cannot, since it rejects two objects of the same
  name in one directory. The merges are `TEfficiency::add`, `TH2Poly::add`,
  `THnSparse::add`, `TGraph::append` and `TParameter::add`, and `Mergeable` now
  covers them, so the merger cannot be pointed at a type that has no merge.
- `Error::context` puts context in front of an error's message and keeps its
  variant.
- `streamer_gen::StreamerInfoList` parses a stored `TList<TStreamerInfo>` and
  gives the descriptions a set of classes needs, with the classes they depend
  on; `oxiroot_hist::hist_streamer_classes` does so for the histogram family's
  captured list.

### Changed

- **A file describes only the classes it holds.** A histogram-family object
  (histograms, profiles, graphs, `TEfficiency`, `THnSparse`, `TH2Poly`,
  `THStack`, `TMultiGraph`, `TF1`/`TF2`/`TF3`) used to embed the whole
  captured histogram-family streamer info, 38 KB. It now embeds its own class,
  the classes it depends on and the classes of the objects it holds. A file
  holding one histogram, graph or function shrinks from about 38 KB to 6–20
  KB; ROOT and uproot read every object as before.
- **Typed errors.** `Error` gains `NotFound`, `WrongClass`,
  `UnsupportedVersion`, `MissingStreamerInfo`, `ChecksumMismatch`,
  `Unsupported` and `InvalidInput`. The errors that fit them use them instead
  of `Error::Format`, which is left for bytes that break the ROOT format: a
  truncated or corrupt file. Remote (HTTP and XRootD) failures are `Error::Io`,
  with an `ErrorKind` that fits where there is one, such as `NotFound` for an
  HTTP 404. `SchemaChanged` also covers trees and RNTuples being concatenated,
  or the trees of a chain, that do not share a schema. Code that matched
  `Error::Format` for one of these conditions must match the new variant.

- **Each public item has one path.** `oxiroot-io-core`, `oxiroot-plot` and
  `oxiroot-stat` no longer expose their modules next to the root re-exports:
  import from the crate root (`oxiroot_io_core::RBuffer`, not
  `oxiroot_io_core::buffer::RBuffer`; `oxiroot_stat::Normal`, not
  `oxiroot_stat::distributions::Normal`). io-core keeps one public module,
  `streamer_gen`, for the helpers that describe a class's members, whose short
  names read best qualified. The items that were reachable only through a
  module are now at the root: io-core's byte buffers, `MmapSource`,
  `XrootdSource`, `MAGIC`, `BIG_FILE_VERSION` and `read_free`, and plot's
  `TickDir` and `Sides`.
- In the `oxiroot` facade, `oxiroot::file` holds all of io-core, like the
  facade's other per-crate modules, and `oxiroot::buffer` and `oxiroot::error`
  are gone: use `oxiroot::file::RBuffer` and `oxiroot::Error`.

### Removed

- `WriteRoot::streamer_blob` and `WriteInto::streamer_blob`, with
  `StreamerSet`'s serialized list (`add_list`, `list`, `blob`), and
  `oxiroot_hist::hist_streamer_blob`. Describe classes with
  `streamer_classes`, and take them from a captured list with
  `StreamerInfoList` or `hist_streamer_classes`.

### Fixed

- A histogram or profile with bin labels describes `TObjString`, so a reader
  that follows the file's streamer info decodes its labels.
- The captured histogram streamer info describes `THnSparseArrayChunk`,
  `TArrayD` and `TArray`. The script that captures it wrote an empty
  `THnSparse`, which holds no chunk, so a sparse histogram's chunks — and
  every histogram's `TArrayD` base — went undescribed.

### Internal

- The tree reader and writer and the RNTuple writer, which ran to 2,000–2,600
  lines each, are split into modules by concern. The API and the written bytes
  are unchanged.

## [0.1.0] — 2026-09-22

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

[Unreleased]: https://github.com/mathieuouillon/oxiroot/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/mathieuouillon/oxiroot/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mathieuouillon/oxiroot/releases/tag/v0.1.0
