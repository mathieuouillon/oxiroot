//! The start-here RNTuple example: write a flat event dataset columnarly, reopen
//! it, and read a few fields back per entry — then put two RNTuples in one file
//! with the `NtupleFile` builder and read both. RNTuple is ROOT's modern columnar
//! event-data format; the files here are readable by official ROOT and uproot.
//! (For nested fields — `std::vector<std::vector<T>>`, records — see
//! `rntuple_nested.rs`.)
//!
//! Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example rntuple
//! ```

use oxiroot::prelude::*;

fn main() -> Result<()> {
    let dir = std::env::temp_dir();

    // --- Build one flat event dataset, column by column. -----------------------
    // Each `Field` is one whole column: a name plus a `Vec` of values, one per
    // event. All fields must have the same number of entries — here, four events.
    // Types are mixed freely: a scalar `f64`, a scalar `i32`, a `std::string`, and
    // a jagged `std::vector<f64>` (a variable-length list per event).
    let mass = vec![91.19, 125.10, 4.18, 173.1]; // GeV
    let charge = vec![0, 0, -1, 1];
    let label = vec!["Z".into(), "H".into(), "b".into(), "top".into()];
    let jet_pt = vec![
        vec![55.0, 31.2],       // two jets
        vec![120.0],            // one jet
        vec![],                 // none
        vec![88.0, 40.0, 12.0], // three jets
    ];

    let events = Ntuple::new(
        "events",
        vec![
            Field::f64("mass", mass),
            Field::i32("charge", charge),
            Field::strings("label", label),
            Field::vec_f64("jet_pt", jet_pt),
        ],
    );

    // --- Write it. `write_root` is the method form of `write_rntuple_file`. -----
    // Pages are Zstd-compressed on disk; the reader decompresses transparently.
    let path = dir.join("oxiroot_ex_rntuple.root");
    events.write_root(&path, Compression::Zstd(5))?;
    println!("wrote RNTuple `events` -> {}", path.display());

    // --- Reopen and inspect the schema before touching any data. ---------------
    // Opening only parses the anchor, header, and footer — column pages are read
    // lazily, per field, when you ask for them.
    let f = RFile::open(&path)?;
    let ntpl = RNTuple::open(&f, "events")?;
    println!(
        "reopened: {} entries, fields = {:?}",
        ntpl.num_entries(),
        ntpl.field_names(),
    );

    // --- Read a couple of fields back. Each `read_field` decodes one column ----
    // (or, for a `std::vector`/`std::string`, its offset + data columns) into a
    // typed `FieldValues`. Match on the variant to get at the `Vec`.
    let masses = match ntpl.read_field(&f, "mass")? {
        FieldValues::F64(v) => v,
        other => panic!("mass should be F64, got {other:?}"),
    };
    let charges = match ntpl.read_field(&f, "charge")? {
        FieldValues::I32(v) => v,
        other => panic!("charge should be I32, got {other:?}"),
    };
    let labels = match ntpl.read_field(&f, "label")? {
        FieldValues::Str(v) => v,
        other => panic!("label should be Str, got {other:?}"),
    };
    // A jagged field reads back as one inner `Vec` per entry.
    let jets = match ntpl.read_field(&f, "jet_pt")? {
        FieldValues::VecF64(v) => v,
        other => panic!("jet_pt should be VecF64, got {other:?}"),
    };

    // --- Print it as an event loop would see it: one row per entry. ------------
    println!("per-entry values:");
    for i in 0..ntpl.num_entries() as usize {
        println!(
            "  {:>3}: label = {:>3}  mass = {:>7.2} GeV  charge = {:+}  jets = {:?}",
            i, labels[i], masses[i], charges[i], jets[i],
        );
    }
    // A tiny derived quantity, to show the columns are just plain `Vec`s now.
    let total_jets: usize = jets.iter().map(Vec::len).sum();
    println!("  ({total_jets} jets across {} events)", jets.len());

    // --- Several RNTuples in one file, via the `NtupleFile` builder. -----------
    // `Ntuple::write_root` writes exactly one; `NtupleFile` puts more than one in
    // the same file (and can nest them in `TDirectory`s — see the docs). Here: the
    // per-event `events` alongside a small per-run bookkeeping RNTuple.
    let multi_path = dir.join("oxiroot_ex_rntuple_multi.root");
    NtupleFile::new()
        .add(Ntuple::new(
            "events",
            vec![
                Field::f64("mass", vec![91.19, 125.10]),
                Field::i32("charge", vec![0, 0]),
            ],
        ))
        .add(Ntuple::new(
            "runs",
            vec![
                Field::i32("run", vec![101, 102, 103]),
                Field::i64("n_events", vec![12_000, 8_400, 15_250]),
            ],
        ))
        .write_root(&multi_path, Compression::Zstd(5))?;
    println!("wrote 2 RNTuples -> {}", multi_path.display());

    // --- Read both RNTuples back out of the one file. --------------------------
    let g = RFile::open(&multi_path)?;
    let ev = RNTuple::open(&g, "events")?;
    let runs = RNTuple::open(&g, "runs")?;
    println!(
        "  `events`: {} entries {:?}",
        ev.num_entries(),
        ev.field_names(),
    );
    if let FieldValues::I64(n) = runs.read_field(&g, "n_events")? {
        let total: i64 = n.iter().sum();
        println!(
            "  `runs`:   {} entries, {total} events recorded across all runs",
            runs.num_entries(),
        );
    }

    // --- Clean up the temp files (never litter). -------------------------------
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&multi_path);

    Ok(())
}
