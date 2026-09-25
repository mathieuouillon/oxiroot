//! Sampling from a function's distribution (`Func1D::GetRandom`).

use oxiroot_hist::Random;

use crate::func::Func1D;

impl Func1D {
    /// Draw a random `x` from the function's distribution over its range (ROOT's
    /// `Func1D::GetRandom`): the function is sampled on a fine grid to build a
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
        rng.sample_binned(&weights, &edges).unwrap_or(xmin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tf1_get_random_matches_the_function() {
        // ROOT: Func1D gaus mean 3 sigma 0.8 → GetRandom mean≈3.00, std≈0.80.
        let f = Func1D::new("g", "gaus", 0.0, 10.0)
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
}
