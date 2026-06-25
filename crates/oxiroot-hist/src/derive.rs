//! Histograms derived from other histograms — ROOT's `Rebin`, `ProjectionX/Y`,
//! `ProfileX/Y`, and `GetCumulative`. Each returns an existing histogram type
//! (`TH1`/`TProfile`), so the results serialize through the normal write paths
//! with no new on-disk format.
//!
//! Two correctness rules are baked into every operation here:
//! - per-bin `Sumw2` aggregates as a **sum of variances** (`Σ sumw2[src]`), never
//!   a `sqrt`; and it is carried over only when the source tracks it;
//! - the result's statistical **moment sums** are set (not just the contents), so
//!   `mean()`/`std_dev()` on the derived histogram are correct.

use crate::{TProfile, TH1, TH2};

impl TH1 {
    /// Merge groups of `ngroup` adjacent bins into one (ROOT's `Rebin`). If
    /// `ngroup` does not divide the bin count, the leftover high bins fold into
    /// the overflow (as ROOT does). Contents and `Sumw2` sum per group; the
    /// statistical moments are unchanged (same fills, coarser bins).
    pub fn rebin(&self, ngroup: usize) -> TH1 {
        let ng = ngroup.max(1);
        let n = self.xaxis.nbins.max(0) as usize;
        let newn = n / ng;
        let edges = self.xaxis.edges();
        // New in-range edges: every ng-th edge, up to the last full group.
        let new_edges: Vec<f64> = (0..=newn).map(|k| edges[k * ng]).collect();

        let mut out = if new_edges.len() >= 2 {
            TH1::new_variable(&self.name, &self.title, &new_edges)
        } else {
            TH1::new(
                &self.name,
                &self.title,
                newn.max(1) as i32,
                edges[0],
                edges[n],
            )
        };
        out.class_name = self.class_name.clone();
        let track = !self.sumw2.is_empty();
        if track {
            out.sumw2 = vec![0.0; out.contents.len()];
        }

        // Underflow carries over; group the in-range bins; leftover → overflow.
        out.contents[0] = self.contents[0];
        for nb in 1..=newn {
            let (lo, hi) = ((nb - 1) * ng + 1, nb * ng);
            out.contents[nb] = (lo..=hi).map(|i| self.contents[i]).sum();
            if track {
                out.sumw2[nb] = (lo..=hi).map(|i| self.sumw2[i]).sum();
            }
        }
        let over: f64 =
            self.contents[n + 1] + (newn * ng + 1..=n).map(|i| self.contents[i]).sum::<f64>();
        out.contents[newn + 1] = over;

        copy_th1_moments(self, &mut out);
        out
    }

    /// The cumulative histogram (ROOT's `GetCumulative`): bin `i` becomes the
    /// running sum of the in-range bins up to `i` (`forward`) or from `i` to the
    /// top (`!forward`). Binning and moment sums are preserved.
    pub fn cumulative(&self, forward: bool) -> TH1 {
        let n = self.xaxis.nbins.max(0) as usize;
        let mut out = self.clone();
        out.contents.iter_mut().for_each(|v| *v = 0.0);
        let mut acc = 0.0;
        if forward {
            for i in 1..=n {
                acc += self.contents[i];
                out.contents[i] = acc;
            }
        } else {
            for i in (1..=n).rev() {
                acc += self.contents[i];
                out.contents[i] = acc;
            }
        }
        // Errors of a running sum are not a simple per-bin copy; drop them.
        out.sumw2 = Vec::new();
        out
    }
}

/// Copy a `TH1`'s entry count and x-moment sums into another `TH1`.
fn copy_th1_moments(src: &TH1, dst: &mut TH1) {
    dst.entries = src.entries;
    dst.tsumw = src.tsumw;
    dst.tsumw2 = src.tsumw2;
    dst.tsumwx = src.tsumwx;
    dst.tsumwx2 = src.tsumwx2;
}

impl TH2 {
    /// Project onto the x axis by summing the in-range y bins (ROOT's
    /// `ProjectionX`) → a `TH1` with this histogram's x binning. The x-moment
    /// sums carry over so `mean()`/`std_dev()` are correct.
    pub fn projection_x(&self, name: &str) -> TH1 {
        self.project(name, true)
    }

    /// Project onto the y axis by summing the in-range x bins (ROOT's
    /// `ProjectionY`) → a `TH1` with this histogram's y binning.
    pub fn projection_y(&self, name: &str) -> TH1 {
        self.project(name, false)
    }

    fn project(&self, name: &str, onto_x: bool) -> TH1 {
        let (nx, ny) = (self.nx(), self.ny());
        let stride = nx + 2;
        let axis = if onto_x { &self.xaxis } else { &self.yaxis };
        let (n_keep, n_sum) = if onto_x { (nx, ny) } else { (ny, nx) };

        let mut out = TH1::new(name, &self.title, n_keep as i32, axis.xmin, axis.xmax);
        out.xaxis = axis.clone();
        let track = !self.sumw2.is_empty();
        if track {
            out.sumw2 = vec![0.0; out.contents.len()];
        }
        // Sum the other axis over its in-range bins, for every kept cell incl flow.
        let cell = |keep: usize, sum: usize| {
            if onto_x {
                keep + stride * sum
            } else {
                sum + stride * keep
            }
        };
        for k in 0..=n_keep + 1 {
            out.contents[k] = (1..=n_sum).map(|s| self.contents[cell(k, s)]).sum();
            if track {
                out.sumw2[k] = (1..=n_sum).map(|s| self.sumw2[cell(k, s)]).sum();
            }
        }
        out.entries = self.entries;
        out.tsumw = self.tsumw;
        out.tsumw2 = self.tsumw2;
        if onto_x {
            out.tsumwx = self.tsumwx;
            out.tsumwx2 = self.tsumwx2;
        } else {
            out.tsumwx = self.tsumwy;
            out.tsumwx2 = self.tsumwy2;
        }
        out
    }

    /// Profile along x (ROOT's `ProfileX`) → a `TProfile` with this histogram's x
    /// binning, accumulating each y bin at its center.
    pub fn profile_x(&self, name: &str) -> TProfile {
        self.profile(name, true)
    }

    /// Profile along y (ROOT's `ProfileY`) → a `TProfile` with this histogram's y
    /// binning.
    pub fn profile_y(&self, name: &str) -> TProfile {
        self.profile(name, false)
    }

    fn profile(&self, name: &str, along_x: bool) -> TProfile {
        let (nx, ny) = (self.nx(), self.ny());
        let stride = nx + 2;
        let keep_axis = if along_x { &self.xaxis } else { &self.yaxis };
        let other_axis = if along_x { &self.yaxis } else { &self.xaxis };
        let (n_keep, n_other) = if along_x { (nx, ny) } else { (ny, nx) };

        let mut p = TProfile::new(
            name,
            &self.title,
            n_keep as i32,
            keep_axis.xmin,
            keep_axis.xmax,
        );
        p.xaxis = keep_axis.clone();
        // Centers of the "other" (profiled-value) axis bins.
        let other_center: Vec<f64> = (0..=n_other + 1)
            .map(|i| other_axis.bin_center(i))
            .collect();
        let cell = |keep: usize, other: usize| {
            if along_x {
                keep + stride * other
            } else {
                other + stride * keep
            }
        };
        for k in 0..=n_keep + 1 {
            let (mut be, mut s, mut s2) = (0.0, 0.0, 0.0);
            // `o` indexes both the cell and the bin-center table.
            #[allow(clippy::needless_range_loop)]
            for o in 1..=n_other {
                let c = self.contents[cell(k, o)];
                let yc = other_center[o];
                be += c;
                s += c * yc;
                s2 += c * yc * yc;
            }
            p.bin_entries[k] = be;
            p.sums[k] = s;
            p.sumy2[k] = s2;
        }
        p.entries = self.entries;
        p.tsumw = self.tsumw;
        p.tsumw2 = self.tsumw2;
        if along_x {
            p.tsumwx = self.tsumwx;
            p.tsumwx2 = self.tsumwx2;
            p.tsumwy = self.tsumwy;
            p.tsumwy2 = self.tsumwy2;
        } else {
            p.tsumwx = self.tsumwy;
            p.tsumwx2 = self.tsumwy2;
            p.tsumwy = self.tsumwx;
            p.tsumwy2 = self.tsumwx2;
        }
        p
    }
}
