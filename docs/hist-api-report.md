# Histogram API: a scikit-hep `hist`–flavoured layer over ROOT histograms

This report covers an API improvement to oxiroot's histograms, drawing on
[scikit-hep/hist](https://github.com/scikit-hep/hist) (the ergonomic Python
histogram library built on `boost-histogram`), **while keeping every histogram a
real ROOT `TH1`/`TH2`/`TH3`** that round-trips through ROOT C++ and uproot.

## Goal and constraint

`hist` is the de-facto modern histogram API in HEP Python. Its appeal is three
things: a fluent **quick-construction** builder (`Hist.new.Reg(...).Weight()`),
**named axes with labels**, and the **`values`/`variances`/`counts`/`density`**
accessor family. oxiroot already had the ROOT data model and a solid
`TH1::new(...).named().titled()` API; what it lacked was the `hist` ergonomics.

The hard constraint: oxiroot's histograms are the on-disk ROOT objects, so any
new API had to map onto ROOT concepts and **survive a ROOT write/read**. It does
— see [Verification](#verification).

## What `hist` offers, and how it maps to ROOT

| `hist` | ROOT concept | oxiroot before | oxiroot now |
|---|---|---|---|
| `Hist.new.Reg(n, lo, hi)` | `TH1` uniform axis | `TH1::new(n, lo, hi)` | `Hist::reg(n, lo, hi)` |
| `.Var(edges)` | variable axis (`fXbins`) | `TH1::new_variable(edges)` | `Hist::var(edges)` |
| `.Double()` | `TH1D` | `.with_precision(Double)` | `.double()` |
| `.Int64()` | `TH1L` (64-bit) | `.with_precision(Long)` | `.int64()` |
| `.Weight()` | `TH1D` + `Sumw2` | `.sumw2()` | `.weight()` |
| (ROOT-only) | `TH1F` (32-bit) | `.with_precision(Float)` | `.float()` |
| axis `name=` | `fXaxis.fName` (`"xaxis"`) | fixed | fixed (as ROOT) |
| axis `label=` | **`fXaxis.fTitle`** | only via `h.xaxis.title` | `.label()` / `with_x_label()` |
| `h.values` | bin contents | `values()` | `values()` |
| `h.variances` | `Sumw2` (or content) | — | **`variances()`** |
| `np.sqrt(variances)` | `GetBinError` | `bin_error(i)` | **`errors()`** |
| `h.counts` | effective entries | — | **`counts()`** |
| `h.density()` | normalized density | — | **`density()`** |
| `h[hist.loc(x)]` | `GetBinContent(FindBin(x))` | `find_bin` + `values` | **`at(x)`** |
| `h.project`/`profile`/`sum` | projections / integral | `projection_x`, `profile_x`, `integral` | (unchanged) |

The two `hist` storages with no `TH1` equivalent are intentionally **not**
mapped: a `Mean`/`WeightedMean` storage is ROOT's `TProfile` (a different class,
already supported separately), and categorical (`IntCategory`/`StrCategory`) axes
have no `TH1` representation — they are a `boost-histogram` concept, out of scope
for a ROOT-compatible type. ROOT's alphanumeric **bin labels** (`fLabels`) are a
narrower thing oxiroot already reads and writes.

## The new API

### Quick construction (`hist`'s `Hist.new`)

```rust
use oxiroot::prelude::*;

// hist:  Hist.new.Reg(50, 0, 100, name="pt", label="$p_T$").Weight()
let mut h = Hist::reg(50, 0.0, 100.0)
    .name("pt")              // ROOT fName (the key it is stored under)
    .title("Z candidates")   // ROOT fTitle
    .label("$p_T$ [GeV]")    // ROOT fXaxis.fTitle — the axis label
    .weight();               // TH1D + Sumw2 (value and variance)

// 2-D and 3-D chain extra axes; `.label()` names the most recent axis:
let h2 = Hist::reg(40, -4.0, 4.0).label("x").reg(40, -4.0, 4.0).label("y").double();

// Regular and variable axes mix freely:
let h3 = Hist::var(&[0.0, 1.0, 4.0, 10.0]).reg(20, 0.0, 1.0).int64();
```

The builder is a small **type-state**: `Hist::reg`/`var` → `H1`, then `.reg`/`.var`
→ `H2` → `H3`, and the storage finalizer on each returns the matching ROOT class
(`TH1`/`TH2`/`TH3`). `int64()` maps to `TH1L`, so 64-bit integer storage is exact.

### Accessors (`hist`'s `.values`/`.variances`/`.counts`/`.density`)

```rust
let mut h = Hist::reg(4, 0.0, 4.0).weight();
h.fill_weight(0.5, 2.0);
h.fill_weight(1.5, 3.0);

h.values();      // [2.0, 3.0, 0.0, 0.0]   bin contents (in-range)
h.variances();   // [4.0, 9.0, 0.0, 0.0]   Sumw2 = Σ w²   (or the content, unweighted)
h.errors();      // [2.0, 3.0, 0.0, 0.0]   √variance — the error bars
h.counts();      // [1.0, 1.0, 0.0, 0.0]   effective entries = value² / variance
h.density();     // [0.4, 0.6, 0.0, 0.0]   Σ density·width = 1
h.at(0.5);       // 2.0                    content at a coordinate (hist's loc)
```

The semantics follow `hist`/`boost-histogram` exactly: `variances` is the stored
`Sumw2` when weights are tracked, otherwise the bin content (the Poisson variance
ROOT assumes — matching `TH1::GetBinError`); `counts` is the effective number of
entries; `density` integrates to one.

### Axis labels on existing histograms

These also work on a histogram **read back from a ROOT file**, so you can relabel
and re-save:

```rust
let h = TH1::read_root(&f, "pt")?
    .with_x_label("$p_T$ [GeV]")
    .with_y_label("Events / 2 GeV");
assert_eq!(h.x_label(), "$p_T$ [GeV]");
```

## Verification

Every part of the new API was checked against all three oracles:

1. **oxiroot round-trip** — a builder-made `Hist::reg(...).label("$p_T$ [GeV]").weight()`
   writes and reads back with the axis label and the `Sumw2` variances intact
   (`crates/oxiroot-hist/tests/quick_hist.rs`).
2. **ROOT C++** reads oxiroot's output and reports the axis label and bin errors:
   ```
   class=TH1D name=pt title=transverse momentum
   xaxis title = $p_T$ [GeV]
   nbins=4 bin1=2 bin2=3 err1=2 err2=3
   ```
   `err = √Sumw2`, so the weighted variances survive. The axis label is in
   `fXaxis.fTitle`, exactly where ROOT and uproot look for it.
3. The label round-trips because the writer already emits `fXaxis.fTitle`
   (`write_taxis` → `write_tnamed`), so **no on-disk format change was needed** —
   the improvement is purely additive API over the existing ROOT serialization.

## Design notes

- **Additive, not a rewrite.** `TH1::new(...)` and the rest are untouched; `Hist`
  is a thin builder and the accessors are new methods. Nothing about the ROOT
  data model or serialization changed.
- **Type-state builder** keeps the finalizer return type correct (`H2.double()`
  is a `TH2`, not a `TH1`) without runtime checks.
- **`int64()` → `TH1L`**, the closest ROOT class to `boost-histogram`'s 64-bit
  integer storage, so integer-count histograms are stored exactly.
- The single thing `hist` does that a `TH1` fundamentally cannot is **categorical
  axes**; those stay out of scope because they have no ROOT representation.
