//! The PDG Monte Carlo particle numbering scheme — decode a signed integer ID
//! into what it *is* (lepton? meson? baryon? nucleus?) and its properties
//! (charge, spin, quark content) purely from the digits, with no data table.
//!
//! This is a faithful port of scikit-hep `particle`'s `pdgid.functions`, and is
//! verified against it for the whole standard table plus a spread of exotic IDs
//! (nuclei, SUSY, R-hadrons, di-quarks, pentaquarks, dyons, …) in
//! `tests/oracle.rs`.

// Digit positions in the 10-digit PDG ID `n Nr Nl Nq1 Nq2 Nq3 Nj` (1 = least
// significant), named as in the PDG scheme.
const NJ: u32 = 1;
const NQ3: u32 = 2;
const NQ2: u32 = 3;
const NQ1: u32 = 4;
const NL: u32 = 5;
const NR: u32 = 6;
const N: u32 = 7;
const N8: u32 = 8;
const N9: u32 = 9;
const N10: u32 = 10;

/// Three times the charge of fundamental IDs 1..=100 (index `id - 1`), in units
/// of `e/3`. Ported verbatim from scikit-hep's `_CH100`.
#[rustfmt::skip]
const CH100: [i64; 100] = [
    -1, 2, -1, 2, -1, 2, -1, 2, 0, 0, -3, 0, -3, 0, -3, 0, -3, 0, 0, 0,
    0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 3, 0, 0, 0,
    0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 3, 6, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// `CH100` indexed by `q - 1`, reproducing Python's negative-index wrap so a
/// zero digit (`q == 0`) reads the last element (`CH100[99] == 0`), exactly as
/// scikit-hep's `_CH100[q - 1]` does. Without this, `q == 0` would index
/// `usize::MAX` and panic.
fn ch100(q: i64) -> i64 {
    CH100[(q - 1).rem_euclid(100) as usize]
}

/// Fundamental IDs that are their own CP conjugate (no distinct antiparticle).
const CP_CONJUGATES: [i64; 11] = [21, 22, 23, 25, 32, 33, 35, 36, 39, 40, 43];
/// Fundamental IDs in 1..=79 that are unassigned in the scheme.
#[rustfmt::skip]
const UNASSIGNED: [i64; 45] = [
    9, 10, 19, 20, 26, 27, 28, 29, 30, 31, 45, 46, 47, 48, 49, 50, 51, 52, 53,
    54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72,
    73, 74, 75, 76, 77, 78, 79,
];

/// A particle identifier in the [PDG Monte Carlo numbering scheme][pdg] — a
/// signed integer whose digits encode the particle's nature and quantum numbers.
///
/// [pdg]: https://pdg.lbl.gov/current/reviews/rpp2023-rev-monte-carlo-numbering.pdf
///
/// The decoding methods answer questions about *any* ID without a lookup table
/// (e.g. `PdgId::new(2212).is_baryon()` is `true` for a proton). For the physical
/// properties of a *named* particle (mass, width, …) use
/// [`Particle`](crate::Particle).
///
/// # Examples
/// ```
/// use oxiroot_particle::PdgId;
/// let electron = PdgId::new(11);
/// assert!(electron.is_lepton());
/// assert_eq!(electron.three_charge(), Some(-3)); // -1 e, in units of e/3
/// assert_eq!(PdgId::new(2212).charge(), Some(1.0)); // the proton
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PdgId(pub i32);

impl PdgId {
    /// Wrap a raw signed PDG ID.
    #[must_use]
    pub const fn new(id: i32) -> Self {
        PdgId(id)
    }

    /// The raw signed integer ID.
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        self.0
    }

    /// The absolute value of the ID (the particle regardless of its sign).
    #[must_use]
    pub fn abspid(self) -> i32 {
        (self.0 as i64).unsigned_abs().min(i32::MAX as u64) as i32
    }

    fn aid(self) -> i64 {
        (self.0 as i64).abs()
    }

    /// The `loc`-th decimal digit of `|id|` (1 = least significant).
    fn digit(self, loc: u32) -> i64 {
        self.aid() % 10i64.pow(loc) / 10i64.pow(loc - 1)
    }

    fn extra_bits(self) -> i64 {
        self.aid() / 10_000_000
    }

    fn fundamental_id(self) -> i64 {
        if self.extra_bits() > 0 {
            return 0;
        }
        let a = self.aid();
        if a <= 100 {
            return a;
        }
        if self.digit(NQ2) == 0 && self.digit(NQ1) == 0 {
            return a % 10000;
        }
        0
    }

    /// Whether the ID is valid in the numbering scheme.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.is_gauge_boson_or_higgs()
            || self.fundamental_id() != 0
            || self.is_meson()
            || self.is_baryon()
            || self.is_pentaquark()
            || self.is_susy()
            || self.is_r_hadron()
            || self.is_dyon()
            || self.is_diquark()
            || self.is_generator_specific()
            || self.is_technicolor()
            || self.is_excited_quark_or_lepton()
            || (self.extra_bits() > 0 && (self.is_qball() || self.is_nucleus()))
    }

    /// A quark (`|id|` in 1..=8, including the hypothetical 4th generation).
    #[must_use]
    pub fn is_quark(self) -> bool {
        (1..=8).contains(&self.aid())
    }

    /// A Standard-Model quark (`|id|` in 1..=6).
    #[must_use]
    pub fn is_sm_quark(self) -> bool {
        (1..=6).contains(&self.aid())
    }

    /// A lepton (`|id|` in 11..=18).
    #[must_use]
    pub fn is_lepton(self) -> bool {
        (11..=18).contains(&self.aid())
    }

    /// A Standard-Model lepton (`|id|` in 11..=16).
    #[must_use]
    pub fn is_sm_lepton(self) -> bool {
        (11..=16).contains(&self.aid())
    }

    /// A neutrino (`|id|` in {12, 14, 16, 18}). *(Convenience; not a PDG function.)*
    #[must_use]
    pub fn is_neutrino(self) -> bool {
        matches!(self.aid(), 12 | 14 | 16 | 18)
    }

    /// A meson.
    #[must_use]
    pub fn is_meson(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        let aid = self.aid();
        if aid <= 100 {
            return false;
        }
        let fid = self.fundamental_id();
        if fid > 0 && fid <= 100 {
            return false;
        }
        if matches!(aid, 130 | 210 | 310) {
            return true;
        }
        if matches!(aid, 150 | 350 | 510 | 530) {
            return true;
        }
        if matches!(self.0, 110 | 990 | 9990) {
            return true;
        }
        if matches!(aid, 998 | 999) {
            return false;
        }
        if self.digit(NJ) > 0 && self.digit(NQ3) > 0 && self.digit(NQ2) > 0 && self.digit(NQ1) == 0
        {
            return self.digit(NQ3) != self.digit(NQ2) || self.0 >= 0;
        }
        false
    }

    /// A baryon.
    #[must_use]
    pub fn is_baryon(self) -> bool {
        let aid = self.aid();
        if aid <= 100 {
            return false;
        }
        if aid == 1000000010 || aid == 1000010010 {
            return true;
        }
        if self.extra_bits() > 0 {
            return false;
        }
        let fid = self.fundamental_id();
        if fid > 0 && fid <= 100 {
            return false;
        }
        if matches!(aid, 2110 | 2210) {
            return true;
        }
        self.digit(NJ) > 0 && self.digit(NQ3) > 0 && self.digit(NQ2) > 0 && self.digit(NQ1) > 0
    }

    /// A hadron (meson, baryon, or R-hadron).
    #[must_use]
    pub fn is_hadron(self) -> bool {
        if self.aid() == 1000000010 || self.aid() == 1000010010 {
            return true;
        }
        if self.extra_bits() > 0 {
            return false;
        }
        self.is_meson() || self.is_baryon() || self.is_r_hadron()
    }

    /// A di-quark.
    #[must_use]
    pub fn is_diquark(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        if self.aid() <= 100 {
            return false;
        }
        let fid = self.fundamental_id();
        if fid > 0 && fid <= 100 {
            return false;
        }
        self.digit(NJ) > 0 && self.digit(NQ3) == 0 && self.digit(NQ2) > 0 && self.digit(NQ1) > 0
    }

    /// An atomic nucleus (including the proton and neutron).
    #[must_use]
    pub fn is_nucleus(self) -> bool {
        let aid = self.aid();
        if aid == 2112 || aid == 2212 {
            return true;
        }
        if self.digit(N10) == 1 && self.digit(N9) == 0 {
            if let (Some(a), Some(z)) = (self.a(), self.z()) {
                return a >= z.unsigned_abs() as i32;
            }
        }
        false
    }

    /// A pentaquark.
    #[must_use]
    pub fn is_pentaquark(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        if self.digit(N) != 9 {
            return false;
        }
        if matches!(self.digit(NR), 9 | 0) {
            return false;
        }
        if matches!(self.digit(NJ), 0 | 9) || self.digit(NL) == 0 {
            return false;
        }
        if self.digit(NQ1) == 0 || self.digit(NQ2) == 0 || self.digit(NQ3) == 0 {
            return false;
        }
        if self.digit(NQ2) > self.digit(NQ1) {
            return false;
        }
        if self.digit(NQ1) > self.digit(NL) {
            return false;
        }
        self.digit(NL) <= self.digit(NR)
    }

    /// A gauge boson or Higgs (`|id|` in 21..=40).
    #[must_use]
    pub fn is_gauge_boson_or_higgs(self) -> bool {
        (21..=40).contains(&self.aid())
    }

    /// A Standard-Model gauge boson or Higgs (g, γ, Z, W, H).
    #[must_use]
    pub fn is_sm_gauge_boson_or_higgs(self) -> bool {
        if self.aid() == 24 {
            return true;
        }
        (21..=25).contains(&self.0)
    }

    /// A generator-specific pseudo-particle or code.
    #[must_use]
    pub fn is_generator_specific(self) -> bool {
        let aid = self.aid();
        matches!(aid,
            81..=100 | 901..=930 | 1901..=1930 | 2901..=2930 | 3901..=3930
            | 998 | 999 | 20022 | 480000000)
    }

    /// A special particle (graviton, dark-matter placeholders, …).
    #[must_use]
    pub fn is_special_particle(self) -> bool {
        matches!(self.0, 39 | 41 | 42 | 51 | 52 | 53 | 110 | 990 | 9990)
            || self.is_generator_specific()
    }

    /// An R-hadron (a bound state containing a SUSY particle).
    #[must_use]
    pub fn is_r_hadron(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        if self.digit(N) != 1 {
            return false;
        }
        if self.digit(NR) != 0 {
            return false;
        }
        if self.is_susy() {
            return false;
        }
        self.digit(NQ2) != 0 && self.digit(NQ3) != 0 && self.digit(NJ) != 0
    }

    /// A Q-ball or similar exotic state with unusual charge.
    #[must_use]
    pub fn is_qball(self) -> bool {
        if self.extra_bits() != 1 {
            return false;
        }
        if self.digit(N) != 0 {
            return false;
        }
        if self.digit(NR) != 0 {
            return false;
        }
        if self.aid() / 10 % 10000 == 0 {
            return false;
        }
        self.digit(NJ) == 0
    }

    /// A magnetic monopole or dyon.
    #[must_use]
    pub fn is_dyon(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        if self.digit(N) != 4 {
            return false;
        }
        if self.digit(NR) != 1 {
            return false;
        }
        if !matches!(self.digit(NL), 1 | 2) {
            return false;
        }
        if self.digit(NQ3) == 0 {
            return false;
        }
        self.digit(NJ) == 0
    }

    /// A supersymmetric particle.
    #[must_use]
    pub fn is_susy(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        if !matches!(self.digit(N), 1 | 2) {
            return false;
        }
        if self.digit(NR) != 0 {
            return false;
        }
        self.fundamental_id() != 0
    }

    /// A technicolor state.
    #[must_use]
    pub fn is_technicolor(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        self.digit(N) == 3
    }

    /// An excited (composite) quark or lepton.
    #[must_use]
    pub fn is_excited_quark_or_lepton(self) -> bool {
        if self.extra_bits() > 0 {
            return false;
        }
        if self.fundamental_id() == 0 {
            return false;
        }
        self.digit(N) == 4 && self.digit(NR) == 0
    }

    /// Whether the particle contains a down quark.
    #[must_use]
    pub fn has_down(self) -> bool {
        self.has_quark_q(1)
    }
    /// Whether the particle contains an up quark.
    #[must_use]
    pub fn has_up(self) -> bool {
        self.has_quark_q(2)
    }
    /// Whether the particle contains a strange quark.
    #[must_use]
    pub fn has_strange(self) -> bool {
        self.has_quark_q(3)
    }
    /// Whether the particle contains a charm quark.
    #[must_use]
    pub fn has_charm(self) -> bool {
        self.has_quark_q(4)
    }
    /// Whether the particle contains a bottom quark.
    #[must_use]
    pub fn has_bottom(self) -> bool {
        self.has_quark_q(5)
    }
    /// Whether the particle contains a top quark.
    #[must_use]
    pub fn has_top(self) -> bool {
        self.has_quark_q(6)
    }

    /// Whether a fundamental particle has a distinct antiparticle.
    #[must_use]
    pub fn has_fundamental_anti(self) -> bool {
        let fid = self.fundamental_id();
        if (81..=100).contains(&fid) {
            return matches!(fid, 82 | 84 | 85 | 86 | 87);
        }
        if (1..=79).contains(&fid) && !CP_CONJUGATES.contains(&fid) {
            return !UNASSIGNED.contains(&fid);
        }
        false
    }

    fn has_quark_q(self, q: i64) -> bool {
        if self.is_nucleus() {
            if q == 1 || q == 2 {
                return true;
            }
            if q == 3 && self.0 != 2112 && self.0 != 2212 {
                return self.digit(N8) > 0;
            }
        }
        if self.is_dyon() {
            return false;
        }
        if self.extra_bits() > 0 {
            return false;
        }
        if self.fundamental_id() > 0 {
            return false;
        }
        if self.is_r_hadron() {
            let mut iz = 7i64;
            for loc in (2..=6).rev() {
                if self.digit(loc) == 0 {
                    iz = loc as i64;
                } else if loc as i64 != iz - 1 && self.digit(loc) == q {
                    return true;
                }
            }
            return false;
        }
        if self.digit(NQ3) == q || self.digit(NQ2) == q || self.digit(NQ1) == q {
            return true;
        }
        if self.is_pentaquark() && (self.digit(NL) == q || self.digit(NR) == q) {
            return true;
        }
        false
    }

    /// Three times the electric charge, in units of `e/3` (`None` for an invalid
    /// ID). Integer, so it is exact for fractionally-charged quarks.
    #[must_use]
    pub fn three_charge(self) -> Option<i32> {
        if !self.is_valid() {
            return None;
        }
        let aid = self.aid();
        let q1 = self.digit(NQ1);
        let q2 = self.digit(NQ2);
        let q3 = self.digit(NQ3);
        let sid = self.fundamental_id();
        let mut charge: Option<i64> = None;

        if self.extra_bits() > 0 {
            if self.is_nucleus() {
                return self.z().map(|z| 3 * z);
            }
            if self.is_qball() {
                charge = Some(3 * (aid / 10 % 10000));
            } else {
                return None;
            }
        } else if self.is_dyon() {
            let mut c = 3 * (aid / 10 % 1000);
            if self.digit(NL) == 2 {
                c = -c;
            }
            charge = Some(c);
        } else if sid > 0 && sid <= 100 {
            let mut c = ch100(sid);
            if matches!(
                aid,
                1000017 | 1000018 | 1000034 | 1000052 | 1000053 | 1000054
            ) {
                c = 0;
            }
            if matches!(aid, 5100061 | 5100062) {
                c = 6;
            }
            charge = Some(c);
        } else if self.digit(NJ) == 0 {
            return Some(0);
        } else if q1 == 0 || (self.is_r_hadron() && q1 == 9) {
            charge = Some(if q2 == 3 || q2 == 5 {
                ch100(q3) - ch100(q2)
            } else {
                ch100(q2) - ch100(q3)
            });
        } else if q3 == 0 {
            charge = Some(ch100(q2) + ch100(q1));
        } else if self.is_baryon() || (self.is_r_hadron() && self.digit(NL) == 9) {
            charge = Some(ch100(q3) + ch100(q2) + ch100(q1));
        }

        let mut charge = charge?;
        if self.0 < 0 {
            charge = -charge;
        }
        Some(charge as i32)
    }

    /// The electric charge in units of `e` (`None` for an invalid ID).
    #[must_use]
    pub fn charge(self) -> Option<f64> {
        let tc = self.three_charge()? as f64;
        Some(if self.is_qball() { tc / 30.0 } else { tc / 3.0 })
    }

    /// The total spin multiplicity `2J + 1` (`None` if undefined).
    #[must_use]
    pub fn j_spin(self) -> Option<i32> {
        if !self.is_valid() {
            return None;
        }
        let fund = self.fundamental_id();
        if fund > 0 {
            if self.is_susy() {
                if fund < 17 {
                    return Some(1);
                }
                if (21..38).contains(&fund) {
                    return Some(2);
                }
                if fund == 39 {
                    return Some(4);
                }
            } else {
                if fund < 7 {
                    return Some(2);
                }
                if fund == 9 {
                    return Some(3);
                }
                if (11..17).contains(&fund) {
                    return Some(2);
                }
                if (21..25).contains(&fund) {
                    return Some(3);
                }
                if fund == 25 {
                    return Some(1);
                }
                return None;
            }
        }
        if self.aid() == 1000000010 || self.aid() == 1000010010 {
            return Some(2);
        }
        if self.extra_bits() > 0 {
            return None;
        }
        if self.0 == 130 || self.0 == 310 {
            return Some(1);
        }
        Some((self.aid() % 10) as i32)
    }

    /// The total angular momentum `J` (`None` if undefined).
    #[must_use]
    pub fn j(self) -> Option<f64> {
        self.j_spin().map(|v| (v - 1) as f64 / 2.0)
    }

    fn big_s(self) -> Option<i32> {
        if !self.is_meson() {
            return None;
        }
        if self.aid() / 1_000_000 % 10 == 9 {
            return None;
        }
        let nl = self.aid() / 10000 % 10;
        let js = self.aid() % 10;
        if js != 1 && js < 3 {
            return Some(0);
        }
        Some(match nl {
            0 => i32::from(js != 1),
            1 => i32::from(js == 1),
            2 | 3 => i32::from(js >= 3),
            _ => 0,
        })
    }

    /// The spin multiplicity `2S + 1` of a meson (`None` otherwise).
    #[must_use]
    pub fn s_spin(self) -> Option<i32> {
        self.big_s().map(|s| 2 * s + 1)
    }

    fn big_l(self) -> Option<i32> {
        if !self.is_meson() {
            return None;
        }
        if self.aid() / 1_000_000 % 10 == 9 {
            return None;
        }
        let nl = self.aid() / 10000 % 10;
        let js = self.aid() % 10;
        Some(match (nl, js) {
            (0, 1 | 3) => 0,
            (0, 5) | (1, 1 | 3) | (2, 3) => 1,
            (0, 7) | (1, 5) | (2, 5) | (3, 3) => 2,
            (0, 9) | (1, 7) | (2, 7) | (3, 5) => 3,
            (1, 9) | (2, 9) | (3, 7) => 4,
            (3, 9) => 5,
            _ => 0,
        })
    }

    /// The orbital angular momentum multiplicity `2L + 1` of a meson (`None`
    /// otherwise).
    #[must_use]
    pub fn l_spin(self) -> Option<i32> {
        self.big_l().map(|l| 2 * l + 1)
    }

    /// The atomic mass number `A` of a nucleus (`None` otherwise).
    #[must_use]
    pub fn a(self) -> Option<i32> {
        let aid = self.aid();
        if aid == 2112 || aid == 2212 {
            return Some(1);
        }
        if self.digit(N10) != 1 || self.digit(N9) != 0 {
            return None;
        }
        Some((aid / 10 % 1000) as i32)
    }

    /// The atomic number `Z` of a nucleus (`None` otherwise).
    #[must_use]
    pub fn z(self) -> Option<i32> {
        let aid = self.aid();
        if aid == 2212 {
            return Some(self.0 / 2212);
        }
        if aid == 2112 {
            return Some(0);
        }
        if self.digit(N10) != 1 || self.digit(N9) != 0 {
            return None;
        }
        let sign = self.0.signum() as i64;
        Some((aid / 10000 % 1000 * sign) as i32)
    }
}

impl From<i32> for PdgId {
    fn from(id: i32) -> Self {
        PdgId(id)
    }
}

impl From<PdgId> for i32 {
    fn from(id: PdgId) -> Self {
        id.0
    }
}

impl core::fmt::Debug for PdgId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PdgId({})", self.0)
    }
}

impl core::fmt::Display for PdgId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::PdgId;

    /// Regression: `three_charge`/`charge` must not panic on generator-specific
    /// or technicolor IDs, where a zero quark digit makes `q - 1` negative and
    /// `CH100` must wrap (as scikit-hep's Python negative indexing does).
    #[test]
    fn charge_does_not_panic_on_wrap_ids() {
        // These are all valid and were index-out-of-bounds panics before the fix.
        assert_eq!(PdgId::new(901).three_charge(), Some(0));
        assert_eq!(PdgId::new(3000101).three_charge(), Some(-1));
        assert_eq!(PdgId::new(2901).three_charge(), Some(2));
        for id in [901, 902, 915, 930, 1901, 2901, 3901, 3000101, 3000111] {
            let p = PdgId::new(id);
            assert!(p.is_valid());
            let _ = p.three_charge(); // must not panic
            let _ = p.charge(); // wraps three_charge — must not panic
        }
    }
}
