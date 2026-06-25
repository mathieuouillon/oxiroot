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
//!   - TTree "Tree": branch ti = int32 [1..5], tf = double [1.5..5.5].

use std::path::Path;
use std::process::exit;

use oxiroot::prelude::*;

/// In-range bin contents of the canonical histogram.
const HIST_BINS: [f64; 4] = [1.0, 2.0, 3.0, 4.0];
/// Canonical RNTuple columns.
const NTPL_X: [i32; 5] = [1, 2, 3, 4, 5];
const NTPL_Y: [f64; 5] = [1.5, 2.5, 3.5, 4.5, 5.5];

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
    write_th1d_file(dir.join("rust_hist.root"), &h, Compression::None)?;

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
    let h = read_th1d(&f, "h")?;
    assert_close("hist bin contents", h.values(), &HIST_BINS);
    println!("read oracle_hist.root — bin contents match");

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

fn assert_close(what: &str, got: &[f64], want: &[f64]) {
    if got.len() != want.len() || got.iter().zip(want).any(|(a, b)| (a - b).abs() > 1e-9) {
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
