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
