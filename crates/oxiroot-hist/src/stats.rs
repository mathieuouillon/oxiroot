//! Statistics accessors mirroring ROOT's `TH1`/`TH2`/`TH3`/`TProfile` API —
//! `GetStdDev`, `GetMaximum`/`GetMaximumBin`, `FindBin`, `GetBinCenter`,
//! `GetEffectiveEntries`, `Reset`, … All are pure derivations from the already
//! stored bin contents and statistical moment sums (no new on-disk state).

use crate::{TProfile, TH1, TH2, TH3};

/// Population standard deviation along one axis from its moment sums
/// (`sqrt(<x²> − <x>²)`), matching ROOT's `GetStdDev`. `0.0` for an empty axis.
fn std_dev_axis(tsumw: f64, tsumwx: f64, tsumwx2: f64) -> f64 {
    if tsumw == 0.0 {
        return 0.0;
    }
    let mean = tsumwx / tsumw;
    (tsumwx2 / tsumw - mean * mean).max(0.0).sqrt()
}

/// Effective entry count `(Σw)² / Σw²` (ROOT's `GetEffectiveEntries`).
fn eff_entries(tsumw: f64, tsumw2: f64) -> f64 {
    if tsumw2 > 0.0 {
        tsumw * tsumw / tsumw2
    } else {
        0.0
    }
}

/// `(cell index, value)` of the largest (or smallest) of the given cells.
fn extremum(contents: &[f64], cells: impl Iterator<Item = usize>, want_max: bool) -> (usize, f64) {
    let mut best_cell = 0usize;
    let mut best_val = if want_max {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    for c in cells {
        let v = contents[c];
        if (want_max && v > best_val) || (!want_max && v < best_val) {
            best_val = v;
            best_cell = c;
        }
    }
    (best_cell, best_val)
}

impl TH1 {
    /// Standard deviation of the filled x distribution (ROOT `GetStdDev`, also
    /// exposed as `GetRMS`).
    pub fn std_dev(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwx, self.tsumwx2)
    }
    /// Largest in-range bin content (ROOT `GetMaximum`).
    pub fn maximum(&self) -> f64 {
        extremum(&self.contents, 1..=self.xaxis.nbins.max(0) as usize, true).1
    }
    /// Smallest in-range bin content (ROOT `GetMinimum`).
    pub fn minimum(&self) -> f64 {
        extremum(&self.contents, 1..=self.xaxis.nbins.max(0) as usize, false).1
    }
    /// Bin of the largest in-range content (1-based; ROOT `GetMaximumBin`).
    pub fn maximum_bin(&self) -> usize {
        extremum(&self.contents, 1..=self.xaxis.nbins.max(0) as usize, true).0
    }
    /// Bin of the smallest in-range content (1-based; ROOT `GetMinimumBin`).
    pub fn minimum_bin(&self) -> usize {
        extremum(&self.contents, 1..=self.xaxis.nbins.max(0) as usize, false).0
    }
    /// Bin holding `x` (0 = underflow, `1..=nbins`, `nbins+1` = overflow).
    pub fn find_bin(&self, x: f64) -> usize {
        self.xaxis.find_bin(x)
    }
    /// Center of bin `bin` (1-based).
    pub fn bin_center(&self, bin: usize) -> f64 {
        self.xaxis.bin_center(bin)
    }
    /// Width of bin `bin` (1-based).
    pub fn bin_width(&self, bin: usize) -> f64 {
        self.xaxis.bin_width(bin)
    }
    /// Low edge of bin `bin` (1-based).
    pub fn bin_low_edge(&self, bin: usize) -> f64 {
        self.xaxis.bin_low_edge(bin)
    }
    /// Effective entry count `(Σw)²/Σw²` (ROOT `GetEffectiveEntries`).
    pub fn effective_entries(&self) -> f64 {
        eff_entries(self.tsumw, self.tsumw2)
    }

    /// Linearly interpolate the bin content at `x` between adjacent bin centers
    /// (ROOT `TH1::Interpolate`). Returns the first/last bin content when `x` is
    /// at or beyond the first/last bin center.
    #[must_use]
    pub fn interpolate(&self, x: f64) -> f64 {
        let n = self.xaxis.nbins.max(0) as usize;
        if n == 0 {
            return 0.0;
        }
        let content = |bin: usize| self.contents.get(bin).copied().unwrap_or(0.0);
        if x <= self.bin_center(1) {
            return content(1);
        }
        if x >= self.bin_center(n) {
            return content(n);
        }
        let xbin = self.find_bin(x).clamp(1, n);
        let (lo, hi) = if x <= self.bin_center(xbin) {
            (xbin - 1, xbin)
        } else {
            (xbin, xbin + 1)
        };
        let (x0, x1) = (self.bin_center(lo), self.bin_center(hi));
        let (y0, y1) = (content(lo), content(hi));
        y0 + (x - x0) * (y1 - y0) / (x1 - x0)
    }

    /// The `x` values where the cumulative bin-content distribution reaches each
    /// probability in `probs` (ROOT `TH1::GetQuantiles`); `probs` should lie in
    /// `[0, 1]`. Within a bin the inverse CDF is interpolated linearly across the
    /// bin's edges; a probability landing exactly on a cumulative bin boundary
    /// returns that bin's center, matching ROOT.
    #[must_use]
    pub fn quantiles(&self, probs: &[f64]) -> Vec<f64> {
        let n = self.xaxis.nbins.max(0) as usize;
        // Cumulative (normalized) in-range contents: integral[i] = Σ_{1..i} / total.
        let mut integral = vec![0.0; n + 1];
        for i in 1..=n {
            integral[i] = integral[i - 1] + self.contents.get(i).copied().unwrap_or(0.0);
        }
        let total = integral[n];
        if total <= 0.0 {
            return vec![self.xaxis.xmin; probs.len()];
        }
        for v in &mut integral {
            *v /= total;
        }
        probs
            .iter()
            .map(|&p| {
                // Largest `ibin` in `0..n` with integral[ibin] <= p (ROOT's
                // TMath::BinarySearch searches the first `n` cumulative values).
                let ibin = integral[..n].iter().rposition(|&c| c <= p).unwrap_or(0);
                if integral[ibin] == p {
                    return self.bin_center(ibin); // exact cumulative tie → bin center
                }
                let dint = integral[ibin + 1] - integral[ibin];
                if dint > 0.0 {
                    self.bin_low_edge(ibin + 1)
                        + (p - integral[ibin]) / dint * self.bin_width(ibin + 1)
                } else {
                    self.bin_low_edge(ibin + 1)
                }
            })
            .collect()
    }
    /// Clear all bin contents, errors, entries, and moment sums (ROOT `Reset`),
    /// keeping the binning.
    pub fn reset(&mut self) {
        self.contents.iter_mut().for_each(|v| *v = 0.0);
        self.sumw2.iter_mut().for_each(|v| *v = 0.0);
        self.entries = 0.0;
        self.tsumw = 0.0;
        self.tsumw2 = 0.0;
        self.tsumwx = 0.0;
        self.tsumwx2 = 0.0;
    }
}

/// In-range cell indices of a 2-D histogram (x fastest), matching `integral()`.
fn cells_2d(nx: usize, ny: usize) -> impl Iterator<Item = usize> {
    let stride = nx + 2;
    (1..=nx).flat_map(move |ix| (1..=ny).map(move |iy| ix + stride * iy))
}

impl TH2 {
    /// Standard deviation of the filled x distribution (ROOT `GetStdDev(1)`).
    pub fn std_dev_x(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwx, self.tsumwx2)
    }
    /// Standard deviation of the filled y distribution (ROOT `GetStdDev(2)`).
    pub fn std_dev_y(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwy, self.tsumwy2)
    }
    /// Largest in-range cell content.
    pub fn maximum(&self) -> f64 {
        extremum(&self.contents, cells_2d(self.nx(), self.ny()), true).1
    }
    /// Smallest in-range cell content.
    pub fn minimum(&self) -> f64 {
        extremum(&self.contents, cells_2d(self.nx(), self.ny()), false).1
    }
    /// Global cell index of the largest in-range content.
    pub fn maximum_bin(&self) -> usize {
        extremum(&self.contents, cells_2d(self.nx(), self.ny()), true).0
    }
    /// Global cell index of the smallest in-range content.
    pub fn minimum_bin(&self) -> usize {
        extremum(&self.contents, cells_2d(self.nx(), self.ny()), false).0
    }
    /// Global cell index `ix + (nx+2)*iy` for `(x, y)` (flow bins included).
    pub fn find_bin(&self, x: f64, y: f64) -> usize {
        self.xaxis.find_bin(x) + (self.nx() + 2) * self.yaxis.find_bin(y)
    }
    /// Effective entry count `(Σw)²/Σw²`.
    pub fn effective_entries(&self) -> f64 {
        eff_entries(self.tsumw, self.tsumw2)
    }
    /// Clear all contents, errors, entries, and moment sums, keeping the binning.
    pub fn reset(&mut self) {
        self.contents.iter_mut().for_each(|v| *v = 0.0);
        self.sumw2.iter_mut().for_each(|v| *v = 0.0);
        self.entries = 0.0;
        self.tsumw = 0.0;
        self.tsumw2 = 0.0;
        self.tsumwx = 0.0;
        self.tsumwx2 = 0.0;
        self.tsumwy = 0.0;
        self.tsumwy2 = 0.0;
        self.tsumwxy = 0.0;
    }
}

/// In-range cell indices of a 3-D histogram (x fastest, then y, then z).
fn cells_3d(nx: usize, ny: usize, nz: usize) -> impl Iterator<Item = usize> {
    let (sx, sy) = (nx + 2, ny + 2);
    (1..=nx).flat_map(move |ix| {
        (1..=ny).flat_map(move |iy| (1..=nz).map(move |iz| ix + sx * (iy + sy * iz)))
    })
}

impl TH3 {
    /// Standard deviation of the filled x distribution.
    pub fn std_dev_x(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwx, self.tsumwx2)
    }
    /// Standard deviation of the filled y distribution.
    pub fn std_dev_y(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwy, self.tsumwy2)
    }
    /// Standard deviation of the filled z distribution.
    pub fn std_dev_z(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwz, self.tsumwz2)
    }
    /// Largest in-range cell content.
    pub fn maximum(&self) -> f64 {
        extremum(
            &self.contents,
            cells_3d(self.nx(), self.ny(), self.nz()),
            true,
        )
        .1
    }
    /// Smallest in-range cell content.
    pub fn minimum(&self) -> f64 {
        extremum(
            &self.contents,
            cells_3d(self.nx(), self.ny(), self.nz()),
            false,
        )
        .1
    }
    /// Global cell index of the largest in-range content.
    pub fn maximum_bin(&self) -> usize {
        extremum(
            &self.contents,
            cells_3d(self.nx(), self.ny(), self.nz()),
            true,
        )
        .0
    }
    /// Effective entry count `(Σw)²/Σw²`.
    pub fn effective_entries(&self) -> f64 {
        eff_entries(self.tsumw, self.tsumw2)
    }
    /// Clear all contents, errors, entries, and moment sums, keeping the binning.
    pub fn reset(&mut self) {
        self.contents.iter_mut().for_each(|v| *v = 0.0);
        self.sumw2.iter_mut().for_each(|v| *v = 0.0);
        self.entries = 0.0;
        for v in [
            &mut self.tsumw,
            &mut self.tsumw2,
            &mut self.tsumwx,
            &mut self.tsumwx2,
            &mut self.tsumwy,
            &mut self.tsumwy2,
            &mut self.tsumwxy,
            &mut self.tsumwz,
            &mut self.tsumwz2,
            &mut self.tsumwxz,
            &mut self.tsumwyz,
        ] {
            *v = 0.0;
        }
    }
}

impl TProfile {
    /// Mean of the filled x distribution (ROOT `GetMean`).
    pub fn mean(&self) -> f64 {
        if self.tsumw == 0.0 {
            0.0
        } else {
            self.tsumwx / self.tsumw
        }
    }
    /// Standard deviation of the filled x distribution (ROOT `GetStdDev`).
    pub fn std_dev(&self) -> f64 {
        std_dev_axis(self.tsumw, self.tsumwx, self.tsumwx2)
    }
    /// Center of bin `bin` (1-based).
    pub fn bin_center(&self, bin: usize) -> f64 {
        self.xaxis.bin_center(bin)
    }
    /// Bin holding `x` (0 = underflow, `1..=nbins`, `nbins+1` = overflow).
    pub fn find_bin(&self, x: f64) -> usize {
        self.xaxis.find_bin(x)
    }
    /// Clear all per-bin sums, entries, and moment sums, keeping the binning.
    pub fn reset(&mut self) {
        for arr in [
            &mut self.sums,
            &mut self.sumy2,
            &mut self.bin_entries,
            &mut self.bin_sumw2,
        ] {
            arr.iter_mut().for_each(|v| *v = 0.0);
        }
        self.entries = 0.0;
        for v in [
            &mut self.tsumw,
            &mut self.tsumw2,
            &mut self.tsumwx,
            &mut self.tsumwx2,
            &mut self.tsumwy,
            &mut self.tsumwy2,
        ] {
            *v = 0.0;
        }
    }
}
