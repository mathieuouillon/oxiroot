# Merging files (hadd)

`oxiroot::hadd` is a pure-Rust [`hadd`](https://root.cern/doc/master/classTFileMerger.html):
it combines several ROOT files the way ROOT's most-used command-line tool does —
**histograms summed** bin-by-bin, **`TTree` and RNTuple entries concatenated** —
with no libROOT dependency. The output is written through oxiroot's own typed
writers, so ROOT and uproot read it back.

```rust
use oxiroot::hadd::merge_files;
use oxiroot::Compression;

let report = merge_files("all.root", &["run1.root", "run2.root"], Compression::Zstd(5))?;
println!("{report}"); // merged 2 file(s) into all.root: 3 summed, 1 copied, 0 skipped
# Ok::<(), oxiroot::Error>(())
```

## What each key becomes

The union of the inputs' top-level keys (in first-seen order) is merged key by
key:

| Class | Action |
|---|---|
| `Hist1D`/`Hist2D`/`Hist3D`, `Profile1D`/`Profile2D`/`Profile3D` | **summed** across every input that holds it (bin contents, `Sumw2`, entries, and all moment sums — the same exact `add` used by the [multithreaded fill](multithreading.md)) |
| `PolyHist`, `SparseHist` | **summed** bin by bin, with the entry count and moment sums |
| `Efficiency` | **summed**: its passed and its total histogram |
| `Graph`/`TGraphErrors`/`TGraphAsymmErrors` | **appended**: the inputs' points in order, with their errors, as ROOT's `Graph::Merge` does |
| `HistStack` | **merged** histogram by histogram, matched by name |
| `Parameter` | **summed** values |
| `Func1D`/`2`/`3`, `Graph2D`, `MultiErrorGraph`, `GraphStack`, `ObjString`, `Vector`, `Matrix`/`Sym`, `ObjMap` | **copied** from the first file that holds it. ROOT's `hadd` does not merge these either: it writes one key per input, which oxiroot cannot do, since it rejects two objects of the same name in one directory |
| anything else | **skipped**, and listed in the report — never silently dropped |

An object that cannot be read from one of the inputs is also skipped and listed,
with the input it failed in; a key is never written as a partial sum.

The summed histogram keeps the first file's name, title, and binning; summing
histograms with incompatible binnings is an error that names the key, exactly as
ROOT's `hadd` refuses it.

## `TTree` and RNTuple

A file holding a single `TTree` (and nothing else) has its entries
concatenated:

```rust
use oxiroot::hadd::merge_files;
use oxiroot::Compression;

let report = merge_files("events.root", &["skim1.root", "skim2.root"], Compression::Zstd(5))?;
println!("{report}"); // TTree "Events" with <n1 + n2> entries
# Ok::<(), oxiroot::Error>(())
```

Each branch is reconstructed with its original kind — scalar, fixed array
(`x[N]`), jagged (`x[n]`), `std::vector<T>`, or string. An RNTuple fileset works
the same way (fields concatenated). Under the hood these are the standalone
[`oxiroot_tree::concat_trees`] and [`oxiroot_rntuple::concat_ntuples`] functions,
which you can call directly for finer control.

[`oxiroot_tree::concat_trees`]: https://docs.rs/oxiroot-tree
[`oxiroot_rntuple::concat_ntuples`]: https://docs.rs/oxiroot-rntuple

## The `Merger` builder

`Merger` is the composable form — collect inputs, pick compression, then merge:

```rust
use oxiroot::hadd::Merger;
use oxiroot::Compression;

let report = Merger::new()
    .inputs(["run1.root", "run2.root", "run3.root"])
    .compression(Compression::Zstd(9))
    .merge("all.root")?;
# Ok::<(), oxiroot::Error>(())
```

## The report

`merge_files` returns a `MergeReport` describing exactly what happened: the
`kind` of merge (`Histograms`, `Tree(name)`, or `Ntuple(name)`), the keys
`merged` / `copied` / `skipped` (with reasons), and the total `entries` for a
tree or RNTuple. Its `Display` prints a one-line summary. **Nothing is dropped
without appearing in `skipped`.**

## What a fileset may contain

One call writes **one** output file, and the merger does not yet combine
histograms with a `TTree`/RNTuple in that output (a `FileWriter` can hold all three,
but the merger concatenates each tree or RNTuple on its own path). So a fileset
must be one of:

- **all histogram-family objects** — summed / copied as above;
- **a single `TTree`** (and nothing else);
- **a single RNTuple** (and nothing else).

Anything else — a tree or RNTuple alongside histograms, or more than one of them
— is refused with an error that names the offending keys, rather than writing a
partial file. To merge such a fileset, merge the pieces separately with
`oxiroot::hadd::merge_histogram_files`, `oxiroot_tree::append_trees` (or
`concat_trees`), and `oxiroot_rntuple::append_ntuples` (or `concat_ntuples`).

## Memory

Inputs are opened with positioned reads, so only the objects and data a merge
touches are read. A tree or RNTuple is streamed to the output one input at a
time: each input becomes one batch of baskets (or one RNTuple cluster), so memory
holds a single input's entries rather than the whole merged dataset. When the
inputs add up to more than 1 GB, or the output turns out not to fit the 32-bit
container form, the output is written in ROOT's 64-bit form. The output path must
not be one of the inputs.

## Verification

oxiroot's merge is checked against ROOT 6.40's own `hadd`. Two files written by
ROOT, holding one object of every class the merge handles, are merged by both:
every object ROOT merges comes out the same, key by key (`fixtures/hadd_*.root`,
from `scripts/gen_hadd_inputs.cpp`). The concatenated tree matches
entry-for-entry, and the merged RNTuple is read back by both uproot and ROOT
C++'s `RNTupleReader`.
See the [`merge` example](https://github.com/mathieuouillon/oxiroot/blob/main/crates/oxiroot/examples/merge.rs).
