//! Histogram fitting (`fit` feature). Run: `cargo test -p oxiroot-hist --features fit`.
#![cfg(feature = "fit")]

use std::path::PathBuf;

use oxiroot_hist::{read_th1d, TF1, TH1};
use oxiroot_io_core::RFile;

fn read(name: &str) -> TH1 {
    let f =
        RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/analysis.root"))
            .expect("open");
    read_th1d(&f, name).expect("read")
}

fn rel_close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * (1.0 + b.abs())
}

#[test]
fn gaussian_fit_matches_root() {
    let hg = read("hg"); // const=1000, mean=0.5, sigma=1.3 gaussian (integer bins)
                         // The fixture sets bin contents directly (no fills), so its moment sums are
                         // zero; estimate the initial mean/sigma from the contents instead.
    let n = hg.xaxis.nbins.max(0) as usize;
    let total: f64 = (1..=n).map(|i| hg.contents[i]).sum();
    let mean: f64 = (1..=n)
        .map(|i| hg.bin_center(i) * hg.contents[i])
        .sum::<f64>()
        / total;
    let var: f64 = (1..=n)
        .map(|i| hg.contents[i] * (hg.bin_center(i) - mean).powi(2))
        .sum::<f64>()
        / total;
    let model = TF1::gaussian("g").with_params(vec![hg.maximum(), mean, var.sqrt()]);
    let r = hg.fit(&model);

    assert!(r.valid, "fit did not converge");
    // ROOT's `hg.Fit("gaus")` ground truth (the Rust and ROOT Minuit2 ports agree
    // to a few parts in 1e3 on this integer-rounded data).
    assert!(
        rel_close(r.params[0], 1000.827893, 5e-3),
        "const = {}",
        r.params[0]
    );
    assert!(
        rel_close(r.params[1], 0.498833, 5e-3),
        "mean = {}",
        r.params[1]
    );
    assert!(
        rel_close(r.params[2], 1.297287, 5e-3),
        "sigma = {}",
        r.params[2]
    );
    // Uncertainties are reported and positive.
    assert!(r.errors.iter().all(|&e| e > 0.0 && e.is_finite()));
    assert!(r.ndf > 0);
}

#[test]
fn polynomial_fit_recovers_a_line() {
    // y = 3 + 2x exactly on a fine grid -> a degree-1 fit recovers (3, 2).
    let mut h = TH1::new("line", "", 100, 0.0, 10.0);
    for i in 1..=100 {
        let x = h.bin_center(i);
        h.contents[i] = 3.0 + 2.0 * x;
    }
    h.sumw2(); // give every bin a unit-ish error so all enter the fit
    for s in h.sumw2.iter_mut() {
        *s = 1.0;
    }
    let r = h.fit(&TF1::polynomial("pol1", 1).with_params(vec![0.0, 0.0]));
    assert!(r.valid);
    assert!(rel_close(r.params[0], 3.0, 1e-6), "p0 = {}", r.params[0]);
    assert!(rel_close(r.params[1], 2.0, 1e-6), "p1 = {}", r.params[1]);
    assert!(r.chi2 < 1e-6, "chi2 = {}", r.chi2);
}

#[test]
fn tf1_eval_uses_current_params() {
    let g = TF1::gaussian("g").with_params(vec![10.0, 0.0, 1.0]);
    assert!((g.eval(0.0) - 10.0).abs() < 1e-12); // peak
    assert!((g.eval(1.0) - 10.0 * (-0.5_f64).exp()).abs() < 1e-12);
}

#[test]
fn likelihood_fit_of_a_constant_equals_the_mean() {
    // The Poisson maximum-likelihood estimate of a constant rate is exactly the
    // mean of the bin counts; the chi-square estimate is a different (smaller)
    // value. This pins the likelihood path analytically and distinguishes it
    // from chi-square.
    use oxiroot_hist::FitMethod;
    let counts = [10.0, 20.0, 30.0, 40.0];
    let mut h = TH1::new("c", "", counts.len() as i32, 0.0, counts.len() as f64);
    for (i, &c) in counts.iter().enumerate() {
        h.contents[i + 1] = c;
    }
    let mean = counts.iter().sum::<f64>() / counts.len() as f64; // 25.0

    let like = h.fit_with(
        &TF1::polynomial("pol0", 0).with_params(vec![mean]),
        FitMethod::Likelihood,
    );
    let chi2 = h.fit_with(
        &TF1::polynomial("pol0", 0).with_params(vec![mean]),
        FitMethod::Chi2,
    );
    assert!(like.valid && chi2.valid);
    assert!(
        (like.params[0] - mean).abs() < 1e-4,
        "L const = {}",
        like.params[0]
    );
    assert!(
        (chi2.params[0] - mean).abs() > 1.0,
        "chi2 const = {}",
        chi2.params[0]
    );
}

#[test]
fn likelihood_gaussian_recovers_shape() {
    use oxiroot_hist::FitMethod;
    // hg is a high-statistics gaussian (const=1000, mean=0.5, sigma=1.3); the
    // Poisson likelihood fit recovers it (at high stats it agrees with chi-square).
    let hg = read("hg");
    let n = hg.xaxis.nbins.max(0) as usize;
    let total: f64 = (1..=n).map(|i| hg.contents[i]).sum();
    let mean: f64 = (1..=n)
        .map(|i| hg.bin_center(i) * hg.contents[i])
        .sum::<f64>()
        / total;
    let model = TF1::gaussian("g").with_params(vec![hg.maximum(), mean, 1.3]);
    let r = hg.fit_with(&model, FitMethod::Likelihood);
    assert!(r.valid);
    assert!(
        rel_close(r.params[0], 1000.0, 1e-2),
        "const = {}",
        r.params[0]
    );
    assert!(rel_close(r.params[1], 0.5, 2e-2), "mean = {}", r.params[1]);
    assert!(rel_close(r.params[2], 1.3, 1e-2), "sigma = {}", r.params[2]);
}

#[test]
fn likelihood_and_chi2_diverge_on_low_statistics() {
    use oxiroot_hist::FitMethod;
    // On a low-statistics, imperfect gaussian the two estimators genuinely
    // differ (where ROOT's "L" on a SetBinContent histogram falls back to chi2).
    let hgl = read("hgl");
    let n = hgl.xaxis.nbins.max(0) as usize;
    let total: f64 = (1..=n).map(|i| hgl.contents[i]).sum();
    let mean: f64 = (1..=n)
        .map(|i| hgl.bin_center(i) * hgl.contents[i])
        .sum::<f64>()
        / total;
    let model = || TF1::gaussian("g").with_params(vec![hgl.maximum(), mean, 1.3]);
    let chi2 = hgl.fit_with(&model(), FitMethod::Chi2);
    let like = hgl.fit_with(&model(), FitMethod::Likelihood);
    assert!(chi2.valid && like.valid);
    assert!(
        (chi2.params[0] - like.params[0]).abs() > 0.5,
        "chi2 const {} vs L const {} should differ",
        chi2.params[0],
        like.params[0]
    );
}
