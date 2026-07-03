//! Descriptive statistics over a sample (`&[f64]`), matching `scipy.stats`
//! conventions (population moments unless noted; `iqr`/`median` use linear
//! interpolation like NumPy). Empty input yields `NaN`.

/// Arithmetic mean, or `NaN` for an empty slice.
pub(crate) fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return f64::NAN;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

/// The `order`-th central moment `E[(x - mean)^order]` — `scipy.stats.moment`.
/// Order 0 is 1, order 1 is 0.
#[must_use]
pub fn moment(data: &[f64], order: i32) -> f64 {
    if data.is_empty() {
        return f64::NAN;
    }
    if order == 0 {
        return 1.0;
    }
    let m = mean(data);
    data.iter().map(|&x| (x - m).powi(order)).sum::<f64>() / data.len() as f64
}

/// Geometric mean `exp(mean(ln x))` — `scipy.stats.gmean` (requires `x > 0`).
#[must_use]
pub fn gmean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return f64::NAN;
    }
    (data.iter().map(|&x| x.ln()).sum::<f64>() / data.len() as f64).exp()
}

/// Harmonic mean `n / Σ(1/x)` — `scipy.stats.hmean` (requires `x > 0`).
#[must_use]
pub fn hmean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return f64::NAN;
    }
    data.len() as f64 / data.iter().map(|&x| 1.0 / x).sum::<f64>()
}

/// Sample skewness (Fisher–Pearson) — `scipy.stats.skew`. `bias = true` is the
/// biased `g1 = m3 / m2^1.5`; `bias = false` applies the sample correction.
#[must_use]
pub fn skew(data: &[f64], bias: bool) -> f64 {
    let m2 = moment(data, 2);
    let m3 = moment(data, 3);
    if m2 == 0.0 {
        return 0.0;
    }
    let g1 = m3 / m2.powf(1.5);
    if bias {
        g1
    } else {
        let n = data.len() as f64;
        g1 * (n * (n - 1.0)).sqrt() / (n - 2.0)
    }
}

/// Sample kurtosis — `scipy.stats.kurtosis`. `fisher = true` subtracts 3 (so a
/// normal distribution is 0); `bias = false` applies the sample correction.
#[must_use]
pub fn kurtosis(data: &[f64], fisher: bool, bias: bool) -> f64 {
    let m2 = moment(data, 2);
    let m4 = moment(data, 4);
    if m2 == 0.0 {
        return 0.0;
    }
    let raw = m4 / (m2 * m2); // Pearson, biased
    let excess = if bias {
        raw - 3.0
    } else {
        let n = data.len() as f64;
        (1.0 / ((n - 2.0) * (n - 3.0))) * ((n * n - 1.0) * raw - 3.0 * (n - 1.0).powi(2))
    };
    if fisher {
        excess
    } else {
        excess + 3.0
    }
}

/// Coefficient of variation `std / mean` (population std, `ddof = 0`) —
/// `scipy.stats.variation`.
#[must_use]
pub fn variation(data: &[f64]) -> f64 {
    std_dev(data, 0) / mean(data)
}

/// Standard error of the mean `std(ddof=1) / √n` — `scipy.stats.sem`.
#[must_use]
pub fn sem(data: &[f64]) -> f64 {
    std_dev(data, 1) / (data.len() as f64).sqrt()
}

/// Standard deviation with `ddof` degrees of freedom removed.
pub(crate) fn std_dev(data: &[f64], ddof: usize) -> f64 {
    let n = data.len();
    if n <= ddof {
        return f64::NAN;
    }
    let m = mean(data);
    let ss: f64 = data.iter().map(|&x| (x - m) * (x - m)).sum();
    (ss / (n - ddof) as f64).sqrt()
}

/// The `q`-th quantile (`q ∈ [0, 1]`) with linear interpolation (NumPy's
/// default). Sorts a copy of `data`.
#[must_use]
pub fn quantile(data: &[f64], q: f64) -> f64 {
    if data.is_empty() {
        return f64::NAN;
    }
    let mut s = data.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n == 1 {
        return s[0];
    }
    let pos = (n as f64 - 1.0) * q.clamp(0.0, 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    s[lo] + (s[hi] - s[lo]) * (pos - lo as f64)
}

/// The median — `numpy.median` (linear interpolation for even `n`).
#[must_use]
pub fn median(data: &[f64]) -> f64 {
    quantile(data, 0.5)
}

/// Interquartile range `Q3 − Q1` — `scipy.stats.iqr`.
#[must_use]
pub fn iqr(data: &[f64]) -> f64 {
    quantile(data, 0.75) - quantile(data, 0.25)
}

/// Median absolute deviation `median(|x − median(x)|)` —
/// `scipy.stats.median_abs_deviation`. `normal = true` rescales by
/// `1 / Φ⁻¹(3/4)` so it estimates the standard deviation of a normal sample.
#[must_use]
pub fn median_abs_deviation(data: &[f64], normal: bool) -> f64 {
    let med = median(data);
    let devs: Vec<f64> = data.iter().map(|&x| (x - med).abs()).collect();
    let mad = median(&devs);
    if normal {
        mad / 0.674_489_750_196_081_7
    } else {
        mad
    }
}

/// Shannon entropy (natural log) of `pk`, normalized to a distribution first —
/// `scipy.stats.entropy`. `H = −Σ pᵢ ln pᵢ`.
#[must_use]
pub fn entropy(pk: &[f64]) -> f64 {
    let total: f64 = pk.iter().sum();
    -pk.iter()
        .map(|&x| x / total)
        .filter(|&p| p > 0.0)
        .map(|p| p * p.ln())
        .sum::<f64>()
}

/// Kullback–Leibler divergence `Σ pᵢ ln(pᵢ/qᵢ)` (natural log) — `scipy.stats.entropy`
/// with a second argument; both are normalized first.
#[must_use]
pub fn kl_divergence(pk: &[f64], qk: &[f64]) -> f64 {
    let ptot: f64 = pk.iter().sum();
    let qtot: f64 = qk.iter().sum();
    pk.iter()
        .zip(qk.iter())
        .map(|(&p, &q)| (p / ptot, q / qtot))
        .filter(|&(p, _)| p > 0.0)
        .map(|(p, q)| p * (p / q).ln())
        .sum()
}

/// Per-element z-scores `(x − mean) / std` (population std, `ddof = 0`) —
/// `scipy.stats.zscore`.
#[must_use]
pub fn zscore(data: &[f64]) -> Vec<f64> {
    let m = mean(data);
    let s = std_dev(data, 0);
    data.iter().map(|&x| (x - m) / s).collect()
}

/// A one-shot descriptive summary — the fields of `scipy.stats.describe`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Describe {
    /// Number of observations.
    pub nobs: usize,
    /// Minimum value.
    pub min: f64,
    /// Maximum value.
    pub max: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// Unbiased (sample, `ddof = 1`) variance.
    pub variance: f64,
    /// Biased Fisher–Pearson skewness (`g1`).
    pub skewness: f64,
    /// Biased Fisher (excess) kurtosis.
    pub kurtosis: f64,
}

/// Compute a descriptive summary in one pass — `scipy.stats.describe` (variance
/// uses `ddof = 1`; skewness and kurtosis are the biased Fisher estimators).
#[must_use]
pub fn describe(data: &[f64]) -> Describe {
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for &x in data {
        min = min.min(x);
        max = max.max(x);
    }
    Describe {
        nobs: data.len(),
        min,
        max,
        mean: mean(data),
        variance: std_dev(data, 1).powi(2),
        skewness: skew(data, true),
        kurtosis: kurtosis(data, true, true),
    }
}

/// Ranks of the data, ties assigned their average rank (1-based) —
/// `scipy.stats.rankdata` (`method='average'`).
#[must_use]
pub fn rankdata(data: &[f64]) -> Vec<f64> {
    let n = data.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| data[i].total_cmp(&data[j]));
    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && data[order[j + 1]] == data[order[i]] {
            j += 1;
        }
        // Ranks i+1..=j+1 are tied; assign their average.
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for &o in &order[i..=j] {
            ranks[o] = avg;
        }
        i = j + 1;
    }
    ranks
}
