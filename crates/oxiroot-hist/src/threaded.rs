//! Multithreaded histogram fill — the pure-Rust analog of ROOT's
//! [`TThreadedObject`](https://root.cern/doc/master/classROOT_1_1TThreadedObject.html).
//!
//! ROOT fills histograms across threads by giving each thread a private copy and
//! merging them at the end. oxiroot is set up for exactly this: every histogram
//! type is [`Clone`] plain data, and [`add(other, 1.0)`](crate::TH1::add) is an
//! *exact* reduction — it combines bin contents, per-bin `Sumw2`, the entry
//! count, and every statistical moment sum. So a parallel fill is just "clone per
//! thread, fill locally without locking, merge at the end", and the merged
//! histogram is identical to a serial fill (up to floating-point summation order).
//!
//! - [`Merge`] — the reduction trait (`merge` == `add(other, 1.0)`).
//! - [`ThreadedHist`] — the accumulator: share `&ThreadedHist`, call
//!   [`fill`](ThreadedHist::fill) from any thread (each gets its own copy), then
//!   [`merge`](ThreadedHist::merge) at the end. Works with [`std::thread::scope`].
//! - [`merge_all`] — fold an iterator of histograms into one (in-memory `hadd`).
//! - [`fill_par`] — one-call parallel fill of a slice (requires the `rayon`
//!   feature).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::ThreadId;

use oxiroot_io_core::error::Result;

use crate::{TProfile, TH1, TH2, TH3};

/// Histograms that combine into one — the reduction behind multithreaded fills
/// and `hadd`-style multi-file merges.
///
/// [`merge`](Merge::merge) is the bin-by-bin combine of `add(other, 1.0)`: it
/// sums contents, per-bin `Sumw2`, the entry count, and every moment sum, so the
/// result is identical to having filled one histogram with all the data. It
/// returns [`oxiroot_io_core::Error::BinningMismatch`] (leaving `self` unchanged)
/// if the binnings differ.
pub trait Merge: Clone + Send + Sized {
    /// Combine `other` into `self` (the `c == 1` case of `add`).
    fn merge(&mut self, other: &Self) -> Result<()>;

    /// Fold an iterator of histograms into one, starting from the first item.
    /// Returns `Ok(None)` for an empty iterator, or the binning-mismatch error
    /// from the first incompatible pair.
    fn merge_all<I: IntoIterator<Item = Self>>(items: I) -> Result<Option<Self>> {
        let mut it = items.into_iter();
        let Some(mut acc) = it.next() else {
            return Ok(None);
        };
        for h in it {
            acc.merge(&h)?;
        }
        Ok(Some(acc))
    }
}

macro_rules! impl_merge {
    ($($t:ty),+ $(,)?) => {$(
        impl Merge for $t {
            fn merge(&mut self, other: &Self) -> Result<()> {
                self.add(other, 1.0)
            }
        }
    )+};
}
impl_merge!(TH1, TH2, TH3, TProfile);

/// Fold an iterator of histograms into a single merged histogram (`Ok(None)` if
/// empty). Equivalent to ROOT's `hadd` over in-memory objects, and the reducer
/// behind [`ThreadedHist::merge`]. Errors on binning mismatch.
pub fn merge_all<H, I>(items: I) -> Result<Option<H>>
where
    H: Merge,
    I: IntoIterator<Item = H>,
{
    H::merge_all(items)
}

/// A multithreaded fill accumulator — the pure-Rust analog of ROOT's
/// `TThreadedObject<TH1>`.
///
/// Hold one *template* histogram (a binning prototype, normally **empty**), share
/// `&ThreadedHist` across threads, and call [`fill`](ThreadedHist::fill) from any
/// of them: each thread transparently gets its own private copy of the template
/// (created on first use) and fills it without contending with the others. At the
/// end, [`merge`](Self::merge) combines every thread's copy into one — exactly as
/// if a single histogram had been filled with all the data (up to floating-point
/// summation order). This mirrors ROOT's `TThreadedObject` (`Fill` per thread,
/// `Merge` at the end), without ROOT's explicit slot bookkeeping.
///
/// `&ThreadedHist` is [`Sync`], so it is shared across [`std::thread::scope`]
/// workers without an `Arc`. (Concurrent fills run in parallel under a shared
/// read lock; each thread's copy is touched only by that thread, so its lock is
/// never contended.)
///
/// ```
/// use oxiroot_hist::{ThreadedHist, TH1};
///
/// let data: Vec<f64> = (0..1000).map(|i| i as f64 % 100.0).collect();
/// let hist = ThreadedHist::new(TH1::new("h", "", 100, 0.0, 100.0));
///
/// std::thread::scope(|s| {
///     for chunk in data.chunks(data.len().div_ceil(4)) {
///         let hist = &hist;
///         s.spawn(move || {
///             for &x in chunk {
///                 hist.fill(x); // each thread fills its own copy — no setup
///             }
///         });
///     }
/// });
///
/// let merged = hist.merge().unwrap(); // combine every thread's copy
/// assert_eq!(merged.entries, 1000.0);
/// ```
pub struct ThreadedHist<H: Merge> {
    template: H,
    /// One private copy per thread, keyed by its (never-reused) [`ThreadId`].
    /// The `Arc` lets a fill clone its slot out and drop the map lock before
    /// touching the histogram, so a long fill never blocks other threads.
    slots: RwLock<HashMap<ThreadId, Arc<Mutex<H>>>>,
}

impl<H: Merge> ThreadedHist<H> {
    /// Create an accumulator from a template histogram — a binning prototype,
    /// normally empty. Each thread's private copy is a clone of it.
    pub fn new(template: H) -> Self {
        Self {
            template,
            slots: RwLock::new(HashMap::new()),
        }
    }

    /// Run `f` on the calling thread's private copy of the histogram, creating it
    /// (a clone of the template) on first use. This is the generic primitive
    /// behind [`fill`](ThreadedHist::fill): use it to call any method, or to fill
    /// a whole batch under a single slot acquisition:
    ///
    /// ```
    /// # use oxiroot_hist::{ThreadedHist, TH1};
    /// # let hist = ThreadedHist::new(TH1::new("h", "", 10, 0.0, 1.0));
    /// hist.with_local(|h| {
    ///     for x in [0.1, 0.2, 0.3] {
    ///         h.fill(x);
    ///     }
    /// });
    /// ```
    pub fn with_local<R>(&self, f: impl FnOnce(&mut H) -> R) -> R {
        let id = std::thread::current().id();
        // Clone this thread's slot (an Arc) out, releasing the map lock before
        // running `f` so a long fill never blocks other threads' first-touch.
        let slot = {
            let read = self.slots.read().unwrap_or_else(|e| e.into_inner());
            if let Some(slot) = read.get(&id) {
                Arc::clone(slot)
            } else {
                drop(read);
                Arc::clone(
                    self.slots
                        .write()
                        .unwrap_or_else(|e| e.into_inner())
                        .entry(id)
                        .or_insert_with(|| Arc::new(Mutex::new(self.template.clone()))),
                )
            }
        };
        let mut local = slot.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut local)
    }

    /// Number of threads that have filled so far (diagnostic).
    pub fn num_slots(&self) -> usize {
        self.slots.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Combine every thread's private copy into the final histogram, consuming
    /// the accumulator. Returns an (empty) clone of the template if no thread
    /// filled. Call after the workers have joined. Errors only if a copy's
    /// binning diverged from the template (normal fills keep it).
    pub fn merge(self) -> Result<H> {
        let slots = self.slots.into_inner().unwrap_or_else(|e| e.into_inner());
        let locals = slots.into_values().map(|arc| match Arc::try_unwrap(arc) {
            Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
            // A worker still holds a reference (it outlived merge): clone its copy.
            Err(arc) => arc.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        });
        Ok(H::merge_all(locals)?.unwrap_or(self.template))
    }
}

/// ROOT-style `Fill`: route to the calling thread's private copy, creating it on
/// first use. The headline convenience over [`with_local`](ThreadedHist::with_local).
impl ThreadedHist<TH1> {
    /// Fill the calling thread's copy with `x` (weight 1).
    pub fn fill(&self, x: f64) {
        self.with_local(|h| h.fill(x));
    }
    /// Fill the calling thread's copy with `x` and weight `w`.
    pub fn fill_weight(&self, x: f64, w: f64) {
        self.with_local(|h| h.fill_weight(x, w));
    }
}

impl ThreadedHist<TH2> {
    /// Fill the calling thread's copy at `(x, y)` (weight 1).
    pub fn fill(&self, x: f64, y: f64) {
        self.with_local(|h| h.fill(x, y));
    }
    /// Fill the calling thread's copy at `(x, y)` with weight `w`.
    pub fn fill_weight(&self, x: f64, y: f64, w: f64) {
        self.with_local(|h| h.fill_weight(x, y, w));
    }
}

impl ThreadedHist<TH3> {
    /// Fill the calling thread's copy at `(x, y, z)` (weight 1).
    pub fn fill(&self, x: f64, y: f64, z: f64) {
        self.with_local(|h| h.fill(x, y, z));
    }
    /// Fill the calling thread's copy at `(x, y, z)` with weight `w`.
    pub fn fill_weight(&self, x: f64, y: f64, z: f64, w: f64) {
        self.with_local(|h| h.fill_weight(x, y, z, w));
    }
}

impl ThreadedHist<TProfile> {
    /// Fill the calling thread's copy at `(x, y)` (weight 1).
    pub fn fill(&self, x: f64, y: f64) {
        self.with_local(|h| h.fill(x, y));
    }
    /// Fill the calling thread's copy at `(x, y)` with weight `w`.
    pub fn fill_weight(&self, x: f64, y: f64, w: f64) {
        self.with_local(|h| h.fill_weight(x, y, w));
    }
}

/// Fill a histogram from a slice in parallel (requires the `rayon` feature).
///
/// Convenience over [`ThreadedHist`] for the common "one histogram, fill from a
/// `&[T]`" case: rayon splits `data`, each task folds into a private
/// `template.clone()`, and the partial histograms reduce with [`Merge::merge`].
/// The result equals a serial fill (up to floating-point summation order).
///
/// `fill(&mut h, &item)` applies one item — e.g. `|h, &x| h.fill(x)` for a 1-D
/// histogram, or `|h, ev| h.fill_weight(ev.x, ev.w)`.
///
/// ```
/// # #[cfg(feature = "rayon")] {
/// use oxiroot_hist::{fill_par, TH1};
/// let data: Vec<f64> = (0..1000).map(|i| i as f64 % 100.0).collect();
/// let template = TH1::new("h", "", 100, 0.0, 100.0);
/// let hist = fill_par(&template, &data, |h, &x| h.fill(x));
/// assert_eq!(hist.entries, 1000.0);
/// # }
/// ```
#[cfg(feature = "rayon")]
pub fn fill_par<H, T, F>(template: &H, data: &[T], fill: F) -> H
where
    H: Merge + Sync,
    T: Sync,
    F: Fn(&mut H, &T) + Sync,
{
    use rayon::prelude::*;
    data.par_iter()
        .fold(
            || template.clone(),
            |mut h, item| {
                fill(&mut h, item);
                h
            },
        )
        .reduce(
            || template.clone(),
            |mut a, b| {
                // Every partial and identity is cloned from the same template, so
                // the binnings are identical and `merge` cannot mismatch here.
                a.merge(&b)
                    .expect("fill_par: identical binning by construction");
                a
            },
        )
}
