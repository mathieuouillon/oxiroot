//! Cross-checks against scipy 1.18 (`scipy.stats` / `scipy.special`). Every
//! expected value here was produced by the corresponding scipy call on the same
//! input; the tests are self-contained (no scipy at build time).

use oxiroot_stat::*;

/// The classic 8-point sample used across scipy's descriptive examples.
const DATA: [f64; 8] = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
const X: [f64; 10] = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10.];
const Y: [f64; 10] = [2., 1., 4., 3., 6., 5., 8., 7., 10., 9.];

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * (1.0 + b.abs())
}
macro_rules! approx {
    ($a:expr, $b:expr) => {
        assert!(close($a, $b, 1e-9), "{} = {} != {}", stringify!($a), $a, $b)
    };
    ($a:expr, $b:expr, $tol:expr) => {
        assert!(close($a, $b, $tol), "{} = {} != {}", stringify!($a), $a, $b)
    };
}

#[test]
fn special_functions_match_scipy() {
    approx!(erf(0.7), 0.6778011938374184);
    approx!(erfc(1.2), 0.08968602177036462);
    approx!(erf(-1.0), -0.8427007929497148);
    approx!(betainc(2.0, 3.0, 0.4), 0.5247999999999999);
    approx!(betainc(0.5, 2.5, 0.3), 0.7968893362799453);
    approx!(gammaln(5.0), 3.1780538303479458);
    approx!(beta(2.0, 3.0), 0.08333333333333333);
    approx!(gammainc(3.0, 5.0), 0.8753479805169189);
    approx!(gammaincc(3.0, 5.0), 0.12465201948308109);
    approx!(ndtri(0.975), 1.959963984540054);
    approx!(ndtri(0.1), -1.2815515655446004);
}

#[test]
fn descriptive_stats_match_scipy() {
    approx!(gmean(&DATA), 4.603215596046736);
    approx!(hmean(&DATA), 4.201750729470613);
    approx!(skew(&DATA, true), 0.65625, 1e-12);
    approx!(skew(&DATA, false), 0.8184875533567996);
    approx!(kurtosis(&DATA, true, true), -0.21875, 1e-12);
    approx!(kurtosis(&DATA, true, false), 0.9406249999999998);
    approx!(kurtosis(&DATA, false, true), 2.78125, 1e-12);
    approx!(moment(&DATA, 2), 4.0, 1e-12);
    approx!(moment(&DATA, 3), 5.25, 1e-12);
    approx!(moment(&DATA, 4), 44.5, 1e-12);
    approx!(sem(&DATA), 0.7559289460184544);
    approx!(variation(&DATA), 0.4, 1e-12);
    approx!(iqr(&DATA), 1.5, 1e-12);
    approx!(median(&DATA), 4.5, 1e-12);
    approx!(median_abs_deviation(&DATA, false), 0.5, 1e-12);
    approx!(median_abs_deviation(&DATA, true), 0.741301109252801);
    approx!(entropy(&DATA), 2.001064502656117);
    assert_eq!(
        zscore(&DATA),
        vec![-1.5, -0.5, -0.5, -0.5, 0.0, 0.0, 1.0, 2.0]
    );
    assert_eq!(
        rankdata(&DATA),
        vec![1.0, 3.0, 3.0, 3.0, 5.5, 5.5, 7.0, 8.0]
    );
}

#[test]
fn distributions_match_scipy() {
    let n = Normal::standard();
    approx!(n.pdf(0.5), 0.3520653267642995);
    approx!(n.cdf(1.5), 0.9331927987311419);
    approx!(n.sf(1.5), 0.06680720126885806);
    approx!(n.ppf(0.975), 1.959963984540054);

    let t = StudentT::new(5.0);
    approx!(t.pdf(1.0), 0.2196797973509805);
    approx!(t.cdf(1.0), 0.8183912661754387);
    approx!(t.sf(2.0), 0.05096973941492916);
    approx!(t.ppf(0.975), 2.5705818356363146, 1e-6);

    let c = ChiSquared::new(4.0);
    approx!(c.pdf(3.0), 0.1673476201113224);
    approx!(c.cdf(3.0), 0.4421745996289252);
    approx!(c.sf(3.0), 0.5578254003710748);
    approx!(c.ppf(0.95), 9.487729036781154, 1e-6);

    let f = FisherF::new(3.0, 10.0);
    approx!(f.pdf(2.0), 0.14821094155911813);
    approx!(f.cdf(2.0), 0.8219925926248245);
    approx!(f.sf(2.0), 0.17800740737517545);
    approx!(f.ppf(0.95), 3.7082648190468435, 1e-6);

    approx!(poisson_cdf(3.0, 2.5), 0.7575761331330662);
    approx!(poisson_sf(3.0, 2.5), 0.2424238668669339);
    approx!(binom_cdf(3.0, 10.0, 0.3), 0.6496107184000002);
    approx!(binom_sf(3.0, 10.0, 0.3), 0.3503892815999998);
}

#[test]
fn correlation_and_tests_match_scipy() {
    let (r, p) = pearsonr(&X, &Y).unwrap();
    approx!(r, 0.939393939393939);
    approx!(p, 5.484052998513792e-05, 1e-7);
    let (r, p) = spearmanr(&X, &Y).unwrap();
    approx!(r, 0.9393939393939393);
    approx!(p, 5.48405299851367e-05, 1e-7);

    let (t, p) = ttest_1samp(&DATA, 4.0);
    approx!(t, 1.3228756555322954);
    approx!(p, 0.22745281805976297);
    let (t, p) = ttest_ind(&X, &Y);
    approx!(t, 0.0, 1e-12);
    approx!(p, 1.0, 1e-12);

    let big = [
        0.0,
        2.6244129544236894,
        2.927892280477045,
        0.7233600241796017,
        -1.870407485923785,
        -2.3767728239894153,
        -0.2382464945967775,
        2.6709597961563674,
        3.768074739870145,
        2.13635545572527,
        -0.6320633326681093,
        -1.8999706196521102,
        -0.40971875400130475,
        2.5605011104799225,
        4.3718220670846115,
        3.4508635204713505,
        0.7362900500048042,
        -1.1841924756386701,
        -0.45296174031502834,
        2.349631628988857,
        4.738835752182883,
        4.609966915608168,
        2.1734460721287885,
        -0.23866121252551187,
        -0.3167350860198712,
        2.1029447497066807,
        4.8876753514388085,
        5.569127785213509,
        3.6127173649236073,
        0.9090983473610978,
        0.03590512772141441,
        1.887887064030805,
        4.854280043725072,
        6.299735580321801,
        4.987248058360072,
        2.215451991511547,
        0.6246634396706527,
        1.7693855999290018,
        4.689105736128156,
        6.791386158852264,
        6.2353394814380465,
        3.6241319935858733,
        1.4504353562530987,
        1.804675772114205,
        4.453105775316241,
        7.052710573602355,
        7.305365042946429,
        5.070719368235672,
        2.495236016029,
        2.038742041721585,
        4.212875438888213,
        7.110687527530125,
        8.159882776121457,
        6.487775450545503,
        3.7236328534451513,
        2.5007344799241404,
        4.035346993739265,
        7.008494265743475,
        8.778617944253611,
        7.810214021417414,
    ];
    let (k2, p) = normaltest(&big);
    approx!(k2, 2.494040535868694, 1e-9);
    approx!(p, 0.2873597775644675, 1e-9);
}

#[test]
fn describe_and_goodness_of_fit_match_scipy() {
    let d = describe(&DATA);
    assert_eq!(d.nobs, 8);
    approx!(d.min, 2.0, 1e-12);
    approx!(d.max, 9.0, 1e-12);
    approx!(d.mean, 5.0, 1e-12);
    approx!(d.variance, 4.571428571428571);
    approx!(d.skewness, 0.65625, 1e-12);
    approx!(d.kurtosis, -0.21875, 1e-12);

    // scipy.stats.chisquare([10,12,8,15,11], [11.2; 5]).
    let (chi2, p) = chisquare(&[10.0, 12.0, 8.0, 15.0, 11.0], &[11.2; 5]).unwrap();
    approx!(chi2, 2.392857142857143);
    approx!(p, 0.6639184808868118);

    // Two Kolmogorov–Smirnov samples.
    const A: [f64; 10] = [0.1, 0.3, 0.55, 0.7, 0.9, 1.2, 1.4, 1.7, 2.0, 2.2];
    const B: [f64; 10] = [0.2, 0.5, 0.6, 1.0, 1.1, 1.3, 1.9, 2.1, 2.5, 3.0];
    let (d2, p2) = ks_2samp(&A, &B);
    approx!(d2, 0.2, 1e-12);
    // Asymptotic Kolmogorov p = kstwobign.sf(sqrt(n_a n_b/(n_a+n_b)) D).
    approx!(p2, 0.9882610776435244, 3e-3);
    let (d1, p1) = ks_1samp(&A, |x| Normal::standard().cdf(x));
    approx!(d1, 0.539827837277029);
    approx!(p1, 0.00588625843788155, 1e-4);
}

#[test]
fn physics_helpers_match_reference() {
    // Significance <-> p-value (one-sided, the HEP "n sigma").
    approx!(significance_from_pvalue(2.8665157187919344e-07), 5.0, 1e-7);
    approx!(pvalue_from_significance(5.0), 2.8665157187919344e-07);
    approx!(pvalue_from_significance(3.0), 0.001349898031630093);

    // Weighted mean and inverse-variance combination.
    approx!(
        weighted_mean(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]).unwrap(),
        2.3333333333333335
    );
    let (m, e) = combine_measurements(&[10.0, 12.0], &[1.0, 2.0]).unwrap();
    approx!(m, 10.4, 1e-12);
    approx!(e, 0.8944271909999159);

    // Clopper–Pearson (8/10) and Garwood Poisson (k = 5) 95% intervals.
    let (lo, hi) = clopper_pearson(8.0, 10.0, 0.95);
    approx!(lo, 0.4439045376923585, 1e-6);
    approx!(hi, 0.9747892736731666, 1e-6);
    let (lo, hi) = poisson_conf_interval(5.0, 0.95);
    approx!(lo, 1.6234863901184204, 1e-5);
    approx!(hi, 11.66833207932267, 1e-5);

    approx!(betaincinv(2.0, 3.0, 0.5247999999999999), 0.4, 1e-9);
}

#[test]
fn nonparametric_tests_match_scipy() {
    const XU: [f64; 7] = [0.5, 1.2, 2.3, 3.1, 4.0, 5.5, 6.1];
    const YU: [f64; 8] = [2.1, 3.3, 4.4, 5.5, 6.6, 7.7, 8.8, 9.9];
    let (u, p) = mannwhitneyu(&XU, &YU);
    approx!(u, 11.5, 1e-12);
    approx!(p, 0.06383999186885041, 1e-3);

    const XW: [f64; 10] = [1.1, 2.2, 3.3, 4.4, 5.5, 6.6, 7.7, 8.8, 9.9, 11.0];
    const YW: [f64; 10] = [1.0, 2.5, 3.0, 5.0, 5.0, 7.0, 7.2, 9.0, 9.5, 10.0];
    let (w, p) = wilcoxon(&XW, &YW).unwrap();
    approx!(w, 20.0, 1e-12);
    approx!(p, 0.4436974958333839, 1e-3);
}

#[test]
fn efficiency_intervals_and_feldman_cousins() {
    // Wilson (scipy `method='wilson'`) and Agresti–Coull 95% intervals for 8/10.
    let (lo, hi) = wilson_interval(8.0, 10.0, 0.95);
    approx!(lo, 0.4901624715366418, 1e-4);
    approx!(hi, 0.9433178485456248, 1e-4);
    let (lo, hi) = agresti_coull_interval(8.0, 10.0, 0.95);
    approx!(lo, 0.4793675905661507, 1e-4);
    approx!(hi, 0.9541127295161158, 1e-4);

    // Feldman–Cousins 90% CL Poisson. The b = 0 cases match the canonical
    // FC-1998 Table IV values exactly (to grid resolution); the background case
    // is cross-checked against an independent implementation of the same
    // likelihood-ratio construction.
    let fc = |n, b| feldman_cousins(n, b, 0.90);
    let (lo, hi) = fc(0, 0.0);
    approx!(lo, 0.0, 1e-9);
    approx!(hi, 2.44, 5e-2);
    let (lo, hi) = fc(3, 0.0);
    approx!(lo, 1.10, 5e-2);
    approx!(hi, 7.42, 5e-2);
    let (lo, hi) = fc(10, 0.0);
    approx!(lo, 5.50, 5e-2);
    approx!(hi, 16.50, 5e-2);
    let (lo, hi) = fc(0, 3.0);
    approx!(lo, 0.0, 1e-9);
    approx!(hi, 0.95, 5e-2);
}

#[test]
fn bootstrap_is_reproducible_and_reasonable() {
    // 95% percentile bootstrap of the mean, reproducible by seed.
    let mean = |d: &[f64]| d.iter().sum::<f64>() / d.len() as f64;
    let (lo, hi) = bootstrap_ci(&X, mean, 4000, 0.95, 42);
    assert_eq!(bootstrap_ci(&X, mean, 4000, 0.95, 42), (lo, hi));

    // Brackets the sample mean (5.5) and is in the ballpark of the normal CI
    // (mean ± 1.96·sem ≈ [3.62, 7.38]).
    let m = mean(&X);
    assert!(lo < m && m < hi, "CI [{lo}, {hi}] must bracket {m}");
    let half = 1.96 * sem(&X);
    assert!(
        (hi - lo) > half && (hi - lo) < 4.0 * half,
        "width {} vs 2·half {}",
        hi - lo,
        2.0 * half
    );
}

#[test]
// The reference values are copied verbatim from scipy / ROOT output; some happen
// to equal a std constant (1/π) or carry more digits than f64 needs.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
fn hep_lineshapes_match_reference() {
    // Crystal Ball (unit peak) vs scipy.stats.crystalball.pdf(x, 1.5, 3) / pdf(0).
    approx!(crystal_ball(0.0, 0.0, 1.0, 1.5, 3.0), 1.0, 1e-12);
    for &(x, r) in &[
        (-4.0, 0.028501725529402444),
        (-2.5, 0.09619332366173326),
        (-1.5, 0.32465246735834974),
        (-0.5, 0.8824969025845955),
        (1.5, 0.32465246735834974),
        (2.5, 0.04393693362340741),
    ] {
        approx!(crystal_ball(x, 0.0, 1.0, 1.5, 3.0), r);
    }
    // alpha < 0 mirrors the tail to the high side.
    approx!(
        crystal_ball(2.5, 0.0, 1.0, -1.5, 3.0),
        crystal_ball(-2.5, 0.0, 1.0, 1.5, 3.0),
        1e-12
    );

    // Breit–Wigner == scipy.stats.cauchy(loc=0, scale=gamma/2).
    approx!(breit_wigner(-1.0, 0.0, 2.0), 0.15915494309189535);
    approx!(breit_wigner(0.0, 0.0, 2.0), 0.3183098861837907);

    // Voigt == scipy.special.voigt_profile(x, sigma=1, gamma=0.5); Humlicek w4 ≈ 1e-6.
    for &(x, r) in &[
        (-3.0, 0.028336408162199005),
        (-1.0, 0.20017963759083915),
        (0.0, 0.27895547038929436),
    ] {
        approx!(voigtian(x, 0.0, 1.0, 0.5), r, 1e-5);
    }

    // Moyal == scipy.stats.moyal.pdf.
    for &(x, r) in &[
        (-2.0, 0.026958231758816037),
        (0.0, 0.24197072451914337),
        (1.0, 0.20131624406488796),
        (5.0, 0.032637037799244456),
    ] {
        approx!(moyal(x, 0.0, 1.0), r);
    }

    // Landau == ROOT's TMath::Landau (Kölbig `denlan`): landau(v, 0, 1) == landau_pdf(v).
    for &(v, r) in &[
        (-2.0, 0.04398547840678685),
        (0.0, 0.1788541609),
        (1.0, 0.145206637130862),
        (5.0, 0.03916341957924747),
        (20.0, 0.003004979394102356),
    ] {
        approx!(landau(v, 0.0, 1.0), r, 1e-8);
    }
    // Unit-area Landau vs ROOT TMath::Landau(x, 0, 2, norm=true).
    approx!(landau(-2.0, 0.0, 2.0), 0.07569595567970359, 1e-8);
    approx!(landau(5.0, 0.0, 2.0), 0.04411210045807406, 1e-8);

    // Self-consistency for the shapes scipy/ROOT do not provide directly.
    // Double CB: unit peak at the mean, tails continuous at the boundaries.
    approx!(
        double_crystal_ball(5.0, 5.0, 1.0, 1.2, 3.0, 1.8, 4.0),
        1.0,
        1e-12
    );
    for &a in &[1.2_f64, 1.8] {
        let xb = 5.0 - a; // low boundary t = -a (sigma = 1)
        approx!(
            double_crystal_ball(xb - 1e-6, 5.0, 1.0, a, 3.0, 2.0, 4.0),
            double_crystal_ball(xb + 1e-6, 5.0, 1.0, a, 3.0, 2.0, 4.0),
            1e-4
        );
    }
    // Novosibirsk: unit peak at `peak`; tail = 0 recovers the Gaussian.
    approx!(novosibirsk(2.0, 2.0, 0.5, 0.3), 1.0, 1e-12);
    approx!(
        novosibirsk(2.5, 2.0, 0.5, 0.0),
        gaussian(2.5, 2.0, 0.5),
        1e-12
    );
    // Bifurcated Gaussian: a different width on each side (t = ∓1 → exp(-½)).
    approx!(
        bifurcated_gaussian(-1.0, 0.0, 1.0, 2.0),
        (-0.5f64).exp(),
        1e-12
    );
    approx!(
        bifurcated_gaussian(2.0, 0.0, 1.0, 2.0),
        (-0.5f64).exp(),
        1e-12
    );
    // ARGUS: zero outside (0, m0), positive inside.
    assert_eq!(argus(-1.0, 5.0, -3.0, 0.5), 0.0);
    assert_eq!(argus(6.0, 5.0, -3.0, 0.5), 0.0);
    assert!(argus(3.0, 5.0, -3.0, 0.5) > 0.0);
    // Relativistic BW: unit area over the physical (0, ∞), mode at M.
    let (m, g, n, hi) = (91.19, 2.5, 400_000usize, 400.0);
    let dx = hi / n as f64;
    let area: f64 = (0..n)
        .map(|i| relativistic_breit_wigner((i as f64 + 0.5) * dx, m, g) * dx)
        .sum();
    approx!(area, 1.0, 3e-3);
}
