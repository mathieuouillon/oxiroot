//! GENERATED — expected Particle-level derived values from scikit-hep `particle`.

pub const NONE_I: i64 = i64::MIN;

pub struct PCase {
    pub id: i32,
    pub name: &'static str,
    pub mass: f64,
    pub three_charge: i64,
    pub lifetime_ns: f64,
    pub ctau_mm: f64,
}

#[rustfmt::skip]
pub static PARTICLE_CASES: &[PCase] = &[
    PCase { id: 11, name: "e-", mass: 0.51099895069, three_charge: -3, lifetime_ns: f64::INFINITY, ctau_mm: f64::INFINITY },
    PCase { id: -11, name: "e+", mass: 0.51099895069, three_charge: 3, lifetime_ns: f64::INFINITY, ctau_mm: f64::INFINITY },
    PCase { id: 13, name: "mu-", mass: 105.6583755, three_charge: -3, lifetime_ns: 2196.981174899978, ctau_mm: 658638.3866029924 },
    PCase { id: 22, name: "gamma", mass: 0.0, three_charge: 0, lifetime_ns: f64::INFINITY, ctau_mm: f64::INFINITY },
    PCase { id: 23, name: "Z0", mass: 91187.9, three_charge: 0, lifetime_ns: 2.637595499703092e-16, ctau_mm: 7.907312380657282e-14 },
    PCase { id: 24, name: "W+", mass: 80362.0, three_charge: 3, lifetime_ns: 3.0757568081818065e-16, ctau_mm: 9.220886937350584e-14 },
    PCase { id: 25, name: "H0", mass: 125130.0, three_charge: 0, lifetime_ns: 2.194039856503022e-13, ctau_mm: 6.577566015310082e-11 },
    PCase { id: 111, name: "pi0", mass: 134.9768, three_charge: 0, lifetime_ns: 8.427809948155015e-08, ctau_mm: 2.5265938599142445e-05 },
    PCase { id: 211, name: "pi+", mass: 139.57039, three_charge: 3, lifetime_ns: 26.032746280292145, ctau_mm: 7804.420995859139 },
    PCase { id: -211, name: "pi-", mass: 139.57039, three_charge: -3, lifetime_ns: 26.032746280292145, ctau_mm: 7804.420995859139 },
    PCase { id: 130, name: "K(L)0", mass: 497.611, three_charge: 0, lifetime_ns: 51.143120198205644, ctau_mm: 15332.321714009518 },
    PCase { id: 310, name: "K(S)0", mass: 497.611, three_charge: 0, lifetime_ns: 0.0895429010381056, ctau_mm: 26.84428639866443 },
    PCase { id: 321, name: "K+", mass: 493.677, three_charge: 3, lifetime_ns: 12.379386062646352, ctau_mm: 3711.246576251692 },
    PCase { id: 2212, name: "p", mass: 938.27208943, three_charge: 3, lifetime_ns: f64::INFINITY, ctau_mm: f64::INFINITY },
    PCase { id: 2112, name: "n", mass: 939.5654219, three_charge: 0, lifetime_ns: 878318597479.1921, ctau_mm: 263313291245399.62 },
    PCase { id: 443, name: "J/psi(1S)", mass: 3096.9, three_charge: 0, lifetime_ns: 7.108120485430957e-12, ctau_mm: 2.1309609120875e-09 },
    PCase { id: 521, name: "B+", mass: 5279.41, three_charge: 3, lifetime_ns: 0.0016369359784901931, ctau_mm: 0.49074106058021016 },
    PCase { id: 531, name: "B(s)0", mass: 5366.93, three_charge: 0, lifetime_ns: 0.0015148721678962178, ctau_mm: 0.4541472507693958 },
    PCase { id: 5122, name: "Lambda(b)0", mass: 5619.57, three_charge: 0, lifetime_ns: 0.00146497208313133, ctau_mm: 0.4391875817033218 },
    PCase { id: 15, name: "tau-", mass: 1776.93, three_charge: -3, lifetime_ns: 0.0002903449302827113, ctau_mm: 0.08704322031729267 },
    PCase { id: 16, name: "nu(tau)", mass: f64::NAN, three_charge: 0, lifetime_ns: f64::INFINITY, ctau_mm: f64::INFINITY },
    PCase { id: 3122, name: "Lambda", mass: 1115.683, three_charge: 0, lifetime_ns: 0.26171449580552947, ctau_mm: 78.46003199177038 },
    PCase { id: 411, name: "D+", mass: 1869.66, three_charge: 3, lifetime_ns: 0.0010332997754331345, ctau_mm: 0.30977547952794743 },
    PCase { id: 421, name: "D0", mass: 1864.84, three_charge: 0, lifetime_ns: 0.0004103565816402161, ctau_mm: 0.12302180826639805 },
];
