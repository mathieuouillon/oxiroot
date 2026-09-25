//! Store run *provenance* next to your data: bundle a small histogram together
//! with the metadata that describes how it was produced — a JSON-ish config
//! string (`ObjString`), the integrated luminosity (`Parameter<double>`), the
//! run number (`Parameter<Long64_t>`), and the list of input files (a `TList`
//! of `ObjString`s) — into ONE ROOT file via `FileWriter`, then read
//! it all back. Everything written here is a real ROOT object, so ROOT and
//! uproot read the provenance alongside the plot.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example objects
//! ```

use oxiroot::prelude::*;

fn main() -> oxiroot::Result<()> {
    // Keep the file out of the repo: a temp path tagged with THIS example's
    // name, removed before we return.
    let path = std::env::temp_dir().join("oxiroot_ex_objects.root");

    // --- The data: a tiny histogram, as an analysis would produce. -------------
    let mut mass = Hist::reg(20, 80.0, 100.0)
        .double()
        .named("mass")
        .titled("di-muon mass [GeV]");
    for &(x, w) in &[(90.9, 1.0), (91.2, 1.2), (91.4, 0.9), (92.0, 1.0)] {
        mass.fill_weight(x, w);
    }

    // --- The provenance: metadata that describes how the data was made. --------
    // An `ObjString` is ROOT's "collectable string" — a plain string stored under
    // a key. Handy for a config blob, a git hash, or (here) a JSON snippet. The
    // `named(...)` builder sets the key it lands under in the file.
    let config =
        ObjString::new(r#"{"trigger":"HLT_Mu","era":"2024C","tune":"CP5"}"#).named("config");

    // A `Parameter<T>` is a named scalar tagged with its C++ type. Use the typed
    // constructors: `f64` -> Parameter<double>, `i64` -> Parameter<Long64_t>.
    let lumi = Parameter::f64("integrated_lumi_fb", 32.7); // inverse femtobarns
    let run = Parameter::i64("run_number", 380_947); // a real-sized run number

    // A `TList` of the input files that fed this histogram. `ObjList::list()`
    // builds a `TList` (use `array()` for a `TObjArray`); `add` takes any writable
    // object, so we push one `ObjString` per file name.
    let inputs = ["skim_000.root", "skim_001.root", "skim_002.root"];
    let mut file_list = ObjList::list().named("input_files");
    for f in inputs {
        file_list = file_list.add(&ObjString::new(f));
    }

    // --- Bundle data + provenance into ONE file with `FileWriter`. ----------------
    // Each `add` takes a `&dyn WriteRoot`, so histograms and metadata objects sit
    // side by side as top-level keys — exactly how ROOT stashes run info next to
    // the plots it belongs to.
    FileWriter::create(&path)
        .add(&mass) // the data
        .add(&config) // the metadata, alongside it
        .add(&lumi)
        .add(&run)
        .add(&file_list)
        .write(Compression::Zstd(5))?;
    println!("wrote data + provenance -> {}", path.display());
    println!(
        "  file holds: mass (TH1D), config (TObjString), lumi + run (TParameter), \
         input_files (TList of {} names)",
        inputs.len(),
    );

    // --- Read it back (the `ReadRoot` trait: `Type::read_root(&file, key)`). ----
    // Each object type knows how to decode itself from a key by name.
    let f = FileReader::open(&path)?;

    let mass_back = Hist1D::read_root(&f, "mass")?;
    println!(
        "\nread back the data:\n  mass: {} entries, integral {:.2}, mean {:.3} GeV",
        mass_back.entries,
        mass_back.integral(),
        mass_back.mean(),
    );

    // The config string comes straight back out with `.value()`.
    let config_back = ObjString::read_root(&f, "config")?;
    println!(
        "\nread back the provenance:\n  config = {}",
        config_back.value()
    );

    // `Parameter::value()` returns a typed `ParamValue`; `as_f64()` widens any of
    // them, or match on the enum to keep the exact type.
    let lumi_back = Parameter::read_root(&f, "integrated_lumi_fb")?;
    println!(
        "  integrated luminosity = {:.1} fb^-1",
        lumi_back.value().as_f64(),
    );

    let run_back = Parameter::read_root(&f, "run_number")?;
    match run_back.value() {
        ParamValue::Long64(n) => println!("  run number = {n} (stored as TParameter<Long64_t>)"),
        other => println!("  run number = {:.0}", other.as_f64()),
    }

    // A `TList` reads back as an `ObjList`; `items::<T>()` pulls out every member
    // of one type, so we recover the file names as `ObjString`s.
    let list_back = ObjList::read_root(&f, "input_files")?;
    let names = list_back.items::<ObjString>()?;
    println!("  input files ({} of them):", names.len());
    for name in &names {
        println!("    - {}", name.value());
    }

    // --- The point: metadata now travels *with* the plot. ----------------------
    // Anyone who opens this file in ROOT, uproot, or oxiroot sees not just the
    // histogram but the exact config, luminosity, run, and inputs it came from —
    // no side-car text file to lose track of.
    println!(
        "\nProvenance stored next to the data: config + {:.1} fb^-1 + run {} + {} inputs, \
         all readable by ROOT/uproot.",
        lumi_back.value().as_f64(),
        match run_back.value() {
            ParamValue::Long64(n) => n,
            other => other.as_f64() as i64,
        },
        names.len(),
    );

    // Clean up — never litter.
    let _ = std::fs::remove_file(&path);
    Ok(())
}
