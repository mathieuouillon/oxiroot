# Multithreaded fill

`ThreadedHist` is the pure-`std` analog of ROOT's
[`TThreadedObject<TH1>`](https://root.cern/doc/master/classROOT_1_1TThreadedObject.html):
share one accumulator across threads, fill it from any of them, then merge the
per-thread copies into a single histogram that is identical to a serial fill.
This page covers `ThreadedHist`, the `Merge` trait and `merge_all`, and the
optional `rayon`-powered `fill_par`.

## The model

ROOT fills histograms in parallel by handing each thread a private copy and
combining them at the end. oxiroot is built for exactly this pattern: every
histogram type is plain `Clone` data, and `add(other, 1.0)` is an *exact*
reduction — it sums bin contents, per-bin `Sumw2`, the entry count, and every
statistical moment sum. A parallel fill is therefore "clone per thread, fill
locally without locking, merge at the end", and the merged result equals a
single-threaded fill up to floating-point summation order.

`ThreadedHist` packages that pattern so your code never touches a lock, an
`Arc`, or explicit slot bookkeeping — just `fill`.

## `ThreadedHist`

Construct an accumulator from a *template* histogram (a binning prototype,
normally empty). Share `&hist` across `std::thread::scope` workers and call
`hist.fill(x)` from any of them: each thread transparently gets its own private
copy of the template, created on first use and filled without contending with
the others. After the workers join, `hist.merge()` consumes the accumulator and
combines every copy into the final histogram.

```rust
use oxiroot::prelude::*;

let data: Vec<f64> = (0..1_000_000).map(|i| (i as f64 * 0.618) % 100.0).collect();

// A binning prototype (normally empty); each thread copies it on first fill.
let hist = ThreadedHist::new(Hist::reg(100, 0.0, 100.0).double().named("mass"));

let n_threads = std::thread::available_parallelism().map_or(4, |n| n.get());

std::thread::scope(|s| {
    for chunk in data.chunks(data.len().div_ceil(n_threads)) {
        let hist = &hist;
        s.spawn(move || {
            // No per-thread setup, no locks in your code — just fill.
            for &x in chunk {
                hist.fill(x);
            }
        });
    }
});

// Combine every thread's copy into one — exact (contents + entries + moments).
let merged = hist.merge().expect("identical binning");
assert_eq!(merged.entries, data.len() as f64);
```

`&ThreadedHist` is `Sync`, so it is shared across `std::thread::scope` without
an `Arc`. Concurrent fills run in parallel under a shared read lock; each
thread's copy is touched only by that thread, so its slot lock is never
contended.

!!! note "Empty template"
    The template is a *binning prototype*. Keep it empty: its entries and bin
    contents are not part of the merge baseline, but a non-empty template means
    every thread's copy starts pre-filled, double-counting on merge. Build the
    binning (and `named`/`titled` metadata), then hand it to `ThreadedHist::new`.

### Methods

| Method | Description |
| --- | --- |
| `ThreadedHist::new(template)` | Build an accumulator from a template histogram. |
| `fill(...)` | Fill the calling thread's private copy with weight 1. Arity matches the histogram type (see below). |
| `fill_weight(..., w)` | Fill the calling thread's copy with weight `w`. |
| `with_local(\|h\| ...)` | Run a closure on the calling thread's copy — reach any method, or batch a whole loop under one slot acquisition. |
| `num_slots()` | Number of threads that have filled so far (diagnostic). |
| `merge()` | Consume the accumulator, returning the combined histogram (`Result<H>`). |

The `fill` / `fill_weight` convenience methods are provided for every
fillable type — `TH1`, `TH2`, `TH3`, and `TProfile` — with the matching arity:

| Type | `fill` | `fill_weight` |
| --- | --- | --- |
| `ThreadedHist<TH1>` | `fill(x)` | `fill_weight(x, w)` |
| `ThreadedHist<TH2>` | `fill(x, y)` | `fill_weight(x, y, w)` |
| `ThreadedHist<TH3>` | `fill(x, y, z)` | `fill_weight(x, y, z, w)` |
| `ThreadedHist<TProfile>` | `fill(x, y)` | `fill_weight(x, y, w)` |

### `with_local`

`fill` is a thin convenience over `with_local`, the generic primitive that runs
a closure on the calling thread's copy (creating it on first use). Use it to
reach any histogram method, or to fold a whole batch under a single slot
acquisition instead of one per value:

```rust
use oxiroot::prelude::*;

let hist = ThreadedHist::new(Hist::reg(100, 0.0, 100.0).double().named("h"));

hist.with_local(|h| {
    for x in batch {
        h.fill_weight(x, weight_of(x));
    }
});
```

### Diagnostics

`num_slots()` reports how many threads have created a private copy so far —
useful for confirming the work actually spread across the pool:

```rust
println!("filled across {} thread-local copies", hist.num_slots());
```

!!! tip "Identical to a serial fill"
    The merged histogram's bin contents match a single-threaded fill *exactly*;
    the moment sums (used for `mean`, `std_dev`, etc.) agree up to floating-point
    summation order. You can assert `merged.values() == serial.values()` and
    `merged.entries == serial.entries` against a reference serial fill.

## `Merge` and `merge_all`

`merge()` is built on the `Merge` trait, implemented for `TH1`, `TH2`, `TH3`,
and `TProfile`. `Merge::merge(&mut self, other)` is the bin-by-bin combine of
`add(other, 1.0)`; it returns
[`Error::BinningMismatch`](../api/oxiroot/index.html) (leaving `self` unchanged)
when the binnings differ.

The free function `merge_all` folds an iterator of histograms into one — an
in-memory equivalent of ROOT's `hadd`. It returns `Ok(None)` for an empty
iterator, or the binning-mismatch error from the first incompatible pair.

```rust
use oxiroot::prelude::*;

// Combine partial histograms (e.g. one per input file) into a single result.
let partials: Vec<TH1> = load_partial_histograms();
let total: Option<TH1> = merge_all(partials)?;
```

`Merge` is also available as a method form (`Merge::merge_all`) on any
implementing type.

## Data-parallel fill with `rayon`

For the common "one histogram, fill from a `&[T]`" case, the optional `rayon`
feature adds a single-call parallel fill. rayon splits `data`, each task folds
items into a private `template.clone()`, and the partials reduce with
`Merge::merge`:

```rust
use oxiroot::prelude::*;

let data: Vec<f64> = (0..1_000_000).map(|i| i as f64 % 100.0).collect();
let template = Hist::reg(100, 0.0, 100.0).double().named("h");

let hist = fill_par(&template, &data, |h, &x| h.fill(x));
assert_eq!(hist.entries, data.len() as f64);
```

The closure `|h, item|` applies one element, so it generalizes beyond 1-D — for
example `|h, ev| h.fill_weight(ev.x, ev.w)` over a slice of event structs.

Enable the feature in `Cargo.toml`:

```toml
[dependencies]
oxiroot = { version = "*", features = ["rayon"] }
```

!!! warning "Summation order"
    `fill_par` differs from a serial fill in floating-point *summation order*:
    bin contents and moment sums agree to rounding, not bit-for-bit. This is the
    same caveat as `ThreadedHist::merge` and is inherent to any parallel
    reduction over floats. The bin *counts* (for unweighted fills) are exact.

## Choosing an approach

| Use | When |
| --- | --- |
| `ThreadedHist` | You control thread spawning, want to fill from arbitrary call sites, or are filling more than one histogram in the same scope. |
| `fill_par` | One histogram, data already in a `&[T]`, and you want the parallelism handled for you (requires `rayon`). |
| `merge_all` | You already have a collection of compatible histograms to combine (in-memory `hadd`). |

A full runnable example lives at
[`crates/oxiroot/examples/threaded.rs`](https://github.com/mathieuouillon/oxiroot/blob/main/crates/oxiroot/examples/threaded.rs)
(`cargo run -p oxiroot --example threaded`).

## See also

- [Histograms](histograms.md)
- [Fitting](fitting.md)
- [Compression](compression.md)
- [API reference](../api/oxiroot/index.html)

`std::thread::scope`: https://doc.rust-lang.org/std/thread/fn.scope.html
`Sync`: https://doc.rust-lang.org/std/marker/trait.Sync.html
