//! Histogram arithmetic: scale, add (merge), multiply, divide, integral.
//!
//! These follow ROOT's `TH1::Scale`/`Add`/`Multiply`/`Divide` semantics,
//! including per-bin error (`Sumw2`) propagation. `add` with `c = 1` is the
//! bin-by-bin merge used to combine outputs across parallel jobs (`hadd`).

use oxiroot_io_core::error::{Error, Result};

use crate::{TProfile, TProfile2D, TProfile3D, TH1, TH2, TH3};

/// Effective per-bin error² for `other`: its `fSumw2[i]` if tracked, else the
/// content (for an unweighted histogram, `Σw² == Σw == content`).
fn err2(sumw2: &[f64], contents: &[f64], i: usize) -> f64 {
    sumw2.get(i).copied().unwrap_or_else(|| contents[i].abs())
}

/// Build a [`Error::BinningMismatch`] describing which operation rejected which
/// pair of incompatible histograms.
fn binning_mismatch(op: &str) -> Error {
    Error::BinningMismatch {
        detail: format!("{op}: histograms have different binning"),
    }
}

impl TH1 {
    /// Multiply all bin contents (and errors) by `c`. The mean is preserved.
    ///
    /// Turns on per-bin error tracking first, as ROOT's `Scale` does: once
    /// scaled, the errors can no longer be derived as `√content`.
    pub fn scale(&mut self, c: f64) {
        self.sumw2();
        for v in &mut self.contents {
            *v *= c;
        }
        for v in &mut self.sumw2 {
            *v *= c * c;
        }
        self.tsumw *= c;
        self.tsumw2 *= c * c;
        self.tsumwx *= c;
        self.tsumwx2 *= c;
    }

    /// Add `c * other` into this histogram (a bin-by-bin merge when `c == 1`).
    /// Returns [`Error::BinningMismatch`] and makes no change if the binnings
    /// differ. Errors are tracked if either side tracks them (or `c != 1`).
    pub fn add(&mut self, other: &TH1, c: f64) -> Result<()> {
        if !self.xaxis.same_binning(&other.xaxis) || self.contents.len() != other.contents.len() {
            return Err(binning_mismatch("add"));
        }
        if self.sumw2.is_empty() && (!other.sumw2.is_empty() || c != 1.0) {
            self.sumw2 = self.contents.iter().map(|v| v.abs()).collect();
        }
        for i in 0..self.contents.len() {
            self.contents[i] += c * other.contents[i];
        }
        for i in 0..self.sumw2.len() {
            self.sumw2[i] += c * c * err2(&other.sumw2, &other.contents, i);
        }
        self.entries += c * other.entries;
        self.tsumw += c * other.tsumw;
        self.tsumw2 += c * c * other.tsumw2;
        self.tsumwx += c * other.tsumwx;
        self.tsumwx2 += c * other.tsumwx2;
        Ok(())
    }

    /// Sum of the in-range bin contents (excludes under/overflow).
    pub fn integral(&self) -> f64 {
        let n = self.contents.len();
        if n < 2 {
            return 0.0;
        }
        self.contents[1..n - 1].iter().sum()
    }

    /// Multiply bin-by-bin by `other`, propagating errors as ROOT does
    /// (`e² = e1²·c2² + e2²·c1²`). Returns [`Error::BinningMismatch`] if the
    /// binnings differ.
    pub fn multiply(&mut self, other: &TH1) -> Result<()> {
        if !self.xaxis.same_binning(&other.xaxis) || self.contents.len() != other.contents.len() {
            return Err(binning_mismatch("multiply"));
        }
        if self.sumw2.is_empty() {
            self.sumw2 = self.contents.iter().map(|v| v.abs()).collect();
        }
        for i in 0..self.contents.len() {
            let (c1, c2) = (self.contents[i], other.contents[i]);
            let (e1, e2) = (self.sumw2[i], err2(&other.sumw2, &other.contents, i));
            self.sumw2[i] = e1 * c2 * c2 + e2 * c1 * c1;
            self.contents[i] = c1 * c2;
        }
        Ok(())
    }

    /// Divide bin-by-bin by `other` (0 where the denominator is 0), propagating
    /// errors as ROOT's default `e² = (e1²·c2² + e2²·c1²) / c2⁴`. Returns
    /// [`Error::BinningMismatch`] if the binnings differ.
    pub fn divide(&mut self, other: &TH1) -> Result<()> {
        if !self.xaxis.same_binning(&other.xaxis) || self.contents.len() != other.contents.len() {
            return Err(binning_mismatch("divide"));
        }
        if self.sumw2.is_empty() {
            self.sumw2 = self.contents.iter().map(|v| v.abs()).collect();
        }
        for i in 0..self.contents.len() {
            let (c1, c2) = (self.contents[i], other.contents[i]);
            if c2 == 0.0 {
                self.contents[i] = 0.0;
                self.sumw2[i] = 0.0;
                continue;
            }
            let (e1, e2) = (self.sumw2[i], err2(&other.sumw2, &other.contents, i));
            let c2sq = c2 * c2;
            self.sumw2[i] = (e1 * c2sq + e2 * c1 * c1) / (c2sq * c2sq);
            self.contents[i] = c1 / c2;
        }
        Ok(())
    }
}

impl TH2 {
    /// Multiply all bin contents (and errors) by `c`.
    ///
    /// Turns on per-bin error tracking first, as ROOT's `Scale` does: once
    /// scaled, the errors can no longer be derived as `√content`.
    pub fn scale(&mut self, c: f64) {
        self.sumw2();
        for v in &mut self.contents {
            *v *= c;
        }
        for v in &mut self.sumw2 {
            *v *= c * c;
        }
        self.tsumw *= c;
        self.tsumw2 *= c * c;
        self.tsumwx *= c;
        self.tsumwx2 *= c;
        self.tsumwy *= c;
        self.tsumwy2 *= c;
        self.tsumwxy *= c;
    }

    /// Add `c * other` into this histogram (merge when `c == 1`). Returns
    /// [`Error::BinningMismatch`] if the binnings differ.
    pub fn add(&mut self, other: &TH2, c: f64) -> Result<()> {
        if !self.xaxis.same_binning(&other.xaxis)
            || !self.yaxis.same_binning(&other.yaxis)
            || self.contents.len() != other.contents.len()
        {
            return Err(binning_mismatch("add"));
        }
        if self.sumw2.is_empty() && (!other.sumw2.is_empty() || c != 1.0) {
            self.sumw2 = self.contents.iter().map(|v| v.abs()).collect();
        }
        for i in 0..self.contents.len() {
            self.contents[i] += c * other.contents[i];
        }
        for i in 0..self.sumw2.len() {
            self.sumw2[i] += c * c * err2(&other.sumw2, &other.contents, i);
        }
        self.entries += c * other.entries;
        self.tsumw += c * other.tsumw;
        self.tsumw2 += c * c * other.tsumw2;
        self.tsumwx += c * other.tsumwx;
        self.tsumwx2 += c * other.tsumwx2;
        self.tsumwy += c * other.tsumwy;
        self.tsumwy2 += c * other.tsumwy2;
        self.tsumwxy += c * other.tsumwxy;
        Ok(())
    }

    /// Sum of the in-range cell contents (excludes flow on both axes).
    pub fn integral(&self) -> f64 {
        let (nx, ny) = (self.nx(), self.ny());
        let stride = nx + 2;
        (1..=nx)
            .flat_map(|ix| (1..=ny).map(move |iy| (ix, iy)))
            .map(|(ix, iy)| self.contents[ix + stride * iy])
            .sum()
    }
}

impl TH3 {
    /// Multiply all bin contents (and errors) by `c`.
    ///
    /// Turns on per-bin error tracking first, as ROOT's `Scale` does: once
    /// scaled, the errors can no longer be derived as `√content`.
    pub fn scale(&mut self, c: f64) {
        self.sumw2();
        for v in &mut self.contents {
            *v *= c;
        }
        for v in &mut self.sumw2 {
            *v *= c * c;
        }
        self.tsumw *= c;
        self.tsumw2 *= c * c;
        self.tsumwx *= c;
        self.tsumwx2 *= c;
        self.tsumwy *= c;
        self.tsumwy2 *= c;
        self.tsumwxy *= c;
        self.tsumwz *= c;
        self.tsumwz2 *= c;
        self.tsumwxz *= c;
        self.tsumwyz *= c;
    }

    /// Add `c * other` into this histogram (merge when `c == 1`). Returns
    /// [`Error::BinningMismatch`] if the binnings differ.
    pub fn add(&mut self, other: &TH3, c: f64) -> Result<()> {
        if !self.xaxis.same_binning(&other.xaxis)
            || !self.yaxis.same_binning(&other.yaxis)
            || !self.zaxis.same_binning(&other.zaxis)
            || self.contents.len() != other.contents.len()
        {
            return Err(binning_mismatch("add"));
        }
        if self.sumw2.is_empty() && (!other.sumw2.is_empty() || c != 1.0) {
            self.sumw2 = self.contents.iter().map(|v| v.abs()).collect();
        }
        for i in 0..self.contents.len() {
            self.contents[i] += c * other.contents[i];
        }
        for i in 0..self.sumw2.len() {
            self.sumw2[i] += c * c * err2(&other.sumw2, &other.contents, i);
        }
        self.entries += c * other.entries;
        self.tsumw += c * other.tsumw;
        self.tsumw2 += c * c * other.tsumw2;
        self.tsumwx += c * other.tsumwx;
        self.tsumwx2 += c * other.tsumwx2;
        self.tsumwy += c * other.tsumwy;
        self.tsumwy2 += c * other.tsumwy2;
        self.tsumwxy += c * other.tsumwxy;
        self.tsumwz += c * other.tsumwz;
        self.tsumwz2 += c * other.tsumwz2;
        self.tsumwxz += c * other.tsumwxz;
        self.tsumwyz += c * other.tsumwyz;
        Ok(())
    }

    /// Sum of the in-range cell contents (excludes flow on all axes).
    pub fn integral(&self) -> f64 {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let (sx, sy) = (nx + 2, ny + 2);
        (1..=nx)
            .flat_map(|ix| (1..=ny).flat_map(move |iy| (1..=nz).map(move |iz| (ix, iy, iz))))
            .map(|(ix, iy, iz)| self.contents[ix + sx * (iy + sy * iz)])
            .sum()
    }
}

/// Add `c · src` into `dst` for a per-cell array a file may leave empty, where
/// empty means every cell is zero (a profile's `fSumw2`). An empty `dst` is
/// filled with zeros first so `src` is never dropped; an empty `src` adds nothing.
fn add_optional_cells(dst: &mut Vec<f64>, src: &[f64], c: f64, ncells: usize) {
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        dst.resize(ncells, 0.0);
    }
    for (d, &s) in dst.iter_mut().zip(src) {
        *d += c * s;
    }
}

/// Whether an optional per-cell array is either absent or covers every cell.
fn optional_len_ok(len: usize, ncells: usize) -> bool {
    len == 0 || len == ncells
}

/// `add` for the whole profile family. The three profiles differ only in their
/// axes, the name of their per-cell `Σw·v²` array and their moment sums, so one
/// body serves them all: the per-cell error bookkeeping is exactly where
/// hand-copied versions had already drifted apart.
macro_rules! impl_profile_add {
    (
        $ty:ident, $dim:literal,
        axes: [$($axis:ident),+],
        value_sq: $sumv2:ident,
        moments: [$($moment:ident),+ $(,)?] $(,)?
    ) => {
        impl $ty {
            #[doc = concat!("Merge `c * other` into this ", $dim, " profile cell by cell (a")]
            /// `hadd`-style merge when `c == 1`). A profile cannot be merged like a
            /// plain histogram: its per-cell weight sums (`fBinEntries`), weighted-value
            /// sums (the base contents), weighted-value² sums (`fSumw2`) and `Σw²`
            /// (`fBinSumw2`) must all be summed so the profiled value `Σwv / Σw` and its
            /// error stay correct.
            ///
            /// As in ROOT's `TProfileHelper::Add`, `c` scales the weights by `|c|`
            /// and a negative `c` flips the sign of the profiled values: the
            /// weighted-value sums scale by `c`; the weight sums, weighted-value²
            /// sums, entries and moments by `|c|`; and `Σw²` by `c²`. Returns
            /// [`Error::BinningMismatch`], leaving `self` unchanged, if the binnings
            /// or per-cell array lengths differ.
            pub fn add(&mut self, other: &$ty, c: f64) -> Result<()> {
                let ncells = self.sums.len();
                let compatible = $(self.$axis.same_binning(&other.$axis))&&+
                    && other.sums.len() == ncells
                    && self.bin_entries.len() == ncells
                    && other.bin_entries.len() == ncells
                    && optional_len_ok(self.$sumv2.len(), ncells)
                    && optional_len_ok(other.$sumv2.len(), ncells)
                    && optional_len_ok(self.bin_sumw2.len(), ncells)
                    && optional_len_ok(other.bin_sumw2.len(), ncells);
                if !compatible {
                    return Err(binning_mismatch("add"));
                }
                // The weights of `other` enter scaled by |c|.
                let ac = c.abs();
                // Seed our Σw² from the weight sums (exact for unit weights) before
                // any sum changes, whenever it is about to stop being derivable from
                // them: `other` tracks it explicitly, or its weights are rescaled,
                // since Σ(|c|·w)² ≠ Σ(|c|·w) unless |c| == 1.
                if !other.bin_sumw2.is_empty() || ac != 1.0 {
                    self.track_bin_sumw2();
                }
                if !self.bin_sumw2.is_empty() {
                    // `other`'s Σw² is its `fBinSumw2` if tracked, else its weight sums.
                    let src: &[f64] = if other.bin_sumw2.is_empty() {
                        &other.bin_entries
                    } else {
                        &other.bin_sumw2
                    };
                    // Σw² is quadratic in the weight: Σ(c·w)² = c²·Σw².
                    for (d, &s) in self.bin_sumw2.iter_mut().zip(src) {
                        *d += c * c * s;
                    }
                }
                for (d, &s) in self.sums.iter_mut().zip(&other.sums) {
                    *d += c * s;
                }
                for (d, &s) in self.bin_entries.iter_mut().zip(&other.bin_entries) {
                    *d += ac * s;
                }
                add_optional_cells(&mut self.$sumv2, &other.$sumv2, ac, ncells);

                self.entries += ac * other.entries;
                // Σw² is quadratic in the weight; every other moment is linear in
                // it, so takes |c|.
                self.tsumw2 += c * c * other.tsumw2;
                $(self.$moment += ac * other.$moment;)+
                Ok(())
            }
        }
    };
}

impl_profile_add!(
    TProfile, "1-D",
    axes: [xaxis],
    value_sq: sumy2,
    moments: [tsumw, tsumwx, tsumwx2, tsumwy, tsumwy2],
);
impl_profile_add!(
    TProfile2D, "2-D",
    axes: [xaxis, yaxis],
    value_sq: sumz2,
    moments: [tsumw, tsumwx, tsumwx2, tsumwy, tsumwy2, tsumwxy, tsumwz, tsumwz2],
);
impl_profile_add!(
    TProfile3D, "3-D",
    axes: [xaxis, yaxis, zaxis],
    value_sq: sumt2,
    moments: [
        tsumw, tsumwx, tsumwx2, tsumwy, tsumwy2, tsumwxy, tsumwz, tsumwz2, tsumwxz, tsumwyz,
        tsumwt, tsumwt2,
    ],
);

// --- Standard operator/formatting traits over the inherent histogram ops. ---
// `scale` is infallible, so `*=`/`*` are clean; `add`/`multiply`/`divide` stay
// inherent + fallible (binning can mismatch), so no `AddAssign`/`+`.

macro_rules! impl_scale_ops {
    ($ty:ty) => {
        impl std::ops::MulAssign<f64> for $ty {
            /// `h *= c` scales every bin (and its error) by `c`, like ROOT's `Scale`.
            fn mul_assign(&mut self, c: f64) {
                self.scale(c);
            }
        }
        impl std::ops::Mul<f64> for $ty {
            type Output = $ty;
            fn mul(mut self, c: f64) -> $ty {
                self.scale(c);
                self
            }
        }
    };
}
impl_scale_ops!(TH1);
impl_scale_ops!(TH2);
impl_scale_ops!(TH3);

/// `h[cell]` reads, and `for &c in &h` iterates, bin contents by flat cell
/// index — `0` is the first under/overflow cell, x varies fastest — the same
/// indexing as [`TH1::bin_error`]. Both cover every cell, flow bins included.
macro_rules! impl_index_iter {
    ($ty:ty) => {
        impl std::ops::Index<usize> for $ty {
            type Output = f64;
            fn index(&self, cell: usize) -> &f64 {
                &self.contents[cell]
            }
        }
        impl<'a> IntoIterator for &'a $ty {
            type Item = &'a f64;
            type IntoIter = std::slice::Iter<'a, f64>;
            fn into_iter(self) -> std::slice::Iter<'a, f64> {
                self.contents.iter()
            }
        }
    };
}
impl_index_iter!(TH1);
impl_index_iter!(TH2);
impl_index_iter!(TH3);

impl std::fmt::Display for TH1 {
    /// A one-line summary, e.g. `TH1D "pt": 100 bins [0, 100), entries=4096`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:?}: {} bins [{}, {}), entries={}",
            self.class_name(),
            self.name,
            self.xaxis.nbins.max(0),
            self.xaxis.xmin,
            self.xaxis.xmax,
            self.entries
        )
    }
}

impl std::fmt::Display for TH2 {
    /// A one-line summary, e.g. `TH2D "h": 100x100 bins, entries=4096`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:?}: {}x{} bins, entries={}",
            self.class_name(),
            self.name,
            self.xaxis.nbins.max(0),
            self.yaxis.nbins.max(0),
            self.entries
        )
    }
}

impl std::fmt::Display for TH3 {
    /// A one-line summary, e.g. `TH3D "h": 20x20x20 bins, entries=4096`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:?}: {}x{}x{} bins, entries={}",
            self.class_name(),
            self.name,
            self.xaxis.nbins.max(0),
            self.yaxis.nbins.max(0),
            self.zaxis.nbins.max(0),
            self.entries
        )
    }
}

/// Uniform read access to a classic histogram's bins, implemented by
/// [`TH1`]/[`TH2`]/[`TH3`] so generic code can work over any of them. (The
/// mutating ops — `scale`/`add`/`integral` — stay inherent per type, since each
/// also updates its own moment sums and dimension-specific in-range cells.)
pub trait Histogram {
    /// All bin contents including under/overflow, flat (x fastest).
    fn contents(&self) -> &[f64];
    /// Number of fills (`fEntries`).
    fn entries(&self) -> f64;
    /// Sum of every cell's content, under/overflow included.
    fn sum(&self) -> f64 {
        self.contents().iter().sum()
    }
    /// Whether the histogram has no cells.
    fn is_empty(&self) -> bool {
        self.contents().is_empty()
    }
}

macro_rules! impl_histogram {
    ($ty:ty) => {
        impl Histogram for $ty {
            fn contents(&self) -> &[f64] {
                &self.contents
            }
            fn entries(&self) -> f64 {
                self.entries
            }
        }
    };
}
impl_histogram!(TH1);
impl_histogram!(TH2);
impl_histogram!(TH3);
