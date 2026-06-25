//! Histograms derived from other histograms — ROOT's `Rebin`/`Rebin2D`/`Rebin3D`,
//! `ProjectionX/Y/Z` & `Project3D`, `ProfileX/Y`, and `GetCumulative`. Each
//! returns an existing histogram type (`TH1`/`TH2`/`TH3`/`TProfile`), so the
//! results serialize through the normal write paths with no new on-disk format.
//!
//! Two correctness rules are baked into every operation here:
//! - per-bin `Sumw2` aggregates as a **sum of variances** (`Σ sumw2[src]`), never
//!   a `sqrt`; and it is carried over only when the source tracks it;
//! - the result's statistical **moment sums** are set (not just the contents), so
//!   `mean()`/`std_dev()` on the derived histogram are correct.

use crate::{TAxis, TProfile, TH1, TH2, TH3};

/// Per-axis rebin map: returns `(new_nbins, old_cell -> new_cell)` of length
/// `n + 2`. Underflow → underflow; in-range bin `i` → group `(i-1)/ng + 1`;
/// leftover high bins (when `ng` does not divide `n`) and the old overflow →
/// the new overflow.
fn rebin_map(n: usize, ng: usize) -> (usize, Vec<usize>) {
    let newn = (n / ng).max(1);
    let mut map = vec![0usize; n + 2];
    for (i, m) in map.iter_mut().enumerate().take(n + 1).skip(1) {
        *m = if i <= newn * ng {
            (i - 1) / ng + 1
        } else {
            newn + 1
        };
    }
    map[n + 1] = newn + 1;
    (newn, map)
}

/// New axis edges after grouping every `ng` bins (`newn + 1` edges).
fn group_edges(edges: &[f64], ng: usize, newn: usize) -> Vec<f64> {
    (0..=newn)
        .map(|k| edges[(k * ng).min(edges.len() - 1)])
        .collect()
}

/// Copy a `TH2`'s entry count and x/y moment sums into another `TH2`.
fn copy_th2_moments(src: &TH2, dst: &mut TH2) {
    dst.entries = src.entries;
    dst.tsumw = src.tsumw;
    dst.tsumw2 = src.tsumw2;
    dst.tsumwx = src.tsumwx;
    dst.tsumwx2 = src.tsumwx2;
    dst.tsumwy = src.tsumwy;
    dst.tsumwy2 = src.tsumwy2;
    dst.tsumwxy = src.tsumwxy;
}

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

    /// Merge `ngx`×`ngy` blocks of adjacent bins into one (ROOT's `Rebin2D`).
    /// Contents and `Sumw2` sum per block; the moment sums are unchanged.
    pub fn rebin2d(&self, ngx: usize, ngy: usize) -> TH2 {
        let (nx, ny) = (self.nx(), self.ny());
        let (ngx, ngy) = (ngx.clamp(1, nx.max(1)), ngy.clamp(1, ny.max(1)));
        let (newnx, mapx) = rebin_map(nx, ngx);
        let (newny, mapy) = rebin_map(ny, ngy);
        let xedges = group_edges(&self.xaxis.edges(), ngx, newnx);
        let yedges = group_edges(&self.yaxis.edges(), ngy, newny);

        let mut out = TH2::new_variable(&self.name, &self.title, &xedges, &yedges);
        out.class_name = self.class_name.clone();
        let track = !self.sumw2.is_empty();
        if track {
            out.sumw2 = vec![0.0; out.contents.len()];
        }
        let (old_s, new_s) = (nx + 2, newnx + 2);
        // Loop indices map old cells to new cells (and index the rebin maps).
        #[allow(clippy::needless_range_loop)]
        for iy in 0..=ny + 1 {
            for ix in 0..=nx + 1 {
                let (oc, nc) = (ix + old_s * iy, mapx[ix] + new_s * mapy[iy]);
                out.contents[nc] += self.contents[oc];
                if track {
                    out.sumw2[nc] += self.sumw2[oc];
                }
            }
        }
        copy_th2_moments(self, &mut out);
        out
    }
}

impl TH3 {
    /// Merge `ngx`×`ngy`×`ngz` blocks of adjacent bins into one. Contents and
    /// `Sumw2` sum per block; the moment sums are unchanged.
    pub fn rebin3d(&self, ngx: usize, ngy: usize, ngz: usize) -> TH3 {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let (ngx, ngy, ngz) = (
            ngx.clamp(1, nx.max(1)),
            ngy.clamp(1, ny.max(1)),
            ngz.clamp(1, nz.max(1)),
        );
        let (newnx, mapx) = rebin_map(nx, ngx);
        let (newny, mapy) = rebin_map(ny, ngy);
        let (newnz, mapz) = rebin_map(nz, ngz);

        // TH3 has no variable-bin constructor; build it and set the axes.
        let mut out = TH3::new(
            &self.name,
            &self.title,
            newnx as i32,
            0.0,
            1.0,
            newny as i32,
            0.0,
            1.0,
            newnz as i32,
            0.0,
            1.0,
        );
        out.class_name = self.class_name.clone();
        out.xaxis = TAxis::variable("xaxis", &group_edges(&self.xaxis.edges(), ngx, newnx));
        out.yaxis = TAxis::variable("yaxis", &group_edges(&self.yaxis.edges(), ngy, newny));
        out.zaxis = TAxis::variable("zaxis", &group_edges(&self.zaxis.edges(), ngz, newnz));
        let track = !self.sumw2.is_empty();
        if track {
            out.sumw2 = vec![0.0; out.contents.len()];
        }
        let (osx, osy) = (nx + 2, ny + 2);
        let (nsx, nsy) = (newnx + 2, newny + 2);
        // Loop indices map old cells to new cells (and index the rebin maps).
        #[allow(clippy::needless_range_loop)]
        for iz in 0..=nz + 1 {
            for iy in 0..=ny + 1 {
                for ix in 0..=nx + 1 {
                    let oc = ix + osx * (iy + osy * iz);
                    let nc = mapx[ix] + nsx * (mapy[iy] + nsy * mapz[iz]);
                    out.contents[nc] += self.contents[oc];
                    if track {
                        out.sumw2[nc] += self.sumw2[oc];
                    }
                }
            }
        }
        out.entries = self.entries;
        for (d, s) in [
            (&mut out.tsumw, self.tsumw),
            (&mut out.tsumw2, self.tsumw2),
            (&mut out.tsumwx, self.tsumwx),
            (&mut out.tsumwx2, self.tsumwx2),
            (&mut out.tsumwy, self.tsumwy),
            (&mut out.tsumwy2, self.tsumwy2),
            (&mut out.tsumwxy, self.tsumwxy),
            (&mut out.tsumwz, self.tsumwz),
            (&mut out.tsumwz2, self.tsumwz2),
            (&mut out.tsumwxz, self.tsumwxz),
            (&mut out.tsumwyz, self.tsumwyz),
        ] {
            *d = s;
        }
        out
    }

    /// Project onto the x axis, summing the in-range y and z bins (ROOT's
    /// `Project3D("x")`) → a `TH1` carrying the x-moment sums.
    pub fn projection_x(&self, name: &str) -> TH1 {
        self.project_axis(name, 0)
    }
    /// Project onto the y axis (sum x, z) → a `TH1`.
    pub fn projection_y(&self, name: &str) -> TH1 {
        self.project_axis(name, 1)
    }
    /// Project onto the z axis (sum x, y) → a `TH1`.
    pub fn projection_z(&self, name: &str) -> TH1 {
        self.project_axis(name, 2)
    }

    fn project_axis(&self, name: &str, keep: u8) -> TH1 {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let (sx, sy) = (nx + 2, ny + 2);
        let cell = |ix: usize, iy: usize, iz: usize| ix + sx * (iy + sy * iz);
        let (axis, nkeep) = match keep {
            0 => (&self.xaxis, nx),
            1 => (&self.yaxis, ny),
            _ => (&self.zaxis, nz),
        };
        let mut out = TH1::new(name, &self.title, nkeep as i32, axis.xmin, axis.xmax);
        out.xaxis = axis.clone();
        let track = !self.sumw2.is_empty();
        if track {
            out.sumw2 = vec![0.0; out.contents.len()];
        }
        let (an, bn) = match keep {
            0 => (ny, nz),
            1 => (nx, nz),
            _ => (nx, ny),
        };
        for k in 0..=nkeep + 1 {
            let (mut c, mut w) = (0.0, 0.0);
            for a in 1..=an {
                for b in 1..=bn {
                    let idx = match keep {
                        0 => cell(k, a, b),
                        1 => cell(a, k, b),
                        _ => cell(a, b, k),
                    };
                    c += self.contents[idx];
                    if track {
                        w += self.sumw2[idx];
                    }
                }
            }
            out.contents[k] = c;
            if track {
                out.sumw2[k] = w;
            }
        }
        out.entries = self.entries;
        out.tsumw = self.tsumw;
        out.tsumw2 = self.tsumw2;
        (out.tsumwx, out.tsumwx2) = match keep {
            0 => (self.tsumwx, self.tsumwx2),
            1 => (self.tsumwy, self.tsumwy2),
            _ => (self.tsumwz, self.tsumwz2),
        };
        out
    }

    /// Project onto the x–y plane, summing the in-range z bins → a `TH2`.
    pub fn projection_xy(&self, name: &str) -> TH2 {
        self.project_plane(name, 2)
    }
    /// Project onto the x–z plane (sum y) → a `TH2`.
    pub fn projection_xz(&self, name: &str) -> TH2 {
        self.project_plane(name, 1)
    }
    /// Project onto the y–z plane (sum x) → a `TH2`.
    pub fn projection_yz(&self, name: &str) -> TH2 {
        self.project_plane(name, 0)
    }

    fn project_plane(&self, name: &str, drop: u8) -> TH2 {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let (sx, sy) = (nx + 2, ny + 2);
        let cell = |ix: usize, iy: usize, iz: usize| ix + sx * (iy + sy * iz);
        // (kept axis a, kept axis b, summed axis size)
        let (axa, na, axb, nb, sumn) = match drop {
            2 => (&self.xaxis, nx, &self.yaxis, ny, nz),
            1 => (&self.xaxis, nx, &self.zaxis, nz, ny),
            _ => (&self.yaxis, ny, &self.zaxis, nz, nx),
        };
        let mut out = TH2::new(
            name,
            &self.title,
            na as i32,
            axa.xmin,
            axa.xmax,
            nb as i32,
            axb.xmin,
            axb.xmax,
        );
        out.xaxis = axa.clone();
        out.yaxis = axb.clone();
        let track = !self.sumw2.is_empty();
        if track {
            out.sumw2 = vec![0.0; out.contents.len()];
        }
        let out_s = na + 2;
        for ib in 0..=nb + 1 {
            for ia in 0..=na + 1 {
                let (mut c, mut w) = (0.0, 0.0);
                for s in 1..=sumn {
                    let idx = match drop {
                        2 => cell(ia, ib, s),
                        1 => cell(ia, s, ib),
                        _ => cell(s, ia, ib),
                    };
                    c += self.contents[idx];
                    if track {
                        w += self.sumw2[idx];
                    }
                }
                out.contents[ia + out_s * ib] = c;
                if track {
                    out.sumw2[ia + out_s * ib] = w;
                }
            }
        }
        out.entries = self.entries;
        out.tsumw = self.tsumw;
        out.tsumw2 = self.tsumw2;
        (
            out.tsumwx,
            out.tsumwx2,
            out.tsumwy,
            out.tsumwy2,
            out.tsumwxy,
        ) = match drop {
            2 => (
                self.tsumwx,
                self.tsumwx2,
                self.tsumwy,
                self.tsumwy2,
                self.tsumwxy,
            ),
            1 => (
                self.tsumwx,
                self.tsumwx2,
                self.tsumwz,
                self.tsumwz2,
                self.tsumwxz,
            ),
            _ => (
                self.tsumwy,
                self.tsumwy2,
                self.tsumwz,
                self.tsumwz2,
                self.tsumwyz,
            ),
        };
        out
    }
}
