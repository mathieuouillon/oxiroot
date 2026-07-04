//! [`Particle`] — a named entry in the bundled PDG table, with its physical
//! properties (mass, width, quantum numbers) and derived quantities (lifetime,
//! cτ, charge). Looked up by [`from_pdgid`](Particle::from_pdgid) or
//! [`from_name`](Particle::from_name).

use crate::data::{Row, PARTICLES};
use crate::enums::{Inv, Parity, SpinType, Status};
use crate::pdgid::PdgId;

/// Reduced Planck constant `ħ` in MeV·ns, for the width → lifetime conversion
/// (matches scikit-hep / `hepunits`).
const HBAR_MEV_NS: f64 = 6.582_119_569_509_066e-13;
/// Speed of light in mm/ns, for the lifetime → cτ conversion.
const C_LIGHT_MM_NS: f64 = 299.792_458;

/// `NaN` (used in the table for an unknown value) becomes `None`.
fn finite(x: f64) -> Option<f64> {
    if x.is_nan() {
        None
    } else {
        Some(x)
    }
}

/// A particle from the bundled PDG table — a cheap `Copy` handle onto its data.
///
/// Physical quantities are `Option`: `None` marks a value the PDG lists as
/// unknown (e.g. a neutrino's mass). Masses and widths are in **MeV**, lifetimes
/// in **ns**, and cτ in **mm**.
///
/// # Examples
/// ```
/// use oxiroot_particle::Particle;
///
/// let pion = Particle::from_name("pi+").unwrap();
/// assert_eq!(pion.pdg_id(), 211);
/// assert!((pion.mass().unwrap() - 139.57039).abs() < 1e-4);
/// assert_eq!(pion.charge(), Some(1.0));
///
/// let z = Particle::from_pdgid(23).unwrap();
/// assert!(z.is_self_conjugate());
/// ```
#[derive(Clone, Copy)]
pub struct Particle {
    row: &'static Row,
}

impl Particle {
    /// Look a particle up by its PDG ID (accepts an `i32` or a [`PdgId`]).
    #[must_use]
    pub fn from_pdgid(id: impl Into<i32>) -> Option<Particle> {
        let id = id.into();
        PARTICLES
            .iter()
            .find(|r| r.id == id)
            .map(|row| Particle { row })
    }

    /// Look a particle up by its exact name (e.g. `"pi+"`, `"J/psi(1S)"`),
    /// returning the first match.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Particle> {
        PARTICLES
            .iter()
            .find(|r| r.name == name)
            .map(|row| Particle { row })
    }

    /// Every particle in the bundled table.
    ///
    /// ```
    /// use oxiroot_particle::Particle;
    /// // All the charged mesons lighter than 1 GeV:
    /// let light: Vec<_> = Particle::all()
    ///     .filter(|p| p.pdgid().is_meson() && p.charge() != Some(0.0))
    ///     .filter(|p| p.mass().is_some_and(|m| m < 1000.0))
    ///     .collect();
    /// assert!(light.iter().any(|p| p.name() == "pi+"));
    /// ```
    pub fn all() -> impl Iterator<Item = Particle> {
        PARTICLES.iter().map(|row| Particle { row })
    }

    /// The number of particles in the bundled table.
    #[must_use]
    pub fn count() -> usize {
        PARTICLES.len()
    }

    /// The PDG ID as a [`PdgId`] (with all its decoding methods).
    #[must_use]
    pub fn pdgid(&self) -> PdgId {
        PdgId(self.row.id)
    }

    /// The raw signed PDG ID.
    #[must_use]
    pub fn pdg_id(&self) -> i32 {
        self.row.id
    }

    /// The particle name (e.g. `"pi+"`).
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.row.name
    }

    /// The LaTeX form of the name (e.g. `\pi^{+}`).
    #[must_use]
    pub fn latex_name(&self) -> &'static str {
        self.row.latex
    }

    /// The valence-quark content string (e.g. `"uud"` for the proton; empty for
    /// fundamental particles).
    #[must_use]
    pub fn quarks(&self) -> &'static str {
        self.row.quarks
    }

    /// The mass in MeV (`None` if unknown).
    #[must_use]
    pub fn mass(&self) -> Option<f64> {
        finite(self.row.mass)
    }

    /// The upper mass uncertainty in MeV (`None` if unknown).
    #[must_use]
    pub fn mass_upper(&self) -> Option<f64> {
        finite(self.row.mass_upper)
    }

    /// The lower mass uncertainty in MeV (`None` if unknown).
    #[must_use]
    pub fn mass_lower(&self) -> Option<f64> {
        finite(self.row.mass_lower)
    }

    /// The decay width Γ in MeV (`None` if unknown; `0` for a stable particle).
    #[must_use]
    pub fn width(&self) -> Option<f64> {
        finite(self.row.width)
    }

    /// The upper width uncertainty in MeV (`None` if unknown).
    #[must_use]
    pub fn width_upper(&self) -> Option<f64> {
        finite(self.row.width_upper)
    }

    /// The lower width uncertainty in MeV (`None` if unknown).
    #[must_use]
    pub fn width_lower(&self) -> Option<f64> {
        finite(self.row.width_lower)
    }

    /// The mean lifetime τ = ħ/Γ in ns (`inf` for a stable particle, `None` if
    /// the width is unknown).
    #[must_use]
    pub fn lifetime(&self) -> Option<f64> {
        self.width().map(|w| HBAR_MEV_NS / w)
    }

    /// The proper decay length cτ in mm (`inf` for a stable particle, `None` if
    /// the width is unknown).
    #[must_use]
    pub fn ctau(&self) -> Option<f64> {
        self.lifetime().map(|t| C_LIGHT_MM_NS * t)
    }

    /// The electric charge in units of `e` (`None` if undefined).
    #[must_use]
    pub fn charge(&self) -> Option<f64> {
        self.pdgid().charge()
    }

    /// Three times the electric charge, in units of `e/3` (`None` if undefined).
    #[must_use]
    pub fn three_charge(&self) -> Option<i32> {
        self.pdgid().three_charge()
    }

    /// The isospin I (`None` if not applicable).
    #[must_use]
    pub fn isospin(&self) -> Option<f64> {
        finite(self.row.isospin)
    }

    /// The total angular momentum J (`None` if undefined).
    #[must_use]
    pub fn j(&self) -> Option<f64> {
        self.pdgid().j()
    }

    /// The parity P.
    #[must_use]
    pub fn parity(&self) -> Parity {
        Parity::from_code(self.row.p)
    }

    /// The C-parity.
    #[must_use]
    pub fn c_parity(&self) -> Parity {
        Parity::from_code(self.row.c)
    }

    /// The G-parity.
    #[must_use]
    pub fn g_parity(&self) -> Parity {
        Parity::from_code(self.row.g)
    }

    /// The boson spin type from `(J, P)` (`NonDefined` for a fermion).
    #[must_use]
    pub fn spin_type(&self) -> SpinType {
        let Some(js) = self.pdgid().j_spin() else {
            return SpinType::NonDefined;
        };
        if js % 2 == 0 {
            return SpinType::NonDefined; // half-integer J → a fermion
        }
        let big_j = ((js - 1) / 2) as usize;
        if big_j <= 2 {
            match self.parity() {
                Parity::Plus => {
                    return [SpinType::Scalar, SpinType::Axial, SpinType::Tensor][big_j]
                }
                Parity::Minus => {
                    return [
                        SpinType::PseudoScalar,
                        SpinType::Vector,
                        SpinType::PseudoTensor,
                    ][big_j]
                }
                Parity::Unknown => {}
            }
        }
        SpinType::Unknown
    }

    /// How well established this entry is (the PDG status code).
    #[must_use]
    pub fn status(&self) -> Status {
        Status::from_code(self.row.status)
    }

    /// The PDG "rank", a hint at how prominent the particle is (`0` = most
    /// common). Used by the PDG for display ordering.
    #[must_use]
    pub fn rank(&self) -> i8 {
        self.row.rank
    }

    /// How the particle relates to its antiparticle.
    #[must_use]
    pub fn anti_flag(&self) -> Inv {
        Inv::from_code(self.row.anti)
    }

    /// Whether the particle is its own antiparticle (e.g. `π⁰`, `Z`).
    #[must_use]
    pub fn is_self_conjugate(&self) -> bool {
        self.anti_flag() == Inv::Same
    }

    /// The antiparticle: the same particle if self-conjugate, otherwise the table
    /// entry with the negated PDG ID (`None` if that entry is not bundled).
    #[must_use]
    pub fn invert(&self) -> Option<Particle> {
        if self.is_self_conjugate() {
            Some(*self)
        } else {
            Particle::from_pdgid(-self.row.id)
        }
    }
}

impl core::fmt::Display for Particle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.row.name)
    }
}

impl core::fmt::Debug for Particle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Particle({}, pdgid={})", self.row.name, self.row.id)
    }
}

impl PartialEq for Particle {
    fn eq(&self, other: &Self) -> bool {
        self.row.id == other.row.id
    }
}

impl Eq for Particle {}
