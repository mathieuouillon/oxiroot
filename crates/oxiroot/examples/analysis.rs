//! A miniature analysis end-to-end: fill weighted histograms (with variable
//! bins), combine and normalize them (`*=`, `Index`, the `Histogram` trait),
//! write them — into subdirectories and a flat heterogeneous file via the
//! `RootFile` builder (with a float-precision `TH1F`) — write a columnar event
//! dataset, and read it back (`TH1::read_root`) — all readable by official ROOT
//! and uproot.
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
        "pt: {} entries, integral {:.2}, all-cell sum {:.2}, bin-2 = {:.2} ± {:.2}",
        pt.entries,
        pt.integral(), // in-range bins only
        pt.sum(),      // Histogram trait: every cell, flow included
        pt[2],         // Index: bin content by cell index
        pt.bin_error(2),
    );

    // --- Multithreaded fill, ROOT's TThreadedObject pattern. -------------------
    // Share `&acc` across threads and call `fill` from any of them — each thread
    // transparently gets its own copy; `merge` combines them exactly (identical
    // to a serial fill).
    let samples: Vec<f64> = (0..100_000).map(|i| (i as f64 * 0.618) % 100.0).collect();
    let acc = ThreadedHist::new(TH1::new("mass", "toy mass", 100, 0.0, 100.0));
    std::thread::scope(|s| {
        for chunk in samples.chunks(samples.len().div_ceil(4)) {
            let acc = &acc;
            s.spawn(move || {
                for &x in chunk {
                    acc.fill(x); // routes to this thread's copy, no manual setup
                }
            });
        }
    });
    let mass = acc.merge()?;
    println!(
        "mass (parallel fill): {} entries, mean {:.3}, std {:.3}, max bin {} = {}",
        mass.entries,
        mass.mean(),
        mass.std_dev(),
        mass.maximum_bin(),
        mass.maximum(),
    );

    // --- Combine and normalize, as when merging samples. -----------------------
    let mut signal = pt.clone();
    let mut background = pt.clone();
    background *= 0.1; // MulAssign — scale background down
    signal.add(&background, 1.0)?; // stack background onto signal (a merge)
    signal *= 1.0 / signal.integral().max(1.0); // normalize to unit area
    println!("normalized signal integral = {:.6}", signal.integral());
    println!("{signal}"); // Display: one-line summary

    // --- Compose a file with the `RootFile` builder: top-level objects plus -----
    // per-region subdirectories. The one way to write more than a single object.
    let hist_path = dir.join("analysis_hists.root");
    RootFile::create(&hist_path)
        .add(&pt) // top level
        .add(&eta_phi)
        .dir("signal", |d| d.add(&signal)) // a TDirectory per region
        .dir("background", |d| d.add(&background))
        .write(Compression::Zstd(5))?;
    println!("wrote histograms -> {}", hist_path.display());

    // --- A flat file mixing any writable types — histograms, profiles, graphs. -
    let mut prof = TProfile::new("pt_prof", "<pt> per region", 5, 0.0, 5.0);
    for (region, &(p, ..)) in events.iter().enumerate() {
        prof.fill(region as f64 + 0.5, p);
    }
    // Write `pt` as a float-precision TH1F just by setting its on-disk precision.
    let pt_f32 = pt.clone().with_precision(Precision::Float);
    let multi_path = dir.join("analysis_multi.root");
    RootFile::create(&multi_path)
        .add(&pt_f32)
        .add(&eta_phi)
        .add(&prof)
        .write(Compression::Zstd(5))?;
    println!(
        "wrote {} as {} + TH2D + TProfile",
        multi_path.display(),
        pt_f32.class_name(),
    );

    // --- Append one more object to that existing file (RootFile::open). --------
    // The file keeps its existing keys; the normalized signal is added alongside.
    let mut sig = signal.clone();
    sig.name = "signal".to_string();
    RootFile::open(&multi_path)?
        .add(&sig)
        .write(Compression::Zstd(5))?;
    println!("appended `{}` to {}", sig.name, multi_path.display());

    // --- Write a columnar event dataset: build an `Ntuple`, then `write_root`. -
    // The method form of `write_rntuple_file`, mirroring `hist.write_root`.
    let ntuple_path = dir.join("analysis_events.root");
    let events_ntuple = Ntuple::new(
        "events",
        vec![
            Field::f64("mass", vec![91.2, 125.1, 173.0]),
            Field::i32("charge", vec![0, -1, 1]),
            Field::strings("label", vec!["Z".into(), "H".into(), "top".into()]),
            Field::vec_f64("jet_pt", vec![vec![30.0, 25.0], vec![], vec![120.0]]),
        ],
    );
    events_ntuple.write_root(&ntuple_path, Compression::Zstd(5))?;
    println!("wrote RNTuple -> {}", ntuple_path.display());

    // --- Same events as a classic TTree: build a `Tree`, then `write_root`. ----
    let tree_path = dir.join("analysis_tree.root");
    Tree::new(
        "Events",
        vec![
            Branch::f64("mass", vec![91.2, 125.1, 173.0]),
            Branch::i32("charge", vec![0, -1, 1]),
        ],
    )
    .write_root(&tree_path, Compression::Zstd(5))?;
    println!("wrote TTree   -> {}", tree_path.display());

    // --- Read it all back (idiomatic `TH1::read_root`; subdir via `read_root_in`).
    let f = RFile::open(&hist_path)?;
    let pt_back = TH1::read_root(&f, "pt")?;
    let sig_back = TH1::read_root_in(&f, "signal", "pt")?;
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

    let h = RFile::open(&tree_path)?;
    let tree = TTree::open(&h, "Events")?;
    println!("TTree `Events`: {} entries", tree.num_entries());
    if let BranchValues::F64(mass) = tree.read_branch(&h, "mass")? {
        println!("  mass per entry: {mass:?}");
    }

    Ok(())
}
