//! Interop driver for the round-trip CI: writes a canonical histogram + RNTuple
//! for an external ROOT reader (uproot / ROOT C++) to verify, and reads the
//! files an external ROOT writer produced, asserting the same canonical values.
//!
//!   cargo run -p oxiroot --example interop -- write <dir>   # Rust → ROOT
//!   cargo run -p oxiroot --example interop -- read  <dir>   # ROOT → Rust
//!
//! Canonical dataset (both sides agree):
//!   - TH1D "h": 4 bins over [0, 4), in-range bin contents [1, 2, 3, 4].
//!   - RNTuple "ntpl": field x = int32 [1,2,3,4,5], y = double [1.5,2.5,3.5,4.5,5.5].
//!   - TTree "Tree": ti = int32 [1..5], tf = double [1.5..5.5], tv = double[3]
//!     fixed array, ts = string, tj = jagged double (auto count ntj),
//!     tw = std::vector<double> (TBranchElement), th = split std::vector<Hit>
//!     (Hit = {float x; float y; int id;}) read back as th.x/th.y/th.id.

use std::path::Path;
use std::process::exit;

use oxiroot::prelude::*;

/// In-range bin contents of the canonical histogram.
const HIST_BINS: [f64; 4] = [1.0, 2.0, 3.0, 4.0];
/// Canonical RNTuple columns.
const NTPL_X: [i32; 5] = [1, 2, 3, 4, 5];
const NTPL_Y: [f64; 5] = [1.5, 2.5, 3.5, 4.5, 5.5];
/// Canonical TTree fixed-array (`tv`) and string (`ts`) columns.
const TREE_TV: [[f64; 3]; 5] = [
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0],
    [7.0, 8.0, 9.0],
    [10.0, 11.0, 12.0],
    [13.0, 14.0, 15.0],
];
const TREE_TS: [&str; 5] = ["a", "bb", "ccc", "dddd", "eeeee"];
/// Canonical TTree jagged (variable-length) column.
const TREE_TJ: [&[f64]; 5] = [&[1.0], &[2.0, 3.0], &[], &[4.0, 5.0, 6.0], &[7.0]];
/// Canonical TTree `std::vector<double>` (TBranchElement) column.
const TREE_TW: [&[f64]; 5] = [
    &[10.0, 20.0],
    &[],
    &[30.0],
    &[40.0, 50.0],
    &[60.0, 70.0, 80.0],
];
/// Canonical split `std::vector<Hit>` branch `th` (Hit = {float x; float y; int
/// id;}), exposed by ROOT as the per-member sub-branches `th.x`/`th.y`/`th.id`.
/// Per-entry element counts [1, 0, 2, 1, 3] (includes an empty entry).
const TREE_TH_X: [&[f32]; 5] = [&[1.0], &[], &[2.0, 3.0], &[4.0], &[5.0, 6.0, 7.0]];
const TREE_TH_Y: [&[f32]; 5] = [&[1.5], &[], &[2.5, 3.5], &[4.5], &[5.5, 6.5, 7.5]];
const TREE_TH_ID: [&[i32]; 5] = [&[1], &[], &[2, 3], &[4], &[5, 6, 7]];
/// Canonical oracle-written TTree "otree" (ROOT/uproot → Rust): a scalar (`oi`),
/// a jagged double (`oj`), a string (`os`), and a `std::vector<double>` (`ov`).
/// uproot cannot write `std::vector`, so `ov` is present only in the ROOT C++
/// oracle and is read back only when the branch exists.
const OTREE_OI: [i32; 3] = [10, 11, 12];
const OTREE_OJ: [&[f64]; 3] = [&[1.0, 2.0], &[], &[3.0]];
const OTREE_OS: [&str; 3] = ["x", "yy", "zzz"];
const OTREE_OV: [&[f64]; 3] = [&[1.0], &[2.0, 3.0], &[]];

fn canonical_hist() -> TH1 {
    let mut h = TH1::new("h", "interop", 4, 0.0, 4.0);
    // Fill bin i (0-based) with i+1 entries at its center, giving contents
    // [1, 2, 3, 4] without relying on direct field mutation.
    for (i, &count) in HIST_BINS.iter().enumerate() {
        let center = i as f64 + 0.5;
        for _ in 0..(count as usize) {
            h.fill(center);
        }
    }
    h
}

fn write(dir: &Path) -> Result<()> {
    let h = canonical_hist();
    h.write_root(dir.join("rust_hist.root"), Compression::None)?;

    let fields = vec![
        Field::i32("x", NTPL_X.to_vec()),
        Field::f64("y", NTPL_Y.to_vec()),
    ];
    write_rntuple_file(
        dir.join("rust_ntuple.root"),
        "ntpl",
        &fields,
        Compression::None,
    )?;

    write_tree_file(
        dir.join("rust_tree.root"),
        "Tree",
        &[
            Branch::i32("ti", NTPL_X.to_vec()),
            Branch::f64("tf", NTPL_Y.to_vec()),
            Branch::vec_f64("tv", TREE_TV.iter().map(|r| r.to_vec()).collect()),
            Branch::strings("ts", TREE_TS.iter().map(|s| s.to_string()).collect()),
            Branch::jagged_f64("tj", TREE_TJ.iter().map(|r| r.to_vec()).collect()),
            Branch::vector_f64("tw", TREE_TW.iter().map(|r| r.to_vec()).collect()),
            Branch::split_vector(
                "th",
                "Hit",
                vec![
                    SplitMember::f32("x", TREE_TH_X.iter().map(|r| r.to_vec()).collect()),
                    SplitMember::f32("y", TREE_TH_Y.iter().map(|r| r.to_vec()).collect()),
                    SplitMember::i32("id", TREE_TH_ID.iter().map(|r| r.to_vec()).collect()),
                ],
            ),
        ],
        Compression::None,
    )?;
    println!(
        "wrote rust_hist.root + rust_ntuple.root + rust_tree.root to {}",
        dir.display()
    );
    Ok(())
}

fn read(dir: &Path) -> Result<()> {
    // Histogram written by the ROOT oracle.
    let f = RFile::open(dir.join("oracle_hist.root"))?;
    let h = TH1::read_root(&f, "h")?;
    assert_close("hist bin contents", h.values(), &HIST_BINS);
    println!("read oracle_hist.root — bin contents match");

    // TTree written by the oracle (both ROOT C++ and uproot produce it).
    read_oracle_tree(dir)?;

    // RNTuple written by the ROOT oracle. uproot's RNTuple writer is
    // experimental, so the uproot job omits this file; only the ROOT C++ job
    // produces it. Skip the check when it is absent.
    let ntuple_path = dir.join("oracle_ntuple.root");
    if !ntuple_path.exists() {
        println!("oracle_ntuple.root absent — skipping RNTuple read check");
        return Ok(());
    }
    let f = RFile::open(ntuple_path)?;
    let ntpl = RNTuple::open(&f, "ntpl")?;
    match ntpl.read_field(&f, "x")? {
        FieldValues::I32(v) => assert_eq_or_die("ntuple x", &v, &NTPL_X),
        other => die(&format!("ntuple x: expected I32, got {other:?}")),
    }
    match ntpl.read_field(&f, "y")? {
        FieldValues::F64(v) => assert_close("ntuple y", &v, &NTPL_Y),
        other => die(&format!("ntuple y: expected F64, got {other:?}")),
    }
    println!("read oracle_ntuple.root — values match");
    Ok(())
}

fn read_oracle_tree(dir: &Path) -> Result<()> {
    let f = RFile::open(dir.join("oracle_tree.root"))?;
    let t = TTree::open(&f, "otree")?;
    match t.read_branch(&f, "oi")? {
        BranchValues::I32(v) => assert_eq_or_die("otree oi", &v, &OTREE_OI),
        other => die(&format!("otree oi: expected I32, got {other:?}")),
    }
    match t.read_branch(&f, "oj")? {
        BranchValues::VecF64(v) => assert_nested("otree oj", &v, &OTREE_OJ),
        other => die(&format!("otree oj: expected VecF64, got {other:?}")),
    }
    match t.read_branch(&f, "os")? {
        BranchValues::Str(v) => {
            let got: Vec<&str> = v.iter().map(String::as_str).collect();
            assert_eq_or_die("otree os", &got, &OTREE_OS);
        }
        other => die(&format!("otree os: expected Str, got {other:?}")),
    }
    // The std::vector<double> branch is written only by the ROOT C++ oracle.
    if t.branch_names().contains(&"ov") {
        match t.read_branch(&f, "ov")? {
            BranchValues::VecF64(v) => assert_nested("otree ov", &v, &OTREE_OV),
            other => die(&format!("otree ov: expected VecF64, got {other:?}")),
        }
    }
    println!("read oracle_tree.root — values match");
    Ok(())
}

fn assert_close(what: &str, got: &[f64], want: &[f64]) {
    if got.len() != want.len() || got.iter().zip(want).any(|(a, b)| (a - b).abs() > 1e-9) {
        die(&format!("{what}: got {got:?}, want {want:?}"));
    }
}

/// Assert a nested `Vec<Vec<f64>>` matches the canonical jagged/vector rows.
fn assert_nested(what: &str, got: &[Vec<f64>], want: &[&[f64]]) {
    let ok = got.len() == want.len()
        && got.iter().zip(want).all(|(g, w)| {
            g.len() == w.len() && g.iter().zip(w.iter()).all(|(a, b)| (a - b).abs() < 1e-9)
        });
    if !ok {
        die(&format!("{what}: got {got:?}, want {want:?}"));
    }
}

fn assert_eq_or_die<T: PartialEq + std::fmt::Debug>(what: &str, got: &[T], want: &[T]) {
    if got != want {
        die(&format!("{what}: got {got:?}, want {want:?}"));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("interop MISMATCH: {msg}");
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = || -> ! {
        eprintln!("usage: interop <write|read> <dir>");
        exit(2);
    };
    if args.len() != 3 {
        usage();
    }
    let dir = Path::new(&args[2]);
    let result = match args[1].as_str() {
        "write" => write(dir),
        "read" => read(dir),
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("interop ERROR: {e}");
        exit(1);
    }
}
