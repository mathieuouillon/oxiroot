# Histograms

oxiroot provides the classic ROOT histogram family — `TH1`/`TH2`/`TH3` in every
precision, profiles, and the specialised `TEfficiency`/`THnSparse`/`TH2Poly`
types — as plain Rust structs you fill, transform, and read or write to ROOT
files. This page covers construction, filling, arithmetic, statistics, derived
histograms, compatibility tests, and persistence. (ROOT 7 `RHist` has no
on-disk format and is intentionally out of scope.)

## Construction without names

A histogram is just data: a binning and its contents. Unlike ROOT, oxiroot never
forces a name at construction and keeps no global directory, so any number of
anonymous (or same-named) histograms coexist in memory. A name is only needed
when you *persist* the object — it becomes the file key.

```rust
use oxiroot::prelude::*;

// Anonymous histograms — no gROOT, no gDirectory.
let mut h = Hist::reg(100, 0.0, 100.0).double();

// Name and title are fluent, chainable, and set whenever you like.
let mut pt = Hist::reg(100, 0.0, 100.0).double()
    .named("pt")
    .titled("p_{T} spectrum");
```

`named` sets the on-disk key (`fName`); `titled` sets the title (`fTitle`). Both
return `Self`, so they chain off the constructor.

### Uniform vs variable bins

`Hist::reg(nbins, xmin, xmax).double()` builds uniform bins over `[xmin, xmax)`.
`Hist::var(edges).double()` takes the `nbins + 1` bin boundaries directly (they
must be strictly ascending):

```rust
use oxiroot::prelude::*;

let uniform = Hist::reg(100, 0.0, 100.0).double();

let edges = [0.0, 10.0, 20.0, 40.0, 80.0, 160.0];
let variable = Hist::var(&edges).double(); // 5 bins of growing width
```

Chain another `reg`/`var` per axis for higher dimensions:
`Hist::reg(nx, xlo, xhi).reg(ny, ylo, yhi).double()` is a `TH2`, and a third
`.reg(nz, zlo, zhi)` makes it a `TH3`. `reg` and `var` mix freely on any axis, so
`Hist::var(xedges).var(yedges).double()` gives variable bins on both axes of a
`TH2`, and a variable axis works on `TH3` too.

## Filling

`fill(x)` adds an entry with unit weight; `fill_weight(x, w)` uses weight `w`.
Both follow ROOT's exact `Fill` semantics: every fill increments `fEntries`, but
the statistical moment sums accumulate only for in-range fills.

```rust
use oxiroot::prelude::*;

let mut pt = Hist::var(&[0.0, 10.0, 20.0, 40.0, 80.0, 160.0]).double().named("pt");
pt.sumw2(); // track per-bin errors before filling (see below)

for &(x, w) in &[(5.0, 1.2), (15.0, 0.8), (35.0, 1.5)] {
    pt.fill_weight(x, w);
}
```

`TH2`/`TH3` add coordinates: `h2.fill(x, y)` / `h2.fill_weight(x, y, w)` and
`h3.fill(x, y, z)` / `h3.fill_weight(x, y, z, w)`.

### Per-bin errors with `sumw2`

`sumw2()` enables ROOT's `Sumw2` error tracking: it allocates the `fSumw2` array,
seeds it from the current contents, and from then on every fill also accumulates
`weight²`. Call it *before* filling for correct weighted errors. It returns
`&mut Self`, so it chains:

```rust
let mut h = Hist::reg(100, 0.0, 1.0).double();
h.sumw2().fill(0.5); // chain enable + fill
```

`bin_error(bin)` then returns `sqrt(sumw2[bin])`; without tracking it falls back
to the Poisson default `sqrt(content)`. The `bin` index includes flow: `0` is
underflow, `1..=nbins` are in range.

!!! note
    Bin indexing is 1-based with flow bins, matching ROOT. Cell `0` is underflow,
    `nbins + 1` is overflow. `values()` returns the in-range contents only, while
    indexing (`h[cell]`) and `contents` cover every cell including flow.

## Precision and class names

A `TH1`/`TH2`/`TH3` keeps its bin contents as `f64` in memory regardless of
on-disk precision. The class suffix (`D`/`F`/`I`/`S`/`C`/`L`) is a typed
[`Precision`](../api/oxiroot/index.html) value chosen by the builder's storage
finalizer: `double()` → `TH1D`, `float()` → `TH1F`, `int32()` → `TH1I`,
`int16()` → `TH1S`, `int8()` → `TH1C`, `int64()` → `TH1L`. To change the
precision of a histogram you already built or read back, use `with_precision`;
either way the contents are narrowed only at write time.

```rust
use oxiroot::prelude::*;

let h = Hist::reg(100, 0.0, 1.0).float().named("h");
assert_eq!(h.class_name(), "TH1F"); // the finalizer picked the class

// Re-precision an existing histogram (e.g. store a filled TH1D compactly):
let hc = h.clone().with_precision(Precision::Char);
assert_eq!(hc.class_name(), "TH1C");
```

| Method | Returns |
| --- | --- |
| `precision()` | the typed `Precision` (the class suffix) |
| `with_precision(p)` | `Self` with the on-disk precision set |
| `class_name()` | the exact ROOT class, e.g. `"TH1D"` / `"TH2F"` |

`Precision` covers `Double`, `Float`, `Int`, `Short`, `Char`, and `Long`.

## The type family

Every type below reads and writes through the same `WriteRoot`/`ReadRoot` traits.

| Type | Class(es) | Construct with |
| --- | --- | --- |
| `TH1` | `TH1D/F/I/S/C/L` | `Hist::reg(nbins, xmin, xmax).double()`, `Hist::var(edges).double()` |
| `TH2` | `TH2D/F/I/S/C/L` | `Hist::reg(nx, xlo, xhi).reg(ny, ylo, yhi).double()`, `Hist::var(xe).var(ye).double()` |
| `TH3` | `TH3D/F/I/S/C/L` | `Hist::reg(nx, xlo, xhi).reg(ny, ylo, yhi).reg(nz, zlo, zhi).double()` |
| `TProfile` | `TProfile` | `Hist::reg(nbins, xlo, xhi).profile()` |
| `TProfile2D` | `TProfile2D` | `Hist::reg(nx, xlo, xhi).reg(ny, ylo, yhi).profile()` |
| `TProfile3D` | `TProfile3D` | `Hist::reg(nx, xlo, xhi).reg(ny, ylo, yhi).reg(nz, zlo, zhi).profile()` |
| `TEfficiency` | `TEfficiency` | `TEfficiency::new(nbins, xlo, xhi)`, then `fill(passed, x)` |
| `THnSparse` | `THnSparseT<TArrayD>` | `THnSparse::new(&[(nbins, lo, hi), …])`, then `fill(&coords)` |
| `TH2Poly` | `TH2Poly` | `TH2Poly::new(xlow, xup, ylow, yup)`, then `add_bin`/`add_bin_rect` |

Profiles store per-bin sums of `w·y` and `w·y²`; the profiled value of a bin is
`sum / entries`. The per-bin error follows the profile's typed
[`ErrorMode`](../api/oxiroot/index.html) (`Mean`, `Spread`, `SpreadI`, `SpreadG`,
mapping ROOT's `fErrorMode`). `TProfile2D::fill(x, y, z)` profiles `z` against
`(x, y)`; `TProfile3D::fill(x, y, z, t)` profiles `t`.

```rust
use oxiroot::prelude::*;

let mut prof = Hist::reg(5, 0.0, 5.0).profile().named("pt_prof").titled("<pt> per region");
prof.fill(0.5, 91.2);
prof.fill(1.5, 125.1);
let profiled = prof.values(); // sum / entries per in-range bin
```

`TEfficiency` wraps a passed/total pair of `TH1D`s; `TH2Poly` supports
arbitrary-shape polygon bins (`add_bin(&xs, &ys)`) and axis-aligned rectangles
(`add_bin_rect(xmin, ymin, xmax, ymax)`), both returning the new bin number.

## Arithmetic

Histogram arithmetic follows ROOT's `Scale`/`Add`/`Multiply`/`Divide`, including
per-bin `Sumw2` error propagation. Operations that can fail on a binning mismatch
stay inherent and fallible; the infallible `scale` is also exposed as `*`/`*=`.

| Operation | Signature | Notes |
| --- | --- | --- |
| scale | `h.scale(c)` | also `h *= c` and `h * c`; the mean is preserved |
| add / merge | `h.add(&other, c)?` | adds `c·other`; `c = 1` is the bin-by-bin `hadd` merge |
| multiply | `h.multiply(&other)?` | `TH1` only |
| divide | `h.divide(&other)?` | `TH1` only; `0` where the denominator is `0` |
| integral | `h.integral()` | sum of the in-range bins (excludes flow) |

`add`, `multiply`, and `divide` return `Error::BinningMismatch` and make no
change if the binnings differ. `add` is implemented for `TH1`/`TH2`/`TH3` and
`TProfile` (which merges its per-bin weight sums correctly).

```rust
use oxiroot::prelude::*;

let mut signal = pt.clone();
let mut background = pt.clone();

background *= 0.1;                 // MulAssign — scale background down
signal.add(&background, 1.0)?;     // stack background onto signal (a merge)
signal *= 1.0 / signal.integral().max(1.0); // normalize to unit area
```

### Indexing, iteration, and Display

`h[cell]` reads a bin content by flat cell index (`0` is the first under/overflow
cell, x varies fastest); `for &c in &h` iterates every cell. The shared
`Histogram` trait abstracts over `TH1`/`TH2`/`TH3` with `contents()`,
`entries()`, `sum()` (every cell, flow included), and `is_empty()`. Every
histogram implements `Display` for a one-line summary.

```rust
use oxiroot::prelude::*;

let bin2 = h[2];        // Index: content of cell 2
let total = h.sum();    // Histogram trait: every cell, flow included
println!("{h}");        // e.g. TH1D "pt": 100 bins [0, 100), entries=4096
```

## Statistics and shape

All accessors are pure derivations from the stored contents and moment sums; no
new on-disk state.

| Accessor | Meaning |
| --- | --- |
| `mean()` | mean of the in-range fills (`TH2`/`TH3` use `mean_x`/`mean_y`/`mean_z`) |
| `std_dev()` | standard deviation, ROOT `GetStdDev`/`GetRMS` (`TH2`/`TH3`: `std_dev_x`/`_y`/`_z`) |
| `maximum()` / `minimum()` | largest / smallest in-range bin content |
| `maximum_bin()` / `minimum_bin()` | bin index of the extremum |
| `find_bin(x)` | bin holding `x` (0 = underflow, `nbins+1` = overflow) |
| `bin_center(bin)` / `bin_width(bin)` / `bin_low_edge(bin)` | 1-based bin geometry |
| `effective_entries()` | `(Σw)² / Σw²` |
| `reset()` | clear contents, errors, entries, and moments; keep the binning |
| `interpolate(x)` | linear interpolation between adjacent bin centers |
| `quantiles(&probs)` | x values where the cumulative reaches each probability |

```rust
use oxiroot::prelude::*;

println!("mean {:.3}, std {:.3}", mass.mean(), mass.std_dev());
println!("max bin {} = {}", mass.maximum_bin(), mass.maximum());

let medians = mass.quantiles(&[0.25, 0.5, 0.75]); // quartiles
let mid = mass.interpolate(50.0);                 // content at x = 50
```

!!! tip
    `interpolate` and `quantiles` reproduce ROOT's behaviour exactly, including
    ROOT's tie-handling quirk where a probability landing on a cumulative bin
    boundary returns that bin's center.

The underlying axis is exposed as `xaxis`/`yaxis`/`zaxis` ([`TAxis`](../api/oxiroot/index.html)),
with `edges()`, the O(1) `edge(i)`, `find_bin(x)`, and the same
`bin_center`/`bin_width`/`bin_low_edge` helpers.

## Derived histograms

Each derived operation returns an existing histogram type and carries over the
statistical moment sums, so `mean()`/`std_dev()` stay correct on the result.
Per-bin `Sumw2` aggregates as a sum of variances and is preserved only when the
source tracks it.

| Operation | From → to | Description |
| --- | --- | --- |
| `rebin(ngroup)` | `TH1` → `TH1` | merge `ngroup` adjacent bins; leftovers fold into overflow |
| `rebin2d(ngx, ngy)` | `TH2` → `TH2` | merge `ngx`×`ngy` blocks |
| `rebin3d(ngx, ngy, ngz)` | `TH3` → `TH3` | merge `ngx`×`ngy`×`ngz` blocks |
| `cumulative(forward)` | `TH1` → `TH1` | running sum, forward or reverse |
| `projection_x(name)` / `projection_y(name)` | `TH2` → `TH1` | sum the other axis |
| `projection_x/y/z(name)` | `TH3` → `TH1` | sum the other two axes |
| `projection_xy/xz/yz(name)` | `TH3` → `TH2` | sum the dropped axis |
| `profile_x(name)` / `profile_y(name)` | `TH2` → `TProfile` | profile along an axis |

```rust
use oxiroot::prelude::*;

let coarse = fine.rebin(4);                 // group every 4 bins
let cdf = fine.cumulative(true);            // forward running sum
let px = corr.projection_x("px");           // TH2 → TH1
let prof = corr.profile_x("pfx");           // TH2 → TProfile
```

## Compatibility tests

`TH1` supports ROOT's `Chi2Test` and `KolmogorovTest`, returning ROOT-matched
p-values. Both require identical binning and otherwise return
`Error::BinningMismatch`.

```rust
use oxiroot::prelude::*;

let chi2 = data.chi2_test(&model)?;                 // Pearson χ², "UU"
println!("χ²/ndf = {:.2}, p = {:.3}", chi2.chi2 / chi2.ndf as f64, chi2.p_value);

// Weighted variants pick the scheme explicitly.
let ww = data.chi2_test_with(&model, Chi2TestKind::WeightedWeighted)?;

let ks = data.kolmogorov_test(&model)?;
println!("KS distance {:.4}, prob {:.3}", ks.distance, ks.prob);
```

`Chi2TestKind` selects the weighting scheme: `UnweightedUnweighted` (`"UU"`, the
default of `chi2_test`), `UnweightedWeighted` (`"UW"`), or `WeightedWeighted`
(`"WW"`). `Chi2TestResult` carries `p_value`, `chi2`, and `ndf`; `KsTestResult`
carries `prob` and `distance`.

## Labelled (alphanumeric) axes

A `TAxis` can carry alphanumeric bin labels (`fLabels`), which round-trip through
ROOT files in both directions.

```rust
use oxiroot::prelude::*;

let mut h = Hist::reg(3, 0.0, 3.0).double().named("by_region");
h.xaxis.set_label(1, "barrel");
h.xaxis.set_label(2, "endcap");
h.xaxis.set_label(3, "forward");

assert_eq!(h.xaxis.bin_label(2), Some("endcap"));
assert_eq!(h.xaxis.find_label("forward"), Some(3));
```

`set_label(bin, text)` and `bin_label(bin)` use 1-based bin numbers; `labels` is
the raw `Vec<String>`, `find_label` maps a label back to its bin, and
`is_labelled` reports whether any label is set.

## Writing and reading

The `WriteRoot` trait writes any single object; the `RootFile` builder composes
several objects (and subdirectories) into one file. The `ReadRoot` trait reads
one object back by key. Written files embed a `TStreamerInfo` list, so they are
self-describing for ROOT and uproot.

### One object

```rust
use oxiroot::prelude::*;

let h = Hist::reg(100, 0.0, 1.0).double().named("h");
h.write_root("out.root", Compression::Zstd(5))?; // works for any writable type

let bytes = h.to_root_bytes(); // just the streamed object payload
```

`write_root` requires a non-empty name; writing an unnamed object is an error.
`to_root_bytes` returns the streamed payload (no file/key framing).

### Several objects with `RootFile`

```rust
use oxiroot::prelude::*;

let pt = Hist::reg(10, 0.0, 1.0).double().named("pt");
let prof = Hist::reg(10, 0.0, 1.0).profile().named("prof");
let signal = Hist::reg(10, 0.0, 1.0).double().named("sig");

RootFile::create("out.root")
    .add(&pt)                            // any &dyn WriteRoot: hist, profile, graph…
    .add(&prof)
    .dir("by_region", |d| d.add(&signal)) // a TDirectory holding `sig`
    .write(Compression::Zstd(5))?;
```

Append to an existing file with `RootFile::open`. The append is *in place* —
existing objects never move — so files that already contain subdirectories or an
RNTuple are preserved (added objects land in the top directory; only *creating*
new subdirectories while appending is unsupported):

```rust
use oxiroot::prelude::*;

RootFile::open("out.root")?
    .add(&extra)
    .write(Compression::None)?;
```

!!! warning "Loud collisions"
    Writing an object with no name, or two objects sharing a name in one
    directory, is a loud error — `Error::DuplicateName` for the collision case —
    not ROOT's silent shadow-on-read. Give every persisted object a unique key
    with `.named("...")`.

### Reading back

```rust
use oxiroot::prelude::*;

let f = RFile::open("out.root")?;
let h = TH1::read_root(&f, "pt")?;            // any of TH1D/F/I/S/C/L
let sig = TH1::read_root_in(&f, "by_region", "sig")?; // from a subdirectory
```

`read_root` auto-detects the on-disk precision (every `TH1D/F/I/S/C/L` reads into
a `TH1`, with the exact class preserved in `class_name()`); contents are widened
to `f64`. `read_root_in(file, dir, name)` reads from a subdirectory written via
`RootFile::dir`.

See [Compression](compression.md) for the codec options and
[ROOT / uproot interop](interop.md) for the cross-language read/write guarantee.

## Fitting and multithreading

Fitting and multithreaded fills have dedicated pages:

- Fit a parametric model (`gaussian`/`exponential`/`polynomial`, or a closure) to
  any 1-D data — `TH1` and `TGraph` implement `FitData` — via `.fit(&model)`.
  Requires the `fit` feature. See [Fitting](fitting.md).
- `ThreadedHist` is the pure-Rust analog of ROOT's `TThreadedObject<TH1>`: share
  `&hist`, call `hist.fill(x)` from any thread, then `hist.merge()`. See
  [Multithreading](multithreading.md).

## See also

- [Fitting](fitting.md)
- [Multithreading](multithreading.md)
- [ROOT / uproot interop](interop.md)
- [Quickstart](../getting-started/quickstart.md)
