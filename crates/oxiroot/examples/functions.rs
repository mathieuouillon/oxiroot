//! Standalone `TF1`/`TF2`/`TF3` functions: build from formulas, evaluate in pure
//! Rust (`eval`/`integral`/`derivative`), and write them to a ROOT file that ROOT
//! C++ and uproot read back.
//!
//! ```sh
//! cargo run -p oxiroot --example functions
//! ```

use oxiroot::prelude::*;

fn main() -> Result<()> {
    // A 1-D function: a decaying sine. Parameters are [0], [1], …; the variable is x.
    let f1 =
        TF1::new("f1", "[0]*exp(-[1]*x)*sin([2]*x)", 0.0, 10.0)?.with_params(vec![5.0, 0.3, 2.0]);
    println!("f1(x) = {}", f1.title());
    println!("  eval(1.5)      = {:.6}", f1.eval(1.5));
    println!("  integral(0,10) = {:.6}", f1.integral(0.0, 10.0));
    println!("  derivative(1.5)= {:.6}", f1.derivative(1.5));

    // A ROOT shortcut: a Gaussian peak on a linear background.
    let peak = TF1::new("peak", "gaus(0) + pol1(3)", -5.0, 5.0)?
        .with_params(vec![10.0, 0.0, 1.0, 2.0, 0.5]);
    println!(
        "peak(0)          = {:.6}  (formula {})",
        peak.eval(0.0),
        peak.formula()
    );

    // 2-D and 3-D functions add y (and z).
    let f2 =
        TF2::new("f2", "[0]*sin(x) + [1]*y*y", -3.0, 3.0, -2.0, 2.0)?.with_params(vec![1.5, 0.7]);
    let f3 = TF3::new("f3", "[0]*x + y*z", 0.0, 2.0, 0.0, 2.0, 0.0, 2.0)?.with_params(vec![2.0]);
    println!("f2(1,1)          = {:.6}", f2.eval(1.0, 1.0));
    println!("f3(1,1,1)        = {:.6}", f3.eval(1.0, 1.0, 1.0));

    // Write all four to a ROOT file (ROOT C++ and uproot read these keys).
    let out = std::env::temp_dir().join("oxiroot_functions.root");
    RootFile::create(&out)
        .add(&f1)
        .add(&peak)
        .add(&f2)
        .add(&f3)
        .write(Compression::Zstd(5))?;
    println!("wrote {}", out.display());

    // Read one back and confirm it evaluates identically.
    let g = TF1::read_root(&RFile::open(&out)?, "f1")?;
    assert!((g.eval(1.5) - f1.eval(1.5)).abs() < 1e-12);
    println!("read back f1: eval(1.5) = {:.6}", g.eval(1.5));
    Ok(())
}
