//! The idiomatic trait API (`WriteRoot`/`ReadRoot`/`Precision`) must produce
//! byte-identical output to the legacy free functions (so ROOT compatibility is
//! preserved) and round-trip through a real file.

use oxiroot_hist::{
    th1d_to_bytes, th1f_to_bytes, write_root_file, Compression, Precision, ReadRoot, TProfile,
    WriteRoot, TH1, TH2,
};
use oxiroot_io_core::RFile;

fn sample() -> TH1 {
    let mut h = TH1::new("h", "title", 10, 0.0, 10.0);
    h.sumw2();
    for i in 0..10 {
        h.fill_weight(i as f64 + 0.5, (i + 1) as f64);
    }
    h
}

#[test]
fn trait_bytes_match_legacy_functions() {
    let h = sample();
    // Default precision (TH1D) via the trait == the legacy th1d_to_bytes.
    assert_eq!(h.to_root_bytes(), th1d_to_bytes(&h));
    assert_eq!(h.root_class(), "TH1D");
    assert_eq!(h.precision(), Precision::Double);

    // Float precision via with_precision == the legacy th1f_to_bytes.
    let hf = sample().with_precision(Precision::Float);
    assert_eq!(hf.to_root_bytes(), th1f_to_bytes(&h));
    assert_eq!(hf.root_class(), "TH1F");
    assert_eq!(hf.precision(), Precision::Float);
}

#[test]
fn write_root_then_read_root_round_trips() {
    let h = sample();
    let path = std::env::temp_dir().join("oxiroot_traitapi_h.root");
    h.write_root(&path, Compression::Zstd(3))
        .expect("write_root");

    let f = RFile::open(&path).expect("open");
    let back = TH1::read_root(&f, "h").expect("read_root");
    assert_eq!(back.values(), h.values());
    assert_eq!(back.name, "h");
    assert_eq!(back.class_name, "TH1D");
}

#[test]
fn float_precision_round_trips_as_th1f() {
    let h = sample().with_precision(Precision::Float);
    let path = std::env::temp_dir().join("oxiroot_traitapi_hf.root");
    h.write_root(&path, Compression::None).expect("write");
    let f = RFile::open(&path).expect("open");
    let back = TH1::read_root(&f, "h").expect("read");
    assert_eq!(back.class_name, "TH1F"); // precision preserved on round-trip
}

#[test]
fn write_root_file_handles_heterogeneous_objects() {
    // The new multi-object writer takes any mix of writable types via &dyn —
    // not just TH1/TH2/TH3 as the old Hist enum did.
    let h1 = TH1::new("h1", "", 5, 0.0, 5.0);
    let h2 = TH2::new("h2", "", 4, 0.0, 4.0, 4, 0.0, 4.0);
    let p = TProfile::new("p", "", 5, 0.0, 5.0);
    let path = std::env::temp_dir().join("oxiroot_traitapi_multi.root");
    write_root_file(&path, &[&h1, &h2, &p], Compression::None).expect("write_root_file");

    let f = RFile::open(&path).expect("open");
    assert!(TH1::read_root(&f, "h1").is_ok());
    assert!(TH2::read_root(&f, "h2").is_ok());
    assert!(TProfile::read_root(&f, "p").is_ok());
}
