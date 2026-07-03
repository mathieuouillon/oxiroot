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
    let (r, p) = pearsonr(&X, &Y);
    approx!(r, 0.939393939393939);
    approx!(p, 5.484052998513792e-05, 1e-7);
    let (r, p) = spearmanr(&X, &Y);
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
