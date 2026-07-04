# Particle data

`oxiroot::particle` is a Rust take on
[scikit-hep `particle`](https://github.com/scikit-hep/particle): decode a PDG
Monte Carlo particle ID, and look particles up in a bundled PDG table for their
mass, width, lifetime, and quantum numbers. It is a dependency-free leaf crate
(`oxiroot-particle`), always available — no feature flag.

There are two pieces: [`PdgId`](../api/oxiroot/particle/struct.PdgId.html), which
decodes an ID purely from its digits, and
[`Particle`](../api/oxiroot/particle/struct.Particle.html), which reads the
bundled table.

## Decoding a PDG ID

The [PDG numbering scheme](https://pdg.lbl.gov/current/reviews/rpp2023-rev-monte-carlo-numbering.pdf)
encodes what a particle *is* in the digits of its integer ID. `PdgId` answers
those questions with no lookup table — just arithmetic:

```rust
use oxiroot::particle::PdgId;

let proton = PdgId::new(2212);
assert!(proton.is_baryon() && proton.is_hadron());
assert_eq!(proton.charge(), Some(1.0));
assert!(proton.has_up() && proton.has_down());

assert!(PdgId::new(211).is_meson()); // pi+
assert!(PdgId::new(-11).is_lepton()); // positron
assert_eq!(PdgId::new(-11).charge(), Some(1.0));

// Even a nucleus ID yields its mass and atomic numbers:
let u235 = PdgId::new(1000922350);
assert!(u235.is_nucleus());
assert_eq!((u235.a(), u235.z()), (Some(235), Some(92)));
```

The classifiers mirror scikit-hep's `pdgid` functions:

| Group | Methods |
|---|---|
| Kind | `is_quark`, `is_lepton`, `is_neutrino`, `is_meson`, `is_baryon`, `is_hadron`, `is_diquark`, `is_nucleus`, `is_pentaquark`, `is_gauge_boson_or_higgs` |
| Exotica | `is_susy`, `is_r_hadron`, `is_dyon`, `is_qball`, `is_technicolor`, `is_excited_quark_or_lepton`, `is_generator_specific`, `is_special_particle` |
| Quark content | `has_up`/`down`/`strange`/`charm`/`bottom`/`top`, `has_fundamental_anti` |
| Numbers | `charge`, `three_charge`, `j_spin`/`j`, `s_spin`, `l_spin`, `a`, `z`, `is_valid` |

`charge` is in units of `e`; `three_charge` is `3·charge` as an exact integer (so
a down quark is `three_charge() == Some(-1)`, i.e. `-1/3 e`). `j_spin`/`s_spin`/
`l_spin` return the multiplicity `2X + 1`.

!!! note "Verified against scikit-hep"
    Every classifier and number is checked against the `particle` package for the
    whole standard table plus a spread of exotic IDs (nuclei, SUSY, R-hadrons,
    di-quarks, pentaquarks, dyons, …) in the crate's `tests/oracle.rs` — the same
    way `oxiroot::stat` is verified against `scipy.stats`.

## The particle table

[`Particle`](../api/oxiroot/particle/struct.Particle.html) looks up the physical
properties the digits cannot give you — mass, width, isospin, parity — from a
bundled ~600-particle PDG table. Masses and widths are in **MeV**, lifetimes in
**ns**, cτ in **mm**; an `Option` is `None` when the PDG lists the value as
unknown.

```rust
use oxiroot::particle::Particle;

let muon = Particle::from_name("mu-").unwrap();
assert_eq!(muon.pdg_id(), 13);
assert!((muon.mass().unwrap() - 105.6583755).abs() < 1e-4); // MeV
assert!((muon.lifetime().unwrap() - 2196.98).abs() < 0.1); // ns (τ = ħ/Γ)

let jpsi = Particle::from_pdgid(443).unwrap();
assert!(jpsi.is_self_conjugate());
assert_eq!(jpsi.j(), Some(1.0)); // a vector meson
```

| Accessor | Returns |
|---|---|
| `mass` / `mass_upper` / `mass_lower` | mass and its ± uncertainties (MeV) |
| `width` / `width_upper` / `width_lower` | decay width Γ (MeV); `0` for a stable particle |
| `lifetime` | mean lifetime τ = ħ/Γ (ns); `inf` if stable |
| `ctau` | proper decay length cτ (mm) |
| `charge` / `three_charge` | electric charge (in `e`, and as `3·charge`) |
| `isospin`, `j`, `parity`, `c_parity`, `g_parity`, `spin_type` | quantum numbers |
| `name`, `latex_name`, `quarks` | the name, its LaTeX form, and the quark content |
| `status`, `rank`, `anti_flag`, `is_self_conjugate` | table metadata |
| `pdgid` | the [`PdgId`](../api/oxiroot/particle/struct.PdgId.html), for the decoders above |

Look particles up with `from_pdgid` / `from_name`, or iterate the whole table
with `all()` and filter it like scikit-hep's `Particle.findall`:

```rust
use oxiroot::particle::Particle;

// The charged leptons, lightest first.
let mut leptons: Vec<Particle> = Particle::all()
    .filter(|p| p.pdgid().is_lepton() && p.charge() == Some(-1.0) && p.mass().is_some())
    .collect();
leptons.sort_by(|a, b| a.mass().partial_cmp(&b.mass()).unwrap());
assert_eq!(leptons.iter().map(|p| p.name()).collect::<Vec<_>>(), ["e-", "mu-", "tau-"]);
```

### Antiparticles

`invert()` returns the antiparticle: the same particle when it is self-conjugate,
otherwise the table entry with the negated ID.

```rust
use oxiroot::particle::Particle;

let pip = Particle::from_name("pi+").unwrap();
assert_eq!(pip.invert().unwrap().name(), "pi-");
assert_eq!(pip.invert().unwrap().charge(), Some(-1.0));

let pi0 = Particle::from_name("pi0").unwrap();
assert_eq!(pi0.invert().unwrap(), pi0); // self-conjugate
```

## Data provenance

The bundled table is generated from the scikit-hep `particle` package's
`particle2026.csv` (BSD-3-Clause), which reproduces the
[Particle Data Group](https://pdg.lbl.gov)'s *Review of Particle Physics*. The
`PdgId` decoder is a port of that package's `pdgid.functions`. Regenerate the
table with `scripts/gen_particle.py`.

## See also

- [Statistics](statistics.md) — the scipy.stats-verified `oxiroot::stat`, including HEP lineshapes.
- A runnable tour: `cargo run -p oxiroot --example particles`.
- [API reference](../api/oxiroot/particle/index.html) — the full `oxiroot::particle` surface.
