//! Named accessors for the commonly-used particles, mirroring scikit-hep
//! `particle`'s `literals` — so you can write [`literals::proton()`](proton)
//! instead of `Particle::from_pdgid(2212).unwrap()`.
//!
//! Each returns the [`Particle`] from the bundled table. These are the everyday
//! particles with friendly names; for anything else (resonances, di-quarks, …)
//! use [`Particle::from_name`] or [`Particle::from_pdgid`].
//!
//! ```
//! use oxiroot_particle::literals as lp;
//! assert_eq!(lp::proton().pdg_id(), 2212);
//! assert_eq!(lp::electron().charge(), Some(-1.0));
//! assert!(lp::jpsi().is_self_conjugate());
//! ```

use crate::Particle;

/// Look up a literal's particle. The id is a compile-time constant known to be in
/// the bundled table, so the lookup never fails.
fn lit(id: i32) -> Particle {
    Particle::from_pdgid(id).expect("literal PDG id is present in the bundled table")
}

// --- Quarks ----------------------------------------------------------------
/// The down quark (PDG 1).
pub fn down() -> Particle {
    lit(1)
}
/// The up quark (PDG 2).
pub fn up() -> Particle {
    lit(2)
}
/// The strange quark (PDG 3).
pub fn strange() -> Particle {
    lit(3)
}
/// The charm quark (PDG 4).
pub fn charm() -> Particle {
    lit(4)
}
/// The bottom quark (PDG 5).
pub fn bottom() -> Particle {
    lit(5)
}
/// The top quark (PDG 6).
pub fn top() -> Particle {
    lit(6)
}

// --- Charged leptons -------------------------------------------------------
/// The electron, e⁻ (PDG 11).
pub fn electron() -> Particle {
    lit(11)
}
/// The positron, e⁺ (PDG −11).
pub fn positron() -> Particle {
    lit(-11)
}
/// The muon, μ⁻ (PDG 13).
pub fn muon() -> Particle {
    lit(13)
}
/// The antimuon, μ⁺ (PDG −13).
pub fn antimuon() -> Particle {
    lit(-13)
}
/// The tau, τ⁻ (PDG 15).
pub fn tau() -> Particle {
    lit(15)
}
/// The antitau, τ⁺ (PDG −15).
pub fn antitau() -> Particle {
    lit(-15)
}

// --- Neutrinos -------------------------------------------------------------
/// The electron neutrino, νₑ (PDG 12).
pub fn nu_e() -> Particle {
    lit(12)
}
/// The electron antineutrino (PDG −12).
pub fn nu_e_bar() -> Particle {
    lit(-12)
}
/// The muon neutrino, ν_μ (PDG 14).
pub fn nu_mu() -> Particle {
    lit(14)
}
/// The muon antineutrino (PDG −14).
pub fn nu_mu_bar() -> Particle {
    lit(-14)
}
/// The tau neutrino, ν_τ (PDG 16).
pub fn nu_tau() -> Particle {
    lit(16)
}
/// The tau antineutrino (PDG −16).
pub fn nu_tau_bar() -> Particle {
    lit(-16)
}

// --- Gauge bosons and the Higgs --------------------------------------------
/// The photon, γ (PDG 22).
pub fn photon() -> Particle {
    lit(22)
}
/// The gluon, g (PDG 21).
pub fn gluon() -> Particle {
    lit(21)
}
/// The W⁺ boson (PDG 24).
pub fn w_plus() -> Particle {
    lit(24)
}
/// The W⁻ boson (PDG −24).
pub fn w_minus() -> Particle {
    lit(-24)
}
/// The Z boson (PDG 23).
pub fn z() -> Particle {
    lit(23)
}
/// The Higgs boson, H (PDG 25).
pub fn higgs() -> Particle {
    lit(25)
}

// --- Light unflavoured mesons ----------------------------------------------
/// The charged pion π⁺ (PDG 211).
pub fn pi_plus() -> Particle {
    lit(211)
}
/// The charged pion π⁻ (PDG −211).
pub fn pi_minus() -> Particle {
    lit(-211)
}
/// The neutral pion π⁰ (PDG 111).
pub fn pi_zero() -> Particle {
    lit(111)
}
/// The kaon K⁺ (PDG 321).
pub fn k_plus() -> Particle {
    lit(321)
}
/// The kaon K⁻ (PDG −321).
pub fn k_minus() -> Particle {
    lit(-321)
}
/// The neutral kaon K⁰ (PDG 311).
pub fn k_zero() -> Particle {
    lit(311)
}
/// The neutral antikaon K̄⁰ (PDG −311).
pub fn k_zero_bar() -> Particle {
    lit(-311)
}
/// The short-lived neutral kaon K⁰_S (PDG 310).
pub fn k_short() -> Particle {
    lit(310)
}
/// The long-lived neutral kaon K⁰_L (PDG 130).
pub fn k_long() -> Particle {
    lit(130)
}
/// The η meson (PDG 221).
pub fn eta() -> Particle {
    lit(221)
}
/// The η′ meson (PDG 331).
pub fn eta_prime() -> Particle {
    lit(331)
}
/// The ρ⁺ meson (PDG 213).
pub fn rho_plus() -> Particle {
    lit(213)
}
/// The ρ⁻ meson (PDG −213).
pub fn rho_minus() -> Particle {
    lit(-213)
}
/// The ρ⁰ meson (PDG 113).
pub fn rho_zero() -> Particle {
    lit(113)
}
/// The ω meson (PDG 223).
pub fn omega() -> Particle {
    lit(223)
}
/// The φ meson (PDG 333).
pub fn phi() -> Particle {
    lit(333)
}

// --- Quarkonia -------------------------------------------------------------
/// The η_c(1S) charmonium (PDG 441).
pub fn eta_c() -> Particle {
    lit(441)
}
/// The J/ψ(1S) charmonium (PDG 443).
pub fn jpsi() -> Particle {
    lit(443)
}
/// The ψ(2S) charmonium (PDG 100443).
pub fn psi_2s() -> Particle {
    lit(100443)
}
/// The Υ(1S) bottomonium (PDG 553).
pub fn upsilon_1s() -> Particle {
    lit(553)
}
/// The Υ(2S) bottomonium (PDG 100553).
pub fn upsilon_2s() -> Particle {
    lit(100553)
}
/// The Υ(3S) bottomonium (PDG 200553).
pub fn upsilon_3s() -> Particle {
    lit(200553)
}

// --- Charm mesons ----------------------------------------------------------
/// The D⁰ meson (PDG 421).
pub fn d_zero() -> Particle {
    lit(421)
}
/// The D̄⁰ meson (PDG −421).
pub fn d_zero_bar() -> Particle {
    lit(-421)
}
/// The D⁺ meson (PDG 411).
pub fn d_plus() -> Particle {
    lit(411)
}
/// The D⁻ meson (PDG −411).
pub fn d_minus() -> Particle {
    lit(-411)
}
/// The D_s⁺ meson (PDG 431).
pub fn d_s_plus() -> Particle {
    lit(431)
}
/// The D_s⁻ meson (PDG −431).
pub fn d_s_minus() -> Particle {
    lit(-431)
}

// --- Bottom mesons ---------------------------------------------------------
/// The B⁰ meson (PDG 511).
pub fn b_zero() -> Particle {
    lit(511)
}
/// The B̄⁰ meson (PDG −511).
pub fn b_zero_bar() -> Particle {
    lit(-511)
}
/// The B⁺ meson (PDG 521).
pub fn b_plus() -> Particle {
    lit(521)
}
/// The B⁻ meson (PDG −521).
pub fn b_minus() -> Particle {
    lit(-521)
}
/// The B_s⁰ meson (PDG 531).
pub fn b_s_zero() -> Particle {
    lit(531)
}
/// The B̄_s⁰ meson (PDG −531).
pub fn b_s_zero_bar() -> Particle {
    lit(-531)
}
/// The B_c⁺ meson (PDG 541).
pub fn b_c_plus() -> Particle {
    lit(541)
}
/// The B_c⁻ meson (PDG −541).
pub fn b_c_minus() -> Particle {
    lit(-541)
}

// --- Baryons ---------------------------------------------------------------
/// The proton, p (PDG 2212).
pub fn proton() -> Particle {
    lit(2212)
}
/// The antiproton, p̄ (PDG −2212).
pub fn antiproton() -> Particle {
    lit(-2212)
}
/// The neutron, n (PDG 2112).
pub fn neutron() -> Particle {
    lit(2112)
}
/// The antineutron, n̄ (PDG −2112).
pub fn antineutron() -> Particle {
    lit(-2112)
}
/// The Λ baryon (PDG 3122).
pub fn lambda() -> Particle {
    lit(3122)
}
/// The anti-Λ baryon (PDG −3122).
pub fn antilambda() -> Particle {
    lit(-3122)
}
/// The Σ⁺ baryon (PDG 3222).
pub fn sigma_plus() -> Particle {
    lit(3222)
}
/// The Σ⁰ baryon (PDG 3212).
pub fn sigma_zero() -> Particle {
    lit(3212)
}
/// The Σ⁻ baryon (PDG 3112).
pub fn sigma_minus() -> Particle {
    lit(3112)
}
/// The Ξ⁰ baryon (PDG 3322).
pub fn xi_zero() -> Particle {
    lit(3322)
}
/// The Ξ⁻ baryon (PDG 3312).
pub fn xi_minus() -> Particle {
    lit(3312)
}
/// The Ω⁻ baryon (PDG 3334).
pub fn omega_minus() -> Particle {
    lit(3334)
}
/// The Δ⁺⁺ baryon (PDG 2224).
pub fn delta_plusplus() -> Particle {
    lit(2224)
}
/// The Λ_c⁺ charmed baryon (PDG 4122).
pub fn lambda_c_plus() -> Particle {
    lit(4122)
}
/// The Λ_b⁰ bottom baryon (PDG 5122).
pub fn lambda_b_zero() -> Particle {
    lit(5122)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_resolve_and_are_correct() {
        // A spot-check of ids, charges, and self-conjugacy.
        assert_eq!(electron().pdg_id(), 11);
        assert_eq!(electron().charge(), Some(-1.0));
        assert_eq!(positron().pdg_id(), -11);
        assert_eq!(proton().pdg_id(), 2212);
        assert_eq!(proton().charge(), Some(1.0));
        assert_eq!(antiproton().charge(), Some(-1.0));
        assert!((muon().mass().unwrap() - 105.6583755).abs() < 1e-4);
        assert!(jpsi().is_self_conjugate());
        assert!(photon().pdgid().is_gauge_boson_or_higgs());
        assert!(lambda_c_plus().pdgid().has_charm());
        // Antiparticle relationships hold.
        assert_eq!(pi_plus().invert(), Some(pi_minus()));
        assert_eq!(d_zero().invert(), Some(d_zero_bar()));
    }
}
