#!/usr/bin/env python
"""Generate oxiroot-particle's data table + an oracle test fixture from scikit-hep `particle`."""
import os
from fractions import Fraction
from particle import Particle
from particle.pdgid import functions as F

ROOT = "/Users/mathieuouillon/Documents/tmp/root-rs/crates/oxiroot-particle"
os.makedirs(os.path.join(ROOT, "src"), exist_ok=True)
os.makedirs(os.path.join(ROOT, "tests", "fixtures"), exist_ok=True)

# Restrict the bundled table to the standard particle table (particle2026.csv),
# NOT the ~6000-entry nuclei table that `Particle.all()` now also includes.
from particle import data as _pdata
_data_dir = os.path.dirname(_pdata.__file__)
_csv = os.path.join(_data_dir, "particle2026.csv")

# Keep the BSD-3 attribution file in sync with the installed `particle` package.
import glob
_up_license = glob.glob(
    os.path.join(_data_dir, "..", "..", "particle-*.dist-info", "licenses", "LICENSE")
)
if _up_license:
    header = (
        "The bundled PDG particle table in `src/data.rs` is generated from the\n"
        'scikit-hep "particle" package (https://github.com/scikit-hep/particle), and\n'
        "the `PdgId` decoder is a port of that package's `pdgid.functions`. That\n"
        "package is distributed under the following BSD-3-Clause license, reproduced\n"
        "here as required. The underlying data originates from the Particle Data Group\n"
        "(https://pdg.lbl.gov).\n\n"
        "----------------------------------------------------------------------\n\n"
    )
    with open(os.path.join(ROOT, "LICENSE-3rdparty"), "w") as f:
        f.write(header + open(_up_license[0]).read())
    print("refreshed LICENSE-3rdparty")
STD_IDS = set()
for line in open(_csv):
    line = line.strip()
    if not line or line.startswith("#") or line.startswith("ID,"):
        continue
    STD_IDS.add(int(line.split(",", 1)[0]))
print("standard-table ids:", len(STD_IDS))


def esc(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def f64(x):
    if x is None:
        return "f64::NAN"
    x = float(x)
    if x != x:
        return "f64::NAN"
    if x == float("inf"):
        return "f64::INFINITY"
    if x == float("-inf"):
        return "f64::NEG_INFINITY"
    return repr(x)


def iso(p):
    try:
        return f64(float(p.I)) if p.I is not None else "f64::NAN"
    except Exception:
        return "f64::NAN"


# ---- data.rs : the static particle table -----------------------------------
rows = []
parts = sorted(
    (p for p in Particle.all() if int(p.pdgid) in STD_IDS),
    key=lambda p: (abs(int(p.pdgid)), int(p.pdgid) < 0),
)
for p in parts:
    pid = int(p.pdgid)
    rows.append(
        "    Row { id: %d, name: \"%s\", latex: \"%s\", quarks: \"%s\", "
        "mass: %s, mass_upper: %s, mass_lower: %s, width: %s, width_upper: %s, width_lower: %s, "
        "isospin: %s, g: %d, p: %d, c: %d, anti: %d, rank: %d, status: %d },"
        % (
            pid, esc(p.name), esc(p.latex_name), esc(p.quarks or ""),
            f64(p.mass), f64(p.mass_upper), f64(p.mass_lower),
            f64(p.width), f64(p.width_upper), f64(p.width_lower),
            iso(p), int(p.G), int(p.P), int(p.C), int(p.anti_flag), int(p.rank), int(p.status),
        )
    )

data_rs = (
    "//! The bundled PDG particle table — GENERATED, do not edit by hand.\n"
    "//!\n"
    "//! Produced from the scikit-hep `particle` package's `particle2026.csv`\n"
    "//! (itself from the PDG). Regenerate with `scripts/gen_particle.py`.\n"
    "//! Masses and widths are in MeV; `f64::NAN` marks an unknown value.\n\n"
    "/// One row of the PDG table.\n"
    "#[derive(Clone, Copy, Debug)]\n"
    "pub(crate) struct Row {\n"
    "    pub id: i32,\n    pub name: &'static str,\n    pub latex: &'static str,\n"
    "    pub quarks: &'static str,\n    pub mass: f64,\n    pub mass_upper: f64,\n"
    "    pub mass_lower: f64,\n    pub width: f64,\n    pub width_upper: f64,\n"
    "    pub width_lower: f64,\n    pub isospin: f64,\n    pub g: i8,\n    pub p: i8,\n"
    "    pub c: i8,\n    pub anti: i8,\n    pub rank: i8,\n    pub status: i8,\n}\n\n"
    "/// Every particle in the table, sorted by |pdgid| then sign.\n"
    "#[rustfmt::skip]\n"
    "pub(crate) static PARTICLES: &[Row] = &[\n" + "\n".join(rows) + "\n];\n"
)
open(os.path.join(ROOT, "src", "data.rs"), "w").write(data_rs)
print("wrote src/data.rs:", len(parts), "particles")

# ---- oracle fixture : PdgID predicate + numeric expectations ---------------
PREDS = [
    ("is_valid", F.is_valid), ("is_quark", F.is_quark), ("is_sm_quark", F.is_sm_quark),
    ("is_lepton", F.is_lepton), ("is_sm_lepton", F.is_sm_lepton), ("is_meson", F.is_meson),
    ("is_baryon", F.is_baryon), ("is_hadron", F.is_hadron), ("is_diquark", F.is_diquark),
    ("is_nucleus", F.is_nucleus), ("is_pentaquark", F.is_pentaquark),
    ("is_gauge_boson_or_higgs", F.is_gauge_boson_or_higgs),
    ("is_sm_gauge_boson_or_higgs", F.is_sm_gauge_boson_or_higgs),
    ("is_generator_specific", F.is_generator_specific),
    ("is_special_particle", F.is_special_particle), ("is_r_hadron", F.is_Rhadron),
    ("is_qball", F.is_Qball), ("is_dyon", F.is_dyon), ("is_susy", F.is_SUSY),
    ("is_technicolor", F.is_technicolor),
    ("is_excited_quark_or_lepton", F.is_excited_quark_or_lepton),
    ("has_down", F.has_down), ("has_up", F.has_up), ("has_strange", F.has_strange),
    ("has_charm", F.has_charm), ("has_bottom", F.has_bottom), ("has_top", F.has_top),
    ("has_fundamental_anti", F.has_fundamental_anti),
]
assert len(PREDS) <= 32

# Test ids: every table id + a curated set of tricky/exotic ids.
synthetic = [
    0, 1, 2, 5, 6, 11, 12, 21, 22, 23, 24, 25, 37, 39, 41,
    2101, 2103, 3201, 5401,                       # diquarks
    1000010020, 1000020040, 1000060120, 1000922350,  # nuclei (d, alpha, C12, U235)
    -1000010020,                                    # anti-deuteron
    1000021, 1000022, 1000024, 2000011, 1000039,    # SUSY
    9221132, 9331122,                               # pentaquarks
    100051, 100061,                                 # (near Q-ball ranges)
    4000011, 4000001,                               # excited
    9900012, 9900024,                               # technicolor-ish
    3000101, 3000111, 3000113, 3000201,             # technicolor (CH100 wrap path)
    88, 99, 998, 999, 20022,                        # generator-specific
    901, 902, 915, 930, 1901, 1930, 2901, 2930, 3901, 3930,  # generator-specific bands (CH100 wrap)
    1000993, 1009213, 1093214,                      # R-hadrons
    42, 4110010,                                    # dyon-ish / misc
    130, 310, 2110, 2210, 210, 110, 990,
]
ids = sorted(set([int(p.pdgid) for p in parts] + synthetic))


def optn(x):
    return "NONE" if x is None else str(int(x))


def numopt(fn, pid):
    try:
        v = fn(pid)
        return None if v is None else int(v)
    except Exception:
        return None


cases = []
for pid in ids:
    bits = 0
    for i, (_, fn) in enumerate(PREDS):
        try:
            if fn(pid):
                bits |= (1 << i)
        except Exception:
            pass  # a raising predicate is treated as False (Rust returns false too)
    tc = numopt(F.three_charge, pid)
    js = numopt(F.j_spin, pid)
    ss = numopt(F.s_spin, pid)
    ls = numopt(F.l_spin, pid)
    a = numopt(F.A, pid)
    z = numopt(F.Z, pid)
    cases.append(
        "    Case { id: %d, preds: 0x%08x, three_charge: %s, j_spin: %s, s_spin: %s, l_spin: %s, a: %s, z: %s },"
        % (pid, bits, optn(tc), optn(js), optn(ss), optn(ls), optn(a), optn(z))
    )

pred_names = ", ".join('"%s"' % n for n, _ in PREDS)
oracle = (
    "//! GENERATED oracle fixture — expected PDG-ID decodings from scikit-hep `particle`.\n"
    "//! Regenerate with `scripts/gen_particle.py`. `NONE` marks a Python `None`.\n\n"
    "pub const NONE: i64 = i64::MIN;\n\n"
    "/// Predicate names, in the bit order of `Case::preds` (bit 0 = first name).\n"
    "#[rustfmt::skip]\n"
    "pub static PRED_NAMES: &[&str] = &[" + pred_names + "];\n\n"
    "#[derive(Clone, Copy)]\n"
    "pub struct Case {\n    pub id: i32,\n    pub preds: u32,\n    pub three_charge: i64,\n"
    "    pub j_spin: i64,\n    pub s_spin: i64,\n    pub l_spin: i64,\n    pub a: i64,\n    pub z: i64,\n}\n\n"
    "#[rustfmt::skip]\n"
    "pub static CASES: &[Case] = &[\n" + "\n".join(cases) + "\n];\n"
)
open(os.path.join(ROOT, "tests", "fixtures", "oracle_cases.rs"), "w").write(oracle)
print("wrote tests/oracle_cases.rs:", len(ids), "ids")

# ---- Particle-level derived expectations (mass/charge/lifetime/ctau) --------
sample = [11, -11, 13, 22, 23, 24, 25, 111, 211, -211, 130, 310, 321, 2212, 2112,
          443, 521, 531, 5122, 15, 16, 3122, 411, 421]
pcases = []
for pid in sample:
    p = Particle.from_pdgid(pid)
    life = p.lifetime
    ctau = p.ctau
    pcases.append(
        '    PCase { id: %d, name: "%s", mass: %s, three_charge: %s, lifetime_ns: %s, ctau_mm: %s },'
        % (pid, esc(p.name), f64(p.mass),
           "NONE_I" if p.three_charge is None else str(int(p.three_charge)),
           f64(life), f64(ctau))
    )
pdata = (
    "//! GENERATED — expected Particle-level derived values from scikit-hep `particle`.\n\n"
    "pub const NONE_I: i64 = i64::MIN;\n\n"
    "pub struct PCase {\n    pub id: i32,\n    pub name: &'static str,\n    pub mass: f64,\n"
    "    pub three_charge: i64,\n    pub lifetime_ns: f64,\n    pub ctau_mm: f64,\n}\n\n"
    "#[rustfmt::skip]\n"
    "pub static PARTICLE_CASES: &[PCase] = &[\n" + "\n".join(pcases) + "\n];\n"
)
open(os.path.join(ROOT, "tests", "fixtures", "particle_cases.rs"), "w").write(pdata)
print("wrote tests/particle_cases.rs:", len(sample), "particles")
