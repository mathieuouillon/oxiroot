//! PDG particle data with oxiroot (`oxiroot::particle`, a Rust take on
//! scikit-hep/particle). Run:
//!
//! ```sh
//! cargo run -p oxiroot --example particles
//! ```
//!
//! Two things: decode a PDG Monte Carlo ID with no lookup table (is it a lepton?
//! a meson? its charge?), and look particles up in the bundled PDG table for
//! their mass, width, lifetime, and quantum numbers.

use oxiroot::particle::{Particle, PdgId};

fn main() {
    // --- 1. Decode PDG IDs straight from their digits — no table needed. ------
    println!("Decoding PDG Monte Carlo IDs (pure arithmetic on the digits):");
    for id in [11, 22, 211, 2212, 1000010020, 1000922350] {
        let p = PdgId::new(id);
        let kind = if p.is_lepton() {
            "lepton"
        } else if p.is_meson() {
            "meson"
        } else if p.is_baryon() {
            "baryon"
        } else if p.is_nucleus() {
            "nucleus"
        } else if p.is_gauge_boson_or_higgs() {
            "boson"
        } else {
            "other"
        };
        println!(
            "  {id:>11}: {kind:<8} charge = {:>4}   quarks: down={} up={} strange={} charm={} bottom={}",
            p.charge()
                .map_or("?".to_string(), |q| format!("{q:+.2}").replace(".00", "")),
            p.has_down() as u8,
            p.has_up() as u8,
            p.has_strange() as u8,
            p.has_charm() as u8,
            p.has_bottom() as u8,
        );
    }
    // A nucleus ID even yields its A and Z:
    let u235 = PdgId::new(1000922350);
    println!(
        "  uranium-235 ({}): A = {}, Z = {}",
        u235,
        u235.a().unwrap(),
        u235.z().unwrap()
    );

    // --- 2. Look up physical properties from the bundled PDG table. -----------
    println!("\nPhysical properties from the bundled table (masses in MeV):");
    for name in ["e-", "mu-", "pi+", "K(L)0", "p", "J/psi(1S)", "Z0", "H0"] {
        let Some(p) = Particle::from_name(name) else {
            continue;
        };
        let mass = p.mass().map_or("?".into(), |m| format!("{m:.4}"));
        let life = match p.lifetime() {
            Some(t) if t.is_finite() => format!("τ = {t:.3e} ns"),
            Some(_) => "stable".to_string(),
            None => "τ unknown".to_string(),
        };
        println!(
            "  {:<10} pdgid={:>6}  m = {:>12} MeV   J = {}   {}",
            p.name(),
            p.pdg_id(),
            mass,
            p.j().map_or("?".into(), |j| format!("{j}")),
            life,
        );
    }

    // --- 3. The muon lifetime, cross-checked against the textbook value. ------
    let muon = Particle::from_name("mu-").unwrap();
    println!(
        "\nMuon: mean lifetime {:.1} ns (PDG ≈ 2196.9 ns), cτ = {:.1} m",
        muon.lifetime().unwrap(),
        muon.ctau().unwrap() / 1000.0, // mm → m
    );

    // --- 4. Iterate + filter the table like scikit-hep's `Particle.findall`. --
    let mut charged_leptons: Vec<Particle> = Particle::all()
        .filter(|p| p.pdgid().is_lepton() && p.charge() == Some(-1.0) && p.mass().is_some())
        .collect();
    charged_leptons.sort_by(|a, b| a.mass().partial_cmp(&b.mass()).unwrap());
    println!("\nThe charged leptons, lightest first:");
    for l in &charged_leptons {
        println!("  {:<5} m = {:.4} MeV", l.name(), l.mass().unwrap());
    }

    // The heaviest meson in the table:
    let heaviest = Particle::all()
        .filter(|p| p.pdgid().is_meson())
        .filter(|p| p.mass().is_some())
        .max_by(|a, b| a.mass().partial_cmp(&b.mass()).unwrap())
        .unwrap();
    println!(
        "\nHeaviest meson in the table: {} at {:.0} MeV",
        heaviest.name(),
        heaviest.mass().unwrap()
    );

    // --- 5. Antiparticles. ----------------------------------------------------
    let pip = Particle::from_name("pi+").unwrap();
    let pim = pip.invert().unwrap();
    println!(
        "\nAntiparticle of {} (charge {:+}) is {} (charge {:+}); the {} is its own antiparticle.",
        pip.name(),
        pip.charge().unwrap(),
        pim.name(),
        pim.charge().unwrap(),
        Particle::from_name("pi0").unwrap().name(),
    );

    println!("\nBundled table: {} particles.", Particle::count());
}
