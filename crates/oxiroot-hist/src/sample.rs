//! Histogram/function sampling and smoothing: draw random values from a
//! histogram's or a function's distribution (`TH1::GetRandom` / `FillRandom`,
//! `TF1::GetRandom`) and smooth a histogram (`TH1::Smooth`).
//!
//! Sampling needs a uniform random source. oxiroot has no global RNG (no
//! `gRandom`), so a small dependency-free [`Random`] is provided; seed it for
//! reproducible draws.
//!
//! ```
//! use oxiroot_hist::{Hist, Random};
//! let mut src = Hist::reg(100, -5.0, 5.0).double();
//! for x in [-1.0, 0.0, 0.0, 1.0, 2.0] { src.fill(x); }
//! let mut rng = Random::seed(1);
//! let mut drawn = Hist::reg(100, -5.0, 5.0).double();
//! drawn.fill_random(&src, 10_000, &mut rng); // draw 10k from src's shape
//! assert!((drawn.mean() - src.mean()).abs() < 0.1);
//! ```

use crate::tf::TF1;
use crate::th1::TH1;

/// A small seedable pseudo-random generator (SplitMix64), yielding `f64` in
/// `[0, 1)` — oxiroot's `TRandom`. Dependency-free and reproducible — seed it
/// and the draws repeat.
#[doc(
    alias = "TRandom",
    alias = "gRandom",
    alias = "Rng",
    alias = "SplitMix64"
)]
#[derive(Debug, Clone)]
pub struct Random {
    state: u64,
}

impl Random {
    /// A generator seeded with `seed` (any value, including 0).
    #[must_use]
    pub fn seed(seed: u64) -> Random {
        Random { state: seed }
    }

    /// The next uniform `f64` in `[0, 1)`.
    pub fn uniform(&mut self) -> f64 {
        // SplitMix64.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Top 53 bits → a double in [0, 1).
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Default for Random {
    fn default() -> Random {
        Random::seed(0)
    }
}

/// The normalized cumulative of `weights` (one entry per bin): `cdf[0] = 0`,
/// `cdf[k] = Σ weights[..k] / total`, length `weights.len() + 1`. `None` if the
/// total is not positive (nothing to sample).
fn build_cdf(weights: &[f64]) -> Option<Vec<f64>> {
    let mut cdf = vec![0.0; weights.len() + 1];
    let mut acc = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        acc += w;
        cdf[i + 1] = acc;
    }
    if acc <= 0.0 {
        return None;
    }
    for c in &mut cdf {
        *c /= acc;
    }
    Some(cdf)
}

/// Inverse-transform sample: given a normalized `cdf` (length `nbins + 1`) and
/// the `nbins + 1` bin `edges`, map a uniform `r ∈ [0, 1)` to an `x`, linearly
/// interpolating within the chosen bin (ROOT's `GetRandom`).
fn sample_cdf(cdf: &[f64], edges: &[f64], r: f64) -> f64 {
    let nbins = cdf.len() - 1;
    // First index k with cdf[k] > r (so the drawn bin is k, 1..=nbins).
    let mut lo = 1;
    let mut hi = nbins;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if cdf[mid] <= r {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let k = lo;
    let (c0, c1) = (cdf[k - 1], cdf[k]);
    let low = edges[k - 1];
    let width = edges[k] - edges[k - 1];
    if c1 > c0 {
        low + (r - c0) / (c1 - c0) * width
    } else {
        low + 0.5 * width
    }
}

impl TH1 {
    /// Draw a random `x` from the histogram's distribution (ROOT's `GetRandom`):
    /// the bin contents define the density, a bin is chosen with probability
    /// proportional to its content, and `x` is interpolated within it. Returns
    /// the axis lower edge for an empty histogram.
    ///
    /// For many draws, prefer [`fill_random`](TH1::fill_random) (it builds the
    /// cumulative once).
    #[must_use]
    pub fn get_random(&self, rng: &mut Random) -> f64 {
        let edges = self.xaxis.edges();
        match build_cdf(self.values()) {
            Some(cdf) => sample_cdf(&cdf, &edges, rng.uniform()),
            None => edges.first().copied().unwrap_or(0.0),
        }
    }

    /// Fill this histogram with `n` values drawn from `source`'s distribution
    /// (ROOT's `FillRandom(TH1*, n)`). Efficient: the cumulative is built once.
    pub fn fill_random(&mut self, source: &TH1, n: usize, rng: &mut Random) {
        let edges = source.xaxis.edges();
        if let Some(cdf) = build_cdf(source.values()) {
            for _ in 0..n {
                self.fill(sample_cdf(&cdf, &edges, rng.uniform()));
            }
        }
    }

    /// Fill this histogram with `n` values drawn from a function `f` over this
    /// histogram's range (ROOT's `FillRandom(TF1*, n)`): `f` is sampled at the
    /// bin centres to form the density.
    pub fn fill_random_fn<F: Fn(f64) -> f64>(&mut self, f: F, n: usize, rng: &mut Random) {
        let nbins = self.xaxis.nbins.max(0) as usize;
        let weights: Vec<f64> = (1..=nbins)
            .map(|b| f(self.xaxis.bin_center(b)).max(0.0))
            .collect();
        let edges = self.xaxis.edges();
        if let Some(cdf) = build_cdf(&weights) {
            for _ in 0..n {
                self.fill(sample_cdf(&cdf, &edges, rng.uniform()));
            }
        }
    }

    /// Smooth the in-range bin contents `ntimes` with ROOT's `353QH, twice`
    /// algorithm (`TH1::Smooth`). A no-op for fewer than 3 bins.
    pub fn smooth(&mut self, ntimes: usize) {
        let nbins = self.xaxis.nbins.max(0) as usize;
        if nbins < 3 {
            return;
        }
        let mut vals: Vec<f64> = self.values().to_vec();
        smooth_array(&mut vals, ntimes);
        for (i, v) in vals.into_iter().enumerate() {
            self.contents[i + 1] = v;
        }
    }
}

impl TF1 {
    /// Draw a random `x` from the function's distribution over its range (ROOT's
    /// `TF1::GetRandom`): the function is sampled on a fine grid to build a
    /// cumulative, then inverse-transform sampled. Assumes `f ≥ 0` on the range.
    #[must_use]
    pub fn get_random(&self, rng: &mut Random) -> f64 {
        const NPX: usize = 200;
        let (xmin, xmax) = self.range();
        let dx = (xmax - xmin) / NPX as f64;
        let weights: Vec<f64> = (0..NPX)
            .map(|i| self.eval(xmin + (i as f64 + 0.5) * dx).max(0.0))
            .collect();
        let edges: Vec<f64> = (0..=NPX).map(|i| xmin + i as f64 * dx).collect();
        match build_cdf(&weights) {
            Some(cdf) => sample_cdf(&cdf, &edges, rng.uniform()),
            None => xmin,
        }
    }
}

/// Median of the first `n` elements of `hh` (`n` ≤ 5).
fn median(n: usize, hh: &[f64]) -> f64 {
    let mut v: [f64; 5] = [0.0; 5];
    v[..n].copy_from_slice(&hh[..n]);
    v[..n].sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[n / 2]
}

/// ROOT's `TH1::SmoothArray` — the `353QH, twice` smoother, applied `ntimes`.
/// Ported faithfully (running medians 3/5/3, quadratic interpolation on flat
/// segments, a Hanning running mean, then a twice pass over the residuals).
fn smooth_array(xx: &mut [f64], ntimes: usize) {
    let nn = xx.len();
    if nn < 3 {
        return;
    }
    let mut hh = [0.0f64; 6];
    let mut yy = vec![0.0f64; nn];
    let mut zz = vec![0.0f64; nn];
    let mut rr = vec![0.0f64; nn];

    for _pass in 0..ntimes {
        yy.copy_from_slice(xx);

        for noent in 0..2 {
            // 353: running medians of window 3, 5, 3.
            for kk in 0..3 {
                zz.copy_from_slice(&yy);
                let median_type = if kk != 1 { 3 } else { 5 };
                let ifirst = if kk != 1 { 1 } else { 2 };
                let ilast = if kk != 1 { nn - 1 } else { nn - 2 };
                for ii in ifirst..ilast {
                    for jj in 0..median_type {
                        hh[jj] = yy[ii - ifirst + jj];
                    }
                    zz[ii] = median(median_type, &hh);
                }

                if kk == 0 {
                    // Endpoints for the median-3 pass.
                    hh[0] = zz[1];
                    hh[1] = zz[0];
                    hh[2] = 3.0 * zz[1] - 2.0 * zz[2];
                    zz[0] = median(3, &hh);
                    hh[0] = zz[nn - 2];
                    hh[1] = zz[nn - 1];
                    hh[2] = 3.0 * zz[nn - 2] - 2.0 * zz[nn - 3];
                    zz[nn - 1] = median(3, &hh);
                }
                if kk == 1 {
                    // Near-endpoints for the median-5 pass.
                    hh[..3].copy_from_slice(&yy[..3]);
                    zz[1] = median(3, &hh);
                    hh[..3].copy_from_slice(&yy[nn - 3..nn]);
                    zz[nn - 2] = median(3, &hh);
                }
                yy.copy_from_slice(&zz);
            }

            // Quadratic interpolation over flat 3-bin segments.
            zz.copy_from_slice(&yy);
            for ii in 2..nn - 2 {
                if zz[ii - 1] != zz[ii] || zz[ii] != zz[ii + 1] {
                    continue;
                }
                hh[0] = zz[ii - 2] - zz[ii];
                hh[1] = zz[ii + 2] - zz[ii];
                if hh[0] * hh[1] <= 0.0 {
                    continue;
                }
                let jk: isize = if hh[1].abs() > hh[0].abs() { -1 } else { 1 };
                let i2 = (ii as isize + 2 * jk) as usize;
                let im2 = (ii as isize - 2 * jk) as usize;
                let ijk = (ii as isize + jk) as usize;
                yy[ii] = -0.5 * zz[im2] + zz[ii] / 0.75 + zz[i2] / 6.0;
                yy[ijk] = 0.5 * (zz[i2] - zz[im2]) + zz[ii];
            }

            // Hanning running mean.
            for ii in 1..nn - 1 {
                zz[ii] = 0.25 * yy[ii - 1] + 0.5 * yy[ii] + 0.25 * yy[ii + 1];
            }
            zz[0] = yy[0];
            zz[nn - 1] = yy[nn - 1];

            if noent == 0 {
                // Keep the smoothed values, then smooth the residuals next round.
                rr.copy_from_slice(&zz);
                for ii in 0..nn {
                    zz[ii] = xx[ii] - zz[ii];
                }
                yy.copy_from_slice(&zz);
            }
        }

        let xmin = xx.iter().copied().fold(f64::INFINITY, f64::min);
        for ii in 0..nn {
            xx[ii] = if xmin < 0.0 {
                rr[ii] + zz[ii]
            } else {
                (rr[ii] + zz[ii]).max(0.0)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick::Hist;

    #[test]
    fn smooth_matches_root() {
        // ROOT: TH1D(10,0,10) contents [2,8,4,9,3,7,1,6,5,2]; Smooth(1) →
        // [2, 3.5, 4, 4.25, 4.75, 5, 5, 5, 5, 5].
        let mut h = Hist::reg(10, 0.0, 10.0).double();
        for (i, &v) in [2.0, 8.0, 4.0, 9.0, 3.0, 7.0, 1.0, 6.0, 5.0, 2.0]
            .iter()
            .enumerate()
        {
            h.contents[i + 1] = v;
        }
        h.smooth(1);
        let expected = [2.0, 3.5, 4.0, 4.25, 4.75, 5.0, 5.0, 5.0, 5.0, 5.0];
        for (got, want) in h.values().iter().zip(&expected) {
            assert!((got - want).abs() < 1e-9, "smooth: got {got}, want {want}");
        }
    }

    #[test]
    fn get_random_reproduces_the_source_distribution() {
        // A source peaked at 5 with width ~1; drawing from it recovers the shape.
        let mut src = Hist::reg(100, 0.0, 10.0).double();
        src.fill_random_fn(
            |x| (-0.5 * (x - 5.0).powi(2)).exp(),
            400_000,
            &mut Random::seed(7),
        );
        assert!((src.mean() - 5.0).abs() < 0.02, "mean {}", src.mean());
        assert!((src.std_dev() - 1.0).abs() < 0.02, "std {}", src.std_dev());

        let mut drawn = Hist::reg(100, 0.0, 10.0).double();
        drawn.fill_random(&src, 400_000, &mut Random::seed(11));
        assert!((drawn.mean() - src.mean()).abs() < 0.03);
        assert!((drawn.std_dev() - src.std_dev()).abs() < 0.03);
    }

    #[test]
    fn tf1_get_random_matches_the_function() {
        // ROOT: TF1 gaus mean 3 sigma 0.8 → GetRandom mean≈3.00, std≈0.80.
        let f = TF1::new("g", "gaus", 0.0, 10.0)
            .unwrap()
            .with_params(vec![1.0, 3.0, 0.8]);
        let mut rng = Random::seed(3);
        let (mut s, mut s2) = (0.0, 0.0);
        let n = 300_000;
        for _ in 0..n {
            let x = f.get_random(&mut rng);
            s += x;
            s2 += x * x;
        }
        let mean = s / n as f64;
        let std = (s2 / n as f64 - mean * mean).sqrt();
        assert!((mean - 3.0).abs() < 0.02, "mean {mean}");
        assert!((std - 0.8).abs() < 0.02, "std {std}");
    }

    #[test]
    fn rng_is_uniform_and_reproducible() {
        let mut a = Random::seed(42);
        let mut b = Random::seed(42);
        let mut sum = 0.0;
        for _ in 0..100_000 {
            let x = a.uniform();
            assert_eq!(x, b.uniform()); // same seed → same stream
            assert!((0.0..1.0).contains(&x));
            sum += x;
        }
        assert!((sum / 100_000.0 - 0.5).abs() < 0.01); // mean ≈ 0.5
    }
}
