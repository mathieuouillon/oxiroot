//! Fitting arbitrary data — no histogram or graph in sight, just `(x, y, σ)`.

use oxiroot_fit::{FitData, FitExt, FitMethod, FitOptions, Model, Point, Points};

fn rel_close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

#[test]
fn weighted_line_fit_recovers_slope_and_intercept() {
    // y = 3 + 2x exactly, tight errors.
    let x = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let y: Vec<f64> = x.iter().map(|&x| 3.0 + 2.0 * x).collect();
    let data = Points::new(&x, &y, &[0.05; 6]);

    let fit = data.fit(&Model::polynomial("line", 1).with_params(vec![0.0, 0.0]));
    assert!(fit.valid);
    assert!(
        rel_close(fit.params[0], 3.0, 1e-6),
        "intercept {}",
        fit.params[0]
    );
    assert!(
        rel_close(fit.params[1], 2.0, 1e-6),
        "slope {}",
        fit.params[1]
    );
    assert_eq!(fit.ndf, 4); // 6 points − 2 params
    assert!(fit.chi2 < 1e-6);
    assert!(fit.p_value() > 0.99); // a near-perfect fit
}

#[test]
fn unweighted_matches_a_slice_of_points() {
    // The `&[Point]` impl works directly, and `unweighted` (σ = 1) gives an OLS fit.
    let pts = [
        Point::new(0.0, 1.0, 1.0),
        Point::new(1.0, 3.0, 1.0),
        Point::new(2.0, 5.0, 1.0),
    ];
    let via_slice = pts
        .as_slice()
        .fit(&Model::polynomial("l", 1).with_params(vec![0.0, 0.0]));
    // The same works on a Vec<Point> (and the array directly) via the ?Sized
    // blanket — no `.as_slice()` needed.
    let via_vec = pts
        .to_vec()
        .fit(&Model::polynomial("l", 1).with_params(vec![0.0, 0.0]));
    let via_points = Points::unweighted(&[0.0, 1.0, 2.0], &[1.0, 3.0, 5.0])
        .fit(&Model::polynomial("l", 1).with_params(vec![0.0, 0.0]));
    assert_eq!(via_slice.params, via_points.params);
    assert_eq!(via_vec.params, via_points.params);
    assert!(rel_close(via_slice.params[1], 2.0, 1e-6));
}

#[test]
fn gaussian_estimate_from_custom_points() {
    // A discretized Gaussian peak; estimate_from seeds (constant, mean, sigma).
    let xs: Vec<f64> = (-20..=20).map(|i| i as f64 * 0.25).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|&x| 5.0 * (-0.5 * ((x - 1.0) / 0.8).powi(2)).exp())
        .collect();
    let data = Points::new(&xs, &ys, &vec![0.05; xs.len()]);

    let model = Model::gaussian("g")
        .estimate_from(&data)
        .lower_limit("sigma", 0.0);
    let fit = data.fit_opts(&model, &FitOptions::new().method(FitMethod::Chi2));
    assert!(fit.valid);
    assert!(
        rel_close(fit.params[1], 1.0, 1e-3),
        "mean {}",
        fit.params[1]
    );
    assert!(
        rel_close(fit.params[2], 0.8, 1e-3),
        "sigma {}",
        fit.params[2]
    );
}

#[test]
fn too_few_points_is_reported_invalid() {
    let data = Points::new(&[0.0], &[1.0], &[0.1]); // 1 point, 2 free params
    let fit = data.fit(&Model::polynomial("line", 1).with_params(vec![0.0, 0.0]));
    assert!(!fit.valid);
    assert_eq!(fit.ndf, 0);
    assert!(fit.p_value().is_nan());
}

/// A bespoke dataset type implementing `FitData` directly — the extension point
/// for data that is neither a histogram nor a graph.
struct Decay {
    t: Vec<f64>,
    counts: Vec<f64>,
}
impl FitData for Decay {
    fn points(&self) -> Vec<Point> {
        self.t
            .iter()
            .zip(&self.counts)
            .map(|(&t, &c)| Point::new(t, c, c.max(1.0).sqrt())) // Poisson errors
            .collect()
    }
}

#[test]
fn user_fitdata_impl_fits_an_exponential() {
    let t: Vec<f64> = (0..10).map(|i| i as f64).collect();
    let counts: Vec<f64> = t.iter().map(|&t| (5.0 - 0.5 * t).exp()).collect();
    let decay = Decay { t, counts };

    let fit = decay.fit(&Model::exponential("decay").with_params(vec![5.0, -0.5]));
    assert!(fit.valid);
    assert!(
        rel_close(fit.params[1], -0.5, 1e-3),
        "slope {}",
        fit.params[1]
    );
}

// --- The optional Nelder–Mead (`argmin`) backend ---------------------------

#[cfg(feature = "argmin")]
#[test]
fn nelder_mead_recovers_a_line() {
    use oxiroot_fit::Minimizer;
    let x = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let y: Vec<f64> = x.iter().map(|&x| 3.0 + 2.0 * x).collect();
    let data = Points::new(&x, &y, &[0.05; 6]);
    let opts = FitOptions::new().minimizer(Minimizer::NelderMead);
    let fit = data.fit_opts(
        &Model::polynomial("l", 1).with_params(vec![0.0, 0.0]),
        &opts,
    );
    assert!(fit.valid);
    assert!(
        rel_close(fit.params[0], 3.0, 1e-3),
        "intercept {}",
        fit.params[0]
    );
    assert!(
        rel_close(fit.params[1], 2.0, 1e-3),
        "slope {}",
        fit.params[1]
    );
    assert_eq!(fit.ndf, 4);
}

#[cfg(feature = "argmin")]
#[test]
fn nelder_mead_agrees_with_minuit2_on_a_gaussian() {
    use oxiroot_fit::Minimizer;
    // A clean discretized Gaussian; fit it with both backends and compare.
    let xs: Vec<f64> = (-30..=30).map(|i| i as f64 * 0.1).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|&x| 7.0 * (-0.5 * ((x - 0.5) / 0.9).powi(2)).exp())
        .collect();
    let data = Points::new(&xs, &ys, &vec![0.05; xs.len()]);
    let model = || {
        Model::gaussian("g")
            .estimate_from(&data)
            .lower_limit("sigma", 0.0)
    };

    let m = data.fit_opts(&model(), &FitOptions::new().minimizer(Minimizer::Minuit2));
    let nm = data.fit_opts(
        &model(),
        &FitOptions::new().minimizer(Minimizer::NelderMead),
    );
    assert!(m.valid && nm.valid);
    // The two minimizers land on the same (constant, mean, sigma).
    for k in 0..3 {
        assert!(
            (m.params[k] - nm.params[k]).abs() < 1e-2,
            "param {k}: minuit {} vs nelder-mead {}",
            m.params[k],
            nm.params[k]
        );
    }
    // Nelder–Mead still reports parabolic errors (numerical Hessian) + covariance,
    // but no MINOS.
    assert!(nm.errors.iter().all(|e| e.is_finite() && *e > 0.0));
    assert!(nm.covariance.is_some());
    assert!(nm.minos.is_none());
    // Its parabolic errors are in the same ballpark as Minuit2's.
    for k in 0..3 {
        assert!(
            rel_close(m.errors[k], nm.errors[k], 0.3),
            "error {k}: minuit {} vs nelder-mead {}",
            m.errors[k],
            nm.errors[k]
        );
    }
}

#[cfg(feature = "argmin")]
#[test]
fn nelder_mead_respects_fixed_parameters() {
    use oxiroot_fit::Minimizer;
    let xs: Vec<f64> = (-20..=20).map(|i| i as f64 * 0.1).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|&x| 5.0 * (-0.5 * ((x - 0.0) / 0.8).powi(2)).exp())
        .collect();
    let data = Points::new(&xs, &ys, &vec![0.05; xs.len()]);
    let model = Model::gaussian("g")
        .with_params(vec![5.0, 0.0, 0.8])
        .fix("mean");
    let fit = data.fit_opts(&model, &FitOptions::new().minimizer(Minimizer::NelderMead));
    assert!(fit.valid);
    assert_eq!(fit.params[1], 0.0, "fixed mean stays put");
    assert_eq!(fit.errors[1], 0.0, "fixed parameter has no error");
}
