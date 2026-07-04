//! Pure-Rust PDG particle data — a Rust take on
//! [scikit-hep `particle`](https://github.com/scikit-hep/particle).
//!
//! Two things live here:
//!
//! - [`PdgId`] — a decoder for the [PDG Monte Carlo numbering scheme][pdg]. Given
//!   any signed integer ID it tells you *what the particle is* (lepton? meson?
//!   baryon? nucleus?) and its properties (charge, spin, quark content) purely
//!   from the digits, with **no data table**. This is a faithful port of
//!   scikit-hep's `pdgid.functions`, verified against it.
//! - [`Particle`] — a lookup into a **bundled PDG table** (~600 particles) for
//!   physical properties: mass, width, isospin, parity, and derived quantities
//!   (lifetime, cτ, charge). Look one up by ID or name.
//!
//! [pdg]: https://pdg.lbl.gov/current/reviews/rpp2023-rev-monte-carlo-numbering.pdf
//!
//! # Examples
//!
//! ```
//! use oxiroot_particle::{Particle, PdgId};
//!
//! // Decode an ID with no lookup — just arithmetic on the digits.
//! assert!(PdgId::new(2212).is_baryon()); // proton
//! assert!(PdgId::new(211).is_meson()); // pi+
//! assert_eq!(PdgId::new(-11).charge(), Some(1.0)); // positron
//!
//! // Physical properties from the bundled table — by name, id, or a `literals`
//! // constant.
//! let muon = Particle::from_name("mu-").unwrap();
//! assert!((muon.mass().unwrap() - 105.6583755).abs() < 1e-4); // MeV
//! assert!((muon.lifetime().unwrap() - 2197.0).abs() < 1.0); // ns
//! assert_eq!(oxiroot_particle::literals::proton().charge(), Some(1.0));
//!
//! // Iterate / filter the table.
//! let n_leptons = Particle::all().filter(|p| p.pdgid().is_lepton()).count();
//! assert!(n_leptons >= 12); // e, mu, tau, 3 neutrinos, and their antiparticles
//! ```
//!
//! The [`literals`] module gives friendly named accessors for the common
//! particles ([`literals::electron`], [`literals::jpsi`], …).
//!
//! # Data provenance
//!
//! The bundled table in `data.rs` is generated from the
//! [scikit-hep `particle`](https://github.com/scikit-hep/particle) package's
//! `particle2026.csv` (BSD-3-Clause), which in turn reproduces the
//! [Particle Data Group](https://pdg.lbl.gov)'s *Review of Particle Physics*.
//! The `PdgId` decoder is a port of the same package's `pdgid.functions`.
//! Regenerate the table with `scripts/gen_particle.py`.
//!
//! The upstream BSD-3-Clause license (© Eduardo Rodrigues and Henry Schreiner)
//! is reproduced in this crate's `LICENSE-3rdparty` file, as its terms require.

#![doc(html_root_url = "https://docs.rs/oxiroot-particle")]

mod data;
mod enums;
pub mod literals;
mod particle;
mod pdgid;

pub use enums::{Inv, Parity, SpinType, Status};
pub use particle::Particle;
pub use pdgid::PdgId;
