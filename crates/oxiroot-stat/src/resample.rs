//! Bootstrap resampling with a small, dependency-free PRNG.

use crate::descriptive::quantile;

/// SplitMix64 — a tiny, fast PRNG so resampling is reproducible without a `rand`
/// dependency.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// A uniform index into `0..n`.
    fn index(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Percentile-bootstrap confidence interval for an arbitrary statistic —
/// `scipy.stats.bootstrap` with `method='percentile'`. Draws `n_resamples`
/// resamples of `data` (with replacement), evaluates `statistic` on each, and
/// returns the central `cl` percentile interval of those values. `seed` makes
/// the draw reproducible.
#[must_use]
pub fn bootstrap_ci(
    data: &[f64],
    statistic: impl Fn(&[f64]) -> f64,
    n_resamples: usize,
    cl: f64,
    seed: u64,
) -> (f64, f64) {
    let n = data.len();
    if n == 0 || n_resamples == 0 {
        return (f64::NAN, f64::NAN);
    }
    let mut rng = SplitMix64::new(seed);
    let mut sample = vec![0.0; n];
    let mut stats: Vec<f64> = Vec::with_capacity(n_resamples);
    for _ in 0..n_resamples {
        for s in sample.iter_mut() {
            *s = data[rng.index(n)];
        }
        stats.push(statistic(&sample));
    }
    let alpha = 1.0 - cl;
    (
        quantile(&stats, alpha / 2.0),
        quantile(&stats, 1.0 - alpha / 2.0),
    )
}
