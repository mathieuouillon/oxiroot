//! M2 integration test: parse the TStreamerInfo list and verify it against the
//! fixture's known classes (cross-checked with uproot's `file.streamers`).

use std::path::PathBuf;

use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn parses_streamer_registry() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let reg = f.streamer_registry().expect("parse streamer info");

    // The exact set of classes uproot reports for this file.
    let mut names = reg.class_names();
    names.sort_unstable();
    let expected = [
        "TAttAxis",
        "TAttFill",
        "TAttLine",
        "TAttMarker",
        "TAxis",
        "TCollection",
        "TH1",
        "TH1D",
        "THashList",
        "TList",
        "TNamed",
        "TObject",
        "TSeqCollection",
        "TString",
    ];
    assert_eq!(names, expected);

    // Versions match what ROOT wrote.
    assert_eq!(reg.get("TH1D").unwrap().class_version, 3);
    assert_eq!(reg.get("TH1").unwrap().class_version, 8);
    assert_eq!(reg.get("TAxis").unwrap().class_version, 10);
    assert_eq!(reg.get("TH1D").unwrap().checksum, 4189148831);

    // TH1D's streamer is two base classes: TH1 and TArrayD.
    let th1d = reg.get("TH1D").unwrap();
    let bases: Vec<(&str, &str)> = th1d
        .elements
        .iter()
        .map(|e| (e.element_class.as_str(), e.name.as_str()))
        .collect();
    assert_eq!(
        bases,
        vec![("TStreamerBase", "TH1"), ("TStreamerBase", "TArrayD")]
    );

    // A basic-type member inside TAxis (fNbins) should be present and typed.
    let taxis = reg.get("TAxis").unwrap();
    let fnbins = taxis
        .elements
        .iter()
        .find(|e| e.name == "fNbins")
        .expect("fNbins");
    assert_eq!(fnbins.type_name, "int");
}
