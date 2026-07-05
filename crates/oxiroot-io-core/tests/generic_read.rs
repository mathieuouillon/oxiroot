//! Tests for the generic streamer-info-driven object reader.

use oxiroot_io_core::{RFile, Value};

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
}

/// Sweep every non-RNTuple fixture: decode each top-level key and report whether
/// the top-level object decoded (vs. degraded to `Unsupported`). Prints a table
/// so any regression in coverage is visible.
#[test]
fn sweep_all_fixtures() {
    let dir = format!("{}/../../fixtures", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".root") && !n.contains("rntuple"))
        .collect();
    files.sort();

    let mut ok = 0;
    let mut degraded = 0;
    for file in files {
        let Ok(f) = RFile::open(fixture(&file)) else {
            continue;
        };
        for key in f.keys().iter() {
            if key.is_deleted() {
                continue;
            }
            let name = key.name.clone();
            let class = key.class_name.clone();
            // RNTuple / directory keys are out of scope.
            if class.contains("RNTuple") || class.starts_with("TDirectory") {
                continue;
            }
            match f.get_value(&name) {
                Ok(v) if v.class().is_some() && !matches!(v, Value::Unsupported { .. }) => {
                    ok += 1;
                    // Every decoded object must carry members.
                    if let Value::Object { members, .. } = &v {
                        assert!(!members.is_empty(), "{file}:{name} ({class}) empty");
                    }
                }
                Ok(_) => {
                    degraded += 1;
                    println!("  DEGRADED {file}:{name} ({class})");
                }
                Err(e) => panic!("{file}:{name} ({class}) errored: {e}"),
            }
        }
    }
    println!("generic reader: {ok} objects decoded, {degraded} degraded to Unsupported");
    assert!(ok > 30, "expected broad coverage, only {ok} decoded");
}

/// Locked assertions cross-checked against uproot.
#[test]
fn decoded_values_match_root() {
    // TObjString.
    let f = RFile::open(fixture("persist_objs.root")).unwrap();
    let s = f.get_value("label").unwrap();
    assert_eq!(
        s.get("fString").and_then(Value::as_str),
        Some("hello world")
    );

    // TParameter<double> / <int>.
    assert_eq!(
        f.get_value("lumi")
            .unwrap()
            .get("fVal")
            .and_then(Value::as_f64),
        Some(137.5)
    );
    assert_eq!(
        f.get_value("nevents")
            .unwrap()
            .get("fVal")
            .and_then(Value::as_i64),
        Some(42)
    );

    // TH1D: name, title, and the bin contents (TArrayD base), vs uproot's
    // values(flow=True) = [0, 20, 38, 54, 68, 80, …].
    let h = RFile::open(fixture("analysis.root"))
        .unwrap()
        .get_value("h")
        .unwrap();
    assert_eq!(h.class(), Some("TH1D"));
    assert_eq!(h.get("fName").and_then(Value::as_str), Some("h"));
    assert_eq!(
        h.get("fXaxis")
            .and_then(|a| a.get("fNbins"))
            .and_then(Value::as_i64),
        Some(20)
    );
    let arr = h.get("fArray").and_then(Value::as_array).unwrap();
    assert_eq!(arr.len(), 22);
    let first: Vec<f64> = arr[..6].iter().filter_map(Value::as_f64).collect();
    assert_eq!(first, [0.0, 20.0, 38.0, 54.0, 68.0, 80.0]);

    // TGraphErrors: fX / fY / fEX / fEY point arrays.
    let g = RFile::open(fixture("graphs.root"))
        .unwrap()
        .get_value("ge")
        .unwrap();
    let fx: Vec<f64> = g
        .get("fX")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let fy: Vec<f64> = g
        .get("fY")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    assert_eq!(fx, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(fy, [10.0, 20.0, 30.0, 40.0]);

    // TMap: three pairs, mixed value types, recursively decoded.
    let m = RFile::open(fixture("tmap.root"))
        .unwrap()
        .get_value("meta")
        .unwrap();
    let items = m.get("items").and_then(Value::as_array).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0]
            .get("key")
            .and_then(|k| k.get("fString"))
            .and_then(Value::as_str),
        Some("version")
    );

    // TList: three heterogeneous members.
    let l = RFile::open(fixture("objlist.root"))
        .unwrap()
        .get_value("mylist")
        .unwrap();
    let its = l.get("items").and_then(Value::as_array).unwrap();
    assert_eq!(its.len(), 3);
    assert_eq!(its[0].class(), Some("TH1F"));
    assert_eq!(its[1].get("fString").and_then(Value::as_str), Some("hello"));

    // TF1: the fitted formula string and cling parameters, nested in TFormula.
    let f1 = RFile::open(fixture("tf1.root"))
        .unwrap()
        .get_value("myfunc")
        .unwrap();
    let formula = f1.get("fFormula").unwrap();
    assert_eq!(
        formula.get("fFormula").and_then(Value::as_str),
        Some("[p0]*sin([p1]*x)+[p2]")
    );
}

/// The generic reader on a genuinely FOREIGN file — written by official ROOT
/// (6.40), not oxiroot (regenerate via `scripts/gen_rootcpp_objects.cpp`).
#[test]
fn reads_root_cpp_written_file() {
    let f = RFile::open(fixture("rootcpp_objects.root")).unwrap();

    // TObjString + TParameter<double> written by ROOT.
    assert_eq!(
        f.get_value("note")
            .unwrap()
            .get("fString")
            .and_then(Value::as_str),
        Some("written by ROOT 6.40")
    );
    assert_eq!(
        f.get_value("thr")
            .unwrap()
            .get("fVal")
            .and_then(Value::as_f64),
        Some(2.5)
    );

    // TList: a TNamed("a","alpha") then a TObjString("beta"), decoded member-wise.
    let list = f.get_value("mylist").unwrap();
    let items = list.get("items").and_then(Value::as_array).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].class(), Some("TNamed"));
    assert_eq!(
        items[0].get("fTitle").and_then(Value::as_str),
        Some("alpha")
    );
    assert_eq!(
        items[1].get("fString").and_then(Value::as_str),
        Some("beta")
    );

    // TH1D written by ROOT: nested TAxis with the 10 bins we booked.
    let h = f.get_value("hpx").unwrap();
    assert_eq!(h.class(), Some("TH1D"));
    assert_eq!(
        h.get("fXaxis")
            .and_then(|a| a.get("fNbins"))
            .and_then(Value::as_i64),
        Some(10)
    );
}
