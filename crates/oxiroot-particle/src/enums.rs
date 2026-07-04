//! Small enums describing particle quantum numbers, mirroring scikit-hep
//! `particle`'s `Parity`, `SpinType`, `Inv`, and `Status`.

/// A parity-like quantum number (P, C, or G): `+1`, `-1`, or unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Parity {
    /// Positive parity, `+1`.
    Plus,
    /// Negative parity, `-1`.
    Minus,
    /// Unknown or not applicable.
    Unknown,
}

impl Parity {
    /// Decode the PDG integer code (`1`, `-1`, else unknown).
    #[must_use]
    pub(crate) fn from_code(code: i8) -> Parity {
        match code {
            1 => Parity::Plus,
            -1 => Parity::Minus,
            _ => Parity::Unknown,
        }
    }

    /// The signed value `+1` / `-1`, or `None` when unknown.
    #[must_use]
    pub fn value(self) -> Option<i32> {
        match self {
            Parity::Plus => Some(1),
            Parity::Minus => Some(-1),
            Parity::Unknown => None,
        }
    }
}

/// The spin type of a boson, from `(J, P)`. `NonDefined` is used for fermions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpinType {
    /// `J = 0`, `P = +1`.
    Scalar,
    /// `J = 0`, `P = -1`.
    PseudoScalar,
    /// `J = 1`, `P = -1`.
    Vector,
    /// `J = 1`, `P = +1`.
    Axial,
    /// `J = 2`, `P = +1`.
    Tensor,
    /// `J = 2`, `P = -1`.
    PseudoTensor,
    /// A boson whose `J` or `P` is unknown.
    Unknown,
    /// Not a boson (a fermion).
    NonDefined,
}

/// How a particle relates to its antiparticle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Inv {
    /// The particle is its own antiparticle (e.g. `π⁰`, `Z`).
    Same,
    /// The antiparticle is written with a bar (e.g. the proton, `Λ`).
    Barred,
    /// The antiparticle differs only by charge sign (e.g. `π⁺` vs `π⁻`).
    ChargeInv,
}

impl Inv {
    pub(crate) fn from_code(code: i8) -> Inv {
        match code {
            0 => Inv::Same,
            2 => Inv::ChargeInv,
            _ => Inv::Barred,
        }
    }
}

/// The PDG status of a table entry (how well established it is).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// Established, in the Particle Physics Booklet summary tables.
    Common,
    /// Well established but omitted from the summary tables to save space.
    Rare,
    /// Omitted because not well established.
    Unsure,
    /// A "further state", poorly established or needing confirmation.
    Further,
    /// Not in the PDT — a non-standard or exotic entry.
    NotInPdt,
}

impl Status {
    pub(crate) fn from_code(code: i8) -> Status {
        match code {
            0 => Status::Common,
            1 => Status::Rare,
            2 => Status::Unsure,
            3 => Status::Further,
            _ => Status::NotInPdt,
        }
    }
}
