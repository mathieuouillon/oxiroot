//! `TMap` — ROOT's keyed map of object → object. oxiroot reads the
//! ROOT-C++-written `tmap.root` fixture (string keys → a TObjString, a
//! TParameter, a TH1F) and round-trips its own writes; ROOT C++ reads oxiroot's
//! output (checked out of band). uproot has no `TMap` model, so a `TMap` is not
//! readable there — a uproot limitation that ROOT's own maps share.

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, RootFile, TMap, TObjString, TParameter, TH1};
use oxiroot_io_core::{Compression, RFile};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tmap.root"))
        .expect("open fixture")
}

#[test]
fn reads_root_written_map() {
    let f = fixture();
    let m = TMap::read_root(&f, "meta").unwrap();
    assert_eq!(m.name(), "meta");
    assert_eq!(m.len(), 3);
    assert_eq!(m.string_keys(), ["version", "lumi", "hist"]);
    assert_eq!(
        m.get::<TObjString>("version").unwrap().unwrap().value(),
        "2.1"
    );
    assert_eq!(
        m.get::<TParameter>("lumi")
            .unwrap()
            .unwrap()
            .value()
            .as_f64(),
        137.5
    );
    assert_eq!(m.values::<TH1>().unwrap().len(), 1);
    // A miss: no such key, or wrong value type.
    assert!(m.get::<TObjString>("missing").is_none());
    assert!(m.get::<TH1>("version").is_none());
}

#[test]
fn round_trips_map_through_oxiroot() {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("h");
    h.fill(0.5);
    let map = TMap::new()
        .named("meta")
        .insert("version", &TObjString::new("2.1"))
        .insert("lumi", &TParameter::f64("lumi", 137.5))
        .insert("hist", &h);

    let out = std::env::temp_dir().join("oxiroot_tmap_rt.root");
    RootFile::create(&out)
        .add(&map)
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&out).unwrap();
    let m = TMap::read_root(&f, "meta").unwrap();
    assert_eq!(m.len(), 3);
    assert_eq!(m.string_keys(), ["version", "lumi", "hist"]);
    assert_eq!(
        m.get::<TObjString>("version").unwrap().unwrap().value(),
        "2.1"
    );
    assert_eq!(
        m.get::<TParameter>("lumi")
            .unwrap()
            .unwrap()
            .value()
            .as_f64(),
        137.5
    );
    assert_eq!(m.values::<TH1>().unwrap().len(), 1);
    let _ = std::fs::remove_file(&out);
}
