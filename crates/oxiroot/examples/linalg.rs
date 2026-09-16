//! ROOT linear-algebra objects from `oxiroot::linalg`: a `TVectorD` (vector of
//! doubles), a `TMatrixD` (dense matrix), and a `TMatrixDSym` (symmetric matrix —
//! the shape a fit covariance takes). We build the three, write them into ONE
//! ROOT file with the `RootFile` builder, read them back, and assert the
//! round-trip is byte-exact. Official ROOT and uproot read this file too — the
//! serialized bytes match ROOT's key-for-key.
//!
//! ```sh
//! cargo run -p oxiroot --example linalg
//! ```

use oxiroot::prelude::*;

fn main() -> oxiroot::Result<()> {
    // Keep the temp file out of the repo and clean it up before returning.
    let path = std::env::temp_dir().join("oxiroot_ex_linalg.root");

    // --- Build three linear-algebra objects, as a fit would produce them. ------
    // A residual 3-vector: (data - model) at three points.
    let residuals = TVectorD::new(vec![0.20, -0.15, 0.05]).named("residuals");

    // A 2x3 design matrix (Jacobian) — 2 parameters, 3 measurements — row-major.
    // Row 0 is the intercept column (all ones); row 1 is the slope column (x).
    let design = TMatrixD::new(
        2,
        3,
        vec![
            1.0, 1.0, 1.0, // ∂/∂intercept at each point
            1.0, 2.0, 3.0, // ∂/∂slope at each point
        ],
    )
    .named("design");

    // A symmetric 3x3 covariance matrix — the full n*n given row-major (only the
    // upper triangle is written to disk; TMatrixDSym re-expands it on read).
    // Diagonal = variances; off-diagonal = covariances (mirrored across it).
    let cov = TMatrixDSym::new(
        3,
        vec![
            0.040, 0.010, 0.000, //
            0.010, 0.090, 0.020, //
            0.000, 0.020, 0.160, //
        ],
    )
    .named("cov");

    println!("built:");
    println!("  residuals : TVectorD  len {}", residuals.len());
    println!(
        "  design    : TMatrixD  {}x{}",
        design.rows(),
        design.cols()
    );
    println!("  cov       : TMatrixDSym {n}x{n}", n = cov.dim());

    // --- Write all three into one file with the `RootFile` builder. ------------
    // `.add` takes anything `WriteRoot`; each object is stored under its `named`
    // key. This is the one way to write more than a single object per file.
    RootFile::create(&path)
        .add(&residuals)
        .add(&design)
        .add(&cov)
        .write(Compression::Zstd(5))?;
    println!("wrote 3 objects -> {}", path.display());

    // --- Read them back (idiomatic `ReadRoot::read_root`, keyed by name). ------
    let f = RFile::open(&path)?;
    let v = TVectorD::read_root(&f, "residuals")?;
    let m = TMatrixD::read_root(&f, "design")?;
    let s = TMatrixDSym::read_root(&f, "cov")?;

    // Print the design matrix as rows via get(i, j) — teaching the accessor.
    println!("read back `design` as rows:");
    for i in 0..m.rows() {
        let row: Vec<String> = (0..m.cols())
            .map(|j| format!("{:5.1}", m.get(i, j)))
            .collect();
        println!("  [ {} ]", row.join(", "));
    }

    // The covariance is symmetric: cov(i, j) == cov(j, i), reflected across the
    // diagonal. Report the parameter uncertainties (√variance) from its diagonal.
    println!("read back `cov`: symmetric? {}", cov_is_symmetric(&s));
    let sigmas: Vec<f64> = (0..s.dim()).map(|i| s.get(i, i).sqrt()).collect();
    println!(
        "  parameter sigmas (sqrt of diagonal): [{:.3}, {:.3}, {:.3}]",
        sigmas[0], sigmas[1], sigmas[2]
    );
    println!(
        "  cov(0,1) = {:.3} == cov(1,0) = {:.3}  (mirrored)",
        s.get(0, 1),
        s.get(1, 0)
    );

    // --- Assert the round-trip is exact. ---------------------------------------
    // No feature gate needed: linalg is always on. Floating-point equality is
    // fair here — ROOT stores raw IEEE-754 f64, so read-back is bit-identical.
    assert_eq!(v, residuals, "TVectorD round-trip differs");
    assert_eq!(m, design, "TMatrixD round-trip differs");
    assert_eq!(s, cov, "TMatrixDSym round-trip differs");
    assert_eq!(v.elements(), &[0.20, -0.15, 0.05]);
    assert_eq!(m.get(1, 2), 3.0);
    assert_eq!(s.get(1, 2), s.get(2, 1)); // symmetry survives disk
    println!("round-trip is byte-exact (asserted); ROOT and uproot read this file.");

    // Never litter: remove the temp file before returning.
    let _ = std::fs::remove_file(&path);
    Ok(())
}

/// True if every off-diagonal pair matches its mirror — a `TMatrixDSym` always
/// reads back symmetric because only the upper triangle is stored.
fn cov_is_symmetric(s: &TMatrixDSym) -> bool {
    let n = s.dim();
    (0..n).all(|i| (0..n).all(|j| s.get(i, j) == s.get(j, i)))
}
