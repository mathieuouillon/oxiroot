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
| `TH1`/`TH2`/`TH3`, `TProfile`/`TProfile2D`/`TProfile3D` | **summed** across every input that holds it (bin contents, `Sumw2`, entries, and all moment sums — the same exact `add` used by the [multithreaded fill](multithreading.md)) |
| `TGraph*`, `TEfficiency`, `TH2Poly`, `THnSparse`, `TF1`/`2`/`3`, `TObjString`, `TParameter`, `TVectorD`, `TMatrixD`/`Sym`, `THStack`, `TMultiGraph`, `TMap` | **copied** from the first file that holds it (ROOT's `hadd` keeps the first for non-addable objects too) |
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
`kind` of merge (`Histograms`, `Tree(name)`, or `RNTuple(name)`), the keys
`merged` / `copied` / `skipped` (with reasons), and the total `entries` for a
tree or RNTuple. Its `Display` prints a one-line summary. **Nothing is dropped
without appearing in `skipped`.**

## What a fileset may contain

One call writes **one** output file, and oxiroot does not yet assemble a single
container that *mixes* histograms with a `TTree`/RNTuple (each of those owns
auxiliary basket/page keys that a self-contained object writer does not model).
So a fileset must be one of:

- **all histogram-family objects** — summed / copied as above;
- **a single `TTree`** (and nothing else);
- **a single RNTuple** (and nothing else).

Anything else — a tree or RNTuple alongside histograms, or more than one of them
— is refused with an error that names the offending keys, rather than writing a
partial file. To merge such a fileset, merge the pieces separately with
`oxiroot_hist::merge_histogram_files`, `oxiroot_tree::concat_trees`, and
`oxiroot_rntuple::concat_ntuples`.

## Verification

oxiroot's merge is checked against ROOT 6.40's own `hadd`: the summed histogram
bin contents are identical, the concatenated tree matches entry-for-entry, and
the merged RNTuple is read back by both uproot and ROOT C++'s `RNTupleReader`.
See the [`merge` example](https://github.com/mathieuouillon/oxiroot/blob/main/crates/oxiroot/examples/merge.rs).
