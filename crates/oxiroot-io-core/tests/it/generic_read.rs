//! Tests for the generic streamer-info-driven object reader.

use oxiroot_io_core::{FileReader, Value};

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
        let Ok(f) = FileReader::open(fixture(&file)) else {
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
    // ObjString.
    let f = FileReader::open(fixture("persist_objs.root")).unwrap();
    let s = f.get_value("label").unwrap();
    assert_eq!(
        s.get("fString").and_then(Value::as_str),
        Some("hello world")
    );

    // Parameter<double> / <int>.
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
    let h = FileReader::open(fixture("analysis.root"))
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
    let g = FileReader::open(fixture("graphs.root"))
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

    // ObjMap: three pairs, mixed value types, recursively decoded.
    let m = FileReader::open(fixture("tmap.root"))
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
    let l = FileReader::open(fixture("objlist.root"))
        .unwrap()
        .get_value("mylist")
        .unwrap();
    let its = l.get("items").and_then(Value::as_array).unwrap();
    assert_eq!(its.len(), 3);
    assert_eq!(its[0].class(), Some("TH1F"));
    assert_eq!(its[1].get("fString").and_then(Value::as_str), Some("hello"));

    // Func1D: the fitted formula string and cling parameters, nested in TFormula.
    let f1 = FileReader::open(fixture("tf1.root"))
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
    let f = FileReader::open(fixture("rootcpp_objects.root")).unwrap();

    // ObjString + Parameter<double> written by ROOT.
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

    // TList: a Named("a","alpha") then an ObjString("beta"), decoded member-wise.
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

    // TH1D written by ROOT: nested Axis with the 10 bins we booked.
    let h = f.get_value("hpx").unwrap();
    assert_eq!(h.class(), Some("TH1D"));
    assert_eq!(
        h.get("fXaxis")
            .and_then(|a| a.get("fNbins"))
            .and_then(Value::as_i64),
        Some(10)
    );
}

/// A file describes every version of a class it holds objects of, and the
/// layouts can differ; each object must be decoded with its own version's.
#[test]
fn each_object_is_decoded_with_its_own_class_version() {
    use oxiroot_io_core::streamer_gen::{basic, Cls};
    use oxiroot_io_core::WBuffer;
    use oxiroot_io_core::{Compression, FileWriter, WriteRoot};

    /// `MyHit` at class version 1 (`fE`) or 2 (`fE`, then `fId`).
    struct Hit(u16);
    impl WriteRoot for Hit {
        fn root_class(&self) -> String {
            "MyHit".to_string()
        }
        fn root_name(&self) -> &str {
            if self.0 == 1 {
                "old"
            } else {
                "new"
            }
        }
        fn root_title(&self) -> &str {
            ""
        }
        fn to_root_bytes(&self) -> Vec<u8> {
            let mut w = WBuffer::new();
            let hit = w.begin_object(self.0);
            w.be_f64(2.5); // fE
            if self.0 >= 2 {
                w.be_i32(7); // fId
            }
            w.end_object(hit);
            w.into_vec()
        }
        fn streamer_classes(&self) -> Vec<Cls<'static>> {
            let mut elements = vec![basic("fE", 8, 8, "double")];
            if self.0 >= 2 {
                elements.push(basic("fId", 3, 4, "int"));
            }
            vec![Cls {
                name: "MyHit".into(),
                version: i32::from(self.0),
                checksum: u32::from(self.0),
                elements,
            }]
        }
    }

    let path = std::env::temp_dir().join("oxiroot_generic_read_versions.root");
    FileWriter::create(&path)
        .add(&Hit(1))
        .add(&Hit(2))
        .write(Compression::None)
        .unwrap();
    let f = FileReader::open(&path).unwrap();

    // Version 1 is described first; version 2 must not be read with its layout.
    let new = f.get_value("new").unwrap();
    assert_eq!(new.get("fE").and_then(Value::as_f64), Some(2.5));
    assert_eq!(new.get("fId").and_then(Value::as_i64), Some(7), "{new}");
    let old = f.get_value("old").unwrap();
    assert_eq!(old.get("fE").and_then(Value::as_f64), Some(2.5));
    assert!(old.get("fId").is_none());

    // A version the file does not describe falls back to the first description.
    let reg = f.streamer_registry().unwrap();
    assert_eq!(reg.get_at("MyHit", 2).map(|i| i.class_version), Some(2));
    assert_eq!(reg.get_at("MyHit", 9).map(|i| i.class_version), Some(1));
    let _ = std::fs::remove_file(path);
}

/// `fixtures/stl_members.root` (ROOT 6.40, `scripts/gen_stl_members.cpp`) holds
/// one object of every STL member shape ROOT streams. The values asserted here
/// are the ones ROOT itself reports for that file.
#[test]
fn stl_members_decode_as_root_wrote_them() {
    let f = FileReader::open(fixture("stl_members.root")).unwrap();

    // TFormula::fParams, a `map<TString,int>` streamed objectwise: the parameter
    // names in index order.
    let formula = f.get_value("fn").unwrap();
    let params = formula
        .get("fFormula")
        .and_then(|v| v.get("fParams"))
        .and_then(Value::as_array)
        .unwrap();
    let entries: Vec<(&str, i64)> = params
        .iter()
        .map(|e| {
            (
                e.get("first").and_then(Value::as_str).unwrap(),
                e.get("second").and_then(Value::as_i64).unwrap(),
            )
        })
        .collect();
    assert_eq!(entries, vec![("p0", 0), ("p1", 1)]);
    // An empty `vector<TObject*>` is an empty array, not an undecoded member.
    assert_eq!(
        formula
            .get("fFormula")
            .and_then(|v| v.get("fLinearParts"))
            .and_then(Value::as_array),
        Some(&[][..])
    );

    // Efficiency::fBeta_bin_params, a `vector<pair<double,double>>` streamed
    // memberwise: every `first`, then every `second`. Bins 1 and 2 were set to
    // (2, 3) and (4, 5); the rest keep ROOT's (1, 1).
    let eff = f.get_value("eff").unwrap();
    let beta = eff
        .get("fBeta_bin_params")
        .and_then(Value::as_array)
        .unwrap();
    let pairs: Vec<(f64, f64)> = beta
        .iter()
        .map(|p| {
            (
                p.get("first").and_then(Value::as_f64).unwrap(),
                p.get("second").and_then(Value::as_f64).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![(1.0, 1.0), (2.0, 3.0), (4.0, 5.0), (1.0, 1.0), (1.0, 1.0)]
    );

    // MultiErrorGraph: `vector<TArrayD>` (objectwise) per y-error bar, and
    // `vector<TAttFill>`/`vector<TAttLine>` (memberwise) per bar.
    let gme = f.get_value("gme").unwrap();
    let low: Vec<Vec<f64>> = gme
        .get("fEyL")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|a| {
            a.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect()
        })
        .collect();
    assert_eq!(low, vec![vec![0.3; 3], vec![0.5; 3]]);
    let fills: Vec<i64> = gme
        .get("fAttFill")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|a| a.get("fFillColor").and_then(Value::as_i64).unwrap())
        .collect();
    assert_eq!(fills, vec![19, 632]); // the second bar was set to kRed
    let widths: Vec<i64> = gme
        .get("fAttLine")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|a| a.get("fLineWidth").and_then(Value::as_i64).unwrap())
        .collect();
    assert_eq!(widths, vec![1, 3]);

    // PolyHist::fCells, a `TStreamerLoop` of `TList`s: ROOT writes each bin in
    // full in the first cell it falls in, so the grid holds both bins once.
    let poly = f.get_value("poly").unwrap();
    let cells = poly.get("fCells").and_then(Value::as_array).unwrap();
    assert_eq!(cells.len(), 625); // fNCells, the 25×25 lookup grid
    let mut bins: Vec<(i64, f64)> = Vec::new();
    for cell in cells {
        for item in cell.get("items").and_then(Value::as_array).unwrap_or(&[]) {
            if let Value::Object { class, .. } = item {
                assert_eq!(class, "TH2PolyBin");
                bins.push((
                    item.get("fNumber").and_then(Value::as_i64).unwrap(),
                    item.get("fContent").and_then(Value::as_f64).unwrap(),
                ));
            }
        }
    }
    assert_eq!(bins, vec![(1, 1.0), (2, 2.0)]);

    // Every other cell that holds a bin, and `fBins`, point back at those two:
    // the slot names the class instead of claiming there is no object there.
    let refs = poly
        .get("fBins")
        .and_then(|l| l.get("items"))
        .and_then(Value::as_array)
        .unwrap();
    assert!(refs.len() == 2 && refs.iter().all(|v| matches!(v, Value::Ref { .. })));
    assert_eq!(refs[0].class(), Some("TH2PolyBin"));

    // Nothing in the file is left undecoded.
    for key in ["fn", "eff", "gme", "poly"] {
        let dump = f.get_value(key).unwrap().to_string();
        assert!(!dump.contains("<unsupported"), "{key}: {dump}");
    }
}
