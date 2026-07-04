//! A HEP counting experiment, narrated with the `oxiroot::stat` physics helpers:
//! discovery significance from a Poisson excess, a Garwood interval on the
//! observed counts, a Feldman–Cousins upper limit for a null search, a trigger
//! efficiency with Clopper–Pearson and Wilson intervals, and the combination of
//! two independent mass measurements. Pure stats — no file IO, no features.
//!
//! ```sh
//! cargo run -p oxiroot --example stat_physics
//! ```

use oxiroot::stat::{
    clopper_pearson, combine_measurements, feldman_cousins, poisson_conf_interval, poisson_sf,
    significance_from_pvalue, weighted_mean, wilson_interval,
};

fn main() {
    // Everything below is deterministic: fixed observed counts, no RNG needed.
    // The 68.27% confidence level (`1σ`) recurs, so name it once.
    const CL_1SIGMA: f64 = 0.6827;

    // --- 1. Discovery significance from a Poisson excess. ----------------------
    // We observe N = 17 events where the expected background alone is b = 8.5.
    // Is the excess a fluctuation, or a signal? The one-sided p-value is the
    // probability the background alone produces N or more counts, P(n ≥ N | b).
    // `poisson_sf(k, μ)` is P(n > k), so P(n ≥ N) = poisson_sf(N − 1, b).
    let n_obs = 17u32;
    let background = 8.5_f64;
    let p_value = poisson_sf(f64::from(n_obs) - 1.0, background);
    let z = significance_from_pvalue(p_value);
    // HEP wisdom: > 3σ is "evidence", > 5σ is a "discovery/observation".
    let verdict = if z >= 5.0 {
        "observation (> 5σ)"
    } else if z >= 3.0 {
        "evidence (> 3σ)"
    } else {
        "not significant (< 3σ)"
    };
    println!("--- 1. Discovery significance -----------------------------------");
    println!("  observed N = {n_obs}, expected background b = {background}");
    println!("  excess p-value P(n ≥ N | b) = {p_value:.3e}");
    println!("  Gaussian significance        = {z:.2}σ  →  {verdict}");

    // --- 2. Garwood (Poisson) confidence interval on the observed counts. ------
    // The signal-region count itself is a Poisson measurement; the Garwood
    // interval is its exact 68% (frequentist) error band. Note it is asymmetric
    // and, unlike √N, never dips below zero.
    let (lo, hi) = poisson_conf_interval(f64::from(n_obs), CL_1SIGMA);
    println!("\n--- 2. Poisson (Garwood) 68% interval on N ----------------------");
    println!("  N = {n_obs} counts → mean in [{lo:.2}, {hi:.2}]  (68.27% CL)");
    println!(
        "  asymmetric errors: +{:.2} / -{:.2}  (√N ≈ {:.2} for comparison)",
        hi - f64::from(n_obs),
        f64::from(n_obs) - lo,
        f64::from(n_obs).sqrt(),
    );

    // --- 3. A null search → Feldman–Cousins upper limit. -----------------------
    // A different channel: n = 3 observed over an expected background b = 2.0.
    // No excess here, so we set a limit rather than claim a signal. The
    // Feldman–Cousins unified interval avoids the empty-interval and
    // flip-flopping pathologies; with no excess its lower edge sits at 0 and the
    // physics result is the upper limit on the signal mean.
    let (search_n, search_bkg) = (3usize, 2.0_f64);
    let (fc_lo, fc_hi) = feldman_cousins(search_n, search_bkg, 0.90);
    println!("\n--- 3. Feldman–Cousins 90% interval (null search) ---------------");
    println!("  observed n = {search_n}, background b = {search_bkg}");
    println!("  signal interval [{fc_lo:.2}, {fc_hi:.2}] events  (90% CL)");
    println!("  → 90% CL upper limit on the signal: μ < {fc_hi:.2} events");

    // --- 4. Trigger efficiency with two binomial intervals. --------------------
    // A trigger fires on k = 95 of n = 100 signal events. The point efficiency is
    // k/n, but the error bar needs a proper binomial interval — the plain
    // ε(1−ε)/n Wald interval misbehaves near 0 or 1. Clopper–Pearson (exact,
    // ROOT's TEfficiency default) and Wilson (score) both stay in [0, 1].
    let (k_pass, n_trig) = (95.0_f64, 100.0_f64);
    let eff = k_pass / n_trig;
    let (cp_lo, cp_hi) = clopper_pearson(k_pass, n_trig, CL_1SIGMA);
    let (w_lo, w_hi) = wilson_interval(k_pass, n_trig, CL_1SIGMA);
    println!("\n--- 4. Trigger efficiency (k of n) ------------------------------");
    println!("  passed {k_pass:.0} of {n_trig:.0} → efficiency = {eff:.3}");
    println!(
        "  Clopper–Pearson 68% CI: [{:.3}, {:.3}]  (+{:.3} / -{:.3})",
        cp_lo,
        cp_hi,
        cp_hi - eff,
        eff - cp_lo,
    );
    println!("  Wilson score   68% CI: [{w_lo:.3}, {w_hi:.3}]");

    // --- 5. Combine two independent mass measurements. -------------------------
    // Two experiments measure the same mass with different precision. The
    // best combined value is the inverse-variance ("BLUE") weighted mean; its
    // error shrinks below either input. `combine_measurements` does exactly this;
    // `weighted_mean` with weights = 1/σ² reproduces the central value directly.
    let masses = [125.10_f64, 125.38_f64];
    let errors = [0.14_f64, 0.11_f64];
    let (m_comb, e_comb) = combine_measurements(&masses, &errors);
    let weights: Vec<f64> = errors.iter().map(|&s| 1.0 / (s * s)).collect();
    let m_wmean = weighted_mean(&masses, &weights);
    println!("\n--- 5. Combining two mass measurements --------------------------");
    println!(
        "  A: {:.2} ± {:.2} GeV     B: {:.2} ± {:.2} GeV",
        masses[0], errors[0], masses[1], errors[1],
    );
    println!("  combined (inverse-variance): {m_comb:.3} ± {e_comb:.3} GeV");
    println!(
        "  cross-check via weighted_mean(1/σ²): {m_wmean:.3} GeV  (matches: {})",
        (m_comb - m_wmean).abs() < 1e-9,
    );
    println!(
        "  → the combined error ({:.3}) beats the better single one ({:.2})",
        e_comb,
        errors.iter().cloned().fold(f64::INFINITY, f64::min),
    );
}
