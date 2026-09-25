//! The graph family as first-class ROOT objects: build a cross-section
//! measurement as a `TGraphErrors` (symmetric y errors), an asymmetric-error
//! variant, a 2-D parameter scan as a `Graph2D`, and two datasets drawn
//! together as a `GraphStack` — write them all into ONE ROOT file with the
//! `FileWriter`, then read one graph back point-by-point
//! (`Graph::read_root`). The file is readable by official ROOT and uproot.
//! (Fitting a graph is shown in `fit.rs`; this is about the objects themselves.)
//!
//! ```sh
//! cargo run -p oxiroot --example graphs
//! ```

use oxiroot::prelude::*;

fn main() -> oxiroot::Result<()> {
    let path = std::env::temp_dir().join("oxiroot_ex_graphs.root");

    // --- 1. A measured cross-section vs beam energy, as a TGraphErrors. --------
    // Each point is (E [GeV], sigma [pb]); the y error is the statistical
    // uncertainty on sigma. There is no x error here (the beam energy is known),
    // so pass a zero array for `ex` — `with_errors` still writes a TGraphErrors.
    let energy = vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
    let sigma = vec![12.4, 18.9, 22.1, 20.3, 15.7, 9.8];
    let sigma_err = vec![1.1, 1.3, 1.5, 1.4, 1.2, 0.9];
    let xsec = Graph::with_errors(
        energy.clone(),
        sigma.clone(),
        vec![0.0; energy.len()], // ex: energy is exact
        sigma_err.clone(),       // ey: statistical error on sigma
    )?
    .named("xsec")
    .titled("cross-section vs energy");
    // `class_name` reflects the error variant we chose: no errors -> "Graph",
    // symmetric -> "TGraphErrors", asymmetric (below) -> "TGraphAsymmErrors".
    println!(
        "xsec: {} ({} points), class {}",
        xsec.title,
        xsec.len(),
        xsec.class_name(),
    );

    // --- 2. The same measurement with ASYMMETRIC errors (a TGraphAsymmErrors). -
    // Systematic bands are often lopsided. Arg order is (x, y, exl, exh, eyl,
    // eyh): the low then high error for x, then the low then high error for y.
    let ey_low: Vec<f64> = sigma_err.iter().map(|e| e * 0.8).collect(); // tighter below
    let ey_high: Vec<f64> = sigma_err.iter().map(|e| e * 1.5).collect(); // looser above
    let xsec_asym = Graph::with_asymm_errors(
        energy.clone(),
        sigma.clone(),
        vec![0.0; energy.len()], // exl
        vec![0.0; energy.len()], // exh
        ey_low,
        ey_high,
    )?
    .named("xsec_asym")
    .titled("cross-section (asymmetric errors)");
    println!(
        "xsec_asym: {} points, class {}",
        xsec_asym.len(),
        xsec_asym.class_name(),
    );

    // --- 3. A 2-D scan over (mass, width) with a likelihood z, as a Graph2D. ---
    // A Graph2D is an (x, y, z) scatter — here a small grid of trial points and
    // the value of some objective (e.g. a negative log-likelihood) at each.
    let (mut sx, mut sy, mut sz): (Vec<f64>, Vec<f64>, Vec<f64>) =
        (Vec::new(), Vec::new(), Vec::new());
    for &m in &[90.0_f64, 91.0, 92.0] {
        for &w in &[2.0_f64, 2.5, 3.0] {
            sx.push(m);
            sy.push(w);
            // A toy parabolic well centred at (91, 2.5).
            sz.push((m - 91.0).powi(2) + 4.0 * (w - 2.5).powi(2));
        }
    }
    let scan = Graph2D::new(sx, sy, sz)?
        .named("scan")
        .titled("-lnL scan over (mass, width)");
    println!("scan: TGraph2D with {} grid points", scan.len());

    // --- 4. Two datasets drawn in one frame, as a GraphStack. ------------------
    // A multigraph just holds several TGraphs so they share a frame/legend when
    // drawn. Give the members their own names so a reader can tell them apart.
    let data = Graph::new(energy.clone(), sigma.clone())?.named("data");
    let theory = Graph::new(
        energy.clone(),
        // A smooth "prediction" curve to overlay on the points.
        energy
            .iter()
            .map(|&e| 25.0 * (-((e - 3.5) / 2.5).powi(2)).exp())
            .collect(),
    )?
    .named("theory");
    let comparison = GraphStack::new()
        .named("comparison")
        .titled("data vs theory")
        .add(data)
        .add(theory);
    println!(
        "comparison: TMultiGraph holding {} graphs",
        comparison.graphs().len(),
    );

    // --- Write every graph into ONE file with FileWriter. -----------------------
    // `add` takes anything that is `WriteRoot`, so graphs of all four kinds go
    // into the same file next to each other — the one way to write more than a
    // single object.
    FileWriter::create(&path)
        .add(&xsec)
        .add(&xsec_asym)
        .add(&scan)
        .add(&comparison)
        .write(Compression::Zstd(5))?;
    println!("\nwrote 4 graphs -> {}", path.display());

    // --- Read one graph back and print its points (idiomatic Graph::read_root).
    let f = FileReader::open(&path)?;
    let back = Graph::read_root(&f, "xsec")?;
    println!("read back `{}` ({} points):", back.name, back.len());
    // The error arrays live in the `errors` enum; pull the y errors out for the
    // symmetric variant we wrote (they are empty for a plain Graph).
    let ey: &[f64] = match &back.errors {
        GraphErrors::Symmetric { ey, .. } => ey,
        _ => &[],
    };
    for i in 0..back.len() {
        println!(
            "  E = {:.1} GeV   sigma = {:5.1} ± {:.1} pb",
            back.x[i],
            back.y[i],
            ey.get(i).copied().unwrap_or(0.0),
        );
    }

    // The multigraph round-trips too: read it and count its members.
    let mg = GraphStack::read_root(&f, "comparison")?;
    println!(
        "read back multigraph `{}`: {} member graphs ({})",
        mg.name(),
        mg.graphs().len(),
        mg.graphs()
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    );

    // --- Clean up: never leave the temp file behind. --------------------------
    drop(f);
    let _ = std::fs::remove_file(&path);
    Ok(())
}
