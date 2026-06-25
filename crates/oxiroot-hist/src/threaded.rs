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
//! - [`ThreadedHist`] — the accumulator: hand out private clones, take them back,
//!   merge. Lock-free in the fill loop; works with [`std::thread::scope`].
//! - [`merge_all`] — fold an iterator of histograms into one (in-memory `hadd`).
//! - [`fill_par`] — one-call parallel fill of a slice (requires the `rayon`
//!   feature).

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
/// Hold one *template* histogram — a binning prototype, normally **empty**. Each
/// worker takes a private clone via [`local`](Self::local), fills it with **no
/// locking** in the hot loop, then hands it back with [`push`](Self::push) (one
/// brief lock per thread). [`merge`](Self::merge) folds the pushed locals into
/// the final histogram (returning an empty clone of the template if no work was
/// done). This mirrors ROOT exactly: `Get()` returns a copy of the model,
/// `Merge()` combines the per-slot copies. As in ROOT, a *pre-seeded* template is
/// replicated into every `local()`, so pass an empty histogram for a plain
/// parallel fill.
///
/// `&ThreadedHist` is [`Sync`], so it can be shared across [`std::thread::scope`]
/// workers without an `Arc`.
///
/// ```
/// use oxiroot_hist::{ThreadedHist, TH1};
///
/// let data: Vec<f64> = (0..1000).map(|i| i as f64 % 100.0).collect();
/// let mut template = TH1::new("h", "", 100, 0.0, 100.0);
/// template.sumw2();
///
/// let acc = ThreadedHist::new(template);
/// std::thread::scope(|s| {
///     for chunk in data.chunks(data.len().div_ceil(4)) {
///         let acc = &acc;
///         s.spawn(move || {
///             let mut h = acc.local();        // private clone, no lock
///             for &x in chunk {
///                 h.fill(x);                  // lock-free hot loop
///             }
///             acc.push(h);                    // one brief lock
///         });
///     }
/// });
/// let hist = acc.merge().unwrap();            // exact combine of all locals
/// assert_eq!(hist.entries, 1000.0);
/// ```
pub struct ThreadedHist<H: Merge> {
    template: H,
    locals: std::sync::Mutex<Vec<H>>,
}

impl<H: Merge> ThreadedHist<H> {
    /// Create an accumulator from a template histogram — a binning prototype,
    /// normally empty. Every [`local`](Self::local) is a clone of it.
    pub fn new(template: H) -> Self {
        Self {
            template,
            locals: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// A fresh private clone of the template for one worker to fill. The clone is
    /// owned by the caller, so the fill loop touches no shared state and takes no
    /// lock.
    pub fn local(&self) -> H {
        self.template.clone()
    }

    /// Hand a filled local back to the accumulator (one short lock). Call once
    /// per worker, after its fill loop.
    pub fn push(&self, local: H) {
        // The only critical section is this push (which cannot panic), so the
        // lock is never poisoned in practice; recover defensively regardless.
        self.locals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(local);
    }

    /// Number of locals handed back so far (diagnostic; takes the lock).
    pub fn num_slots(&self) -> usize {
        self.locals.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Fold every pushed local into the final histogram, consuming the
    /// accumulator. Returns an (empty) clone of the template if no local was
    /// pushed. Errors only if a local's binning diverged from the template
    /// (normal fills keep it).
    pub fn merge(self) -> Result<H> {
        let locals = self.locals.into_inner().unwrap_or_else(|e| e.into_inner());
        Ok(H::merge_all(locals)?.unwrap_or(self.template))
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
