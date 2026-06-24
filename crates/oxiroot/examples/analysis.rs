//! A miniature analysis end-to-end: fill weighted histograms (with variable
//! bins), combine and normalize them, organize the output into subdirectories,
//! write a columnar event dataset, and read it back — all readable by official
//! ROOT and uproot.
//!
//! Run with: `cargo run -p oxiroot --example analysis`

use oxiroot::prelude::*;

fn main() -> Result<()> {
    let dir = std::env::temp_dir();

    // --- Fill histograms, as in an event loop. ---------------------------------
    // A momentum spectrum with physics-style variable bins and weighted entries.
    let edges = [0.0, 10.0, 20.0, 40.0, 80.0, 160.0];
    let mut pt = TH1::new_variable("pt", "p_{T} spectrum", &edges);
    pt.sumw2(); // track per-bin errors for the weighted fills

    // A 2-D correlation.
    let mut eta_phi = TH2::new("eta_phi", "#eta vs #phi", 5, -2.5, 2.5, 4, -3.2, 3.2);

    // Pretend events: (pt, weight, eta, phi).
    let events = [
        (5.0, 1.2, 0.3, 1.0),
        (15.0, 0.8, -1.1, -2.0),
        (35.0, 1.5, 2.0, 0.5),
        (90.0, 1.0, 0.1, 3.0),
        (12.0, 0.9, -0.4, -0.2),
    ];
    for &(p, w, eta, phi) in &events {
        pt.fill_weight(p, w);
        eta_phi.fill(eta, phi);
    }
    println!(
        "pt: {} entries, integral {:.2}, bin-2 = {:.2} ± {:.2}",
        pt.entries,
        pt.integral(),
        pt.contents[2],
        pt.bin_error(2),
    );

    // --- Combine and normalize, as when merging samples. -----------------------
    let mut signal = pt.clone();
    let mut background = pt.clone();
    background.scale(0.1); // scale background down
    signal.add(&background, 1.0)?; // stack background onto signal (a merge)
    signal.scale(1.0 / signal.integral().max(1.0)); // normalize to unit area
    println!("normalized signal integral = {:.6}", signal.integral());

    // --- Save histograms into per-region subdirectories. -----------------------
    let hist_path = dir.join("analysis_hists.root");
    write_histograms_dirs(
        &hist_path,
        &[Hist::Th1(&pt), Hist::Th2(&eta_phi)], // top level
        &[
            ("signal", &[Hist::Th1(&signal)]),
            ("background", &[Hist::Th1(&background)]),
        ],
        Compression::Zstd(5),
    )?;
    println!("wrote histograms -> {}", hist_path.display());

    // --- Write a columnar event dataset (ergonomic Field constructors). --------
    let ntuple_path = dir.join("analysis_events.root");
    let fields = vec![
        Field::f64("mass", vec![91.2, 125.1, 173.0]),
        Field::i32("charge", vec![0, -1, 1]),
        Field::strings("label", vec!["Z".into(), "H".into(), "top".into()]),
        Field::vec_f64("jet_pt", vec![vec![30.0, 25.0], vec![], vec![120.0]]),
    ];
    write_rntuple_file(&ntuple_path, "events", &fields, Compression::Zstd(5))?;
    println!("wrote RNTuple -> {}", ntuple_path.display());

    // --- Read it all back. -----------------------------------------------------
    let f = RFile::open(&hist_path)?;
    let pt_back = read_th1d(&f, "pt")?;
    let sig_back = read_th1d_in(&f, "signal", "pt")?;
    println!(
        "read back: pt has {} bins, signal/pt integral = {:.6}",
        pt_back.values().len(),
        sig_back.integral(),
    );

    let g = RFile::open(&ntuple_path)?;
    let events = RNTuple::open(&g, "events")?;
    println!("RNTuple `events`: {} entries", events.num_entries());
    if let FieldValues::VecF64(jets) = events.read_field(&g, "jet_pt")? {
        println!("  jet_pt per event: {jets:?}");
    }

    Ok(())
}
