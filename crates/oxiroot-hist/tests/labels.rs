//! Read alphanumeric (labelled) histogram axes (`TAxis::fLabels`).

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TH1};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_alphanumeric_axis_labels() {
    let f = RFile::open(fixture("analysis.root")).expect("open");
    let h = TH1::read_root(&f, "hl").expect("read labelled TH1D");

    assert!(h.xaxis.is_labelled());
    assert_eq!(h.xaxis.labels, ["apple", "banana", "cherry"]);
    assert_eq!(h.xaxis.bin_label(1), Some("apple"));
    assert_eq!(h.xaxis.bin_label(2), Some("banana"));
    assert_eq!(h.xaxis.find_label("cherry"), Some(3));
    assert_eq!(h.xaxis.find_label("durian"), None);

    // Labels line up with the filled bin contents.
    assert_eq!(h.contents[1], 5.0); // apple
    assert_eq!(h.contents[2], 2.0); // banana
    assert_eq!(h.contents[3], 8.0); // cherry
}

#[test]
fn numeric_axis_has_no_labels() {
    let f = RFile::open(fixture("analysis.root")).expect("open");
    let h = TH1::read_root(&f, "h").expect("read numeric TH1D");
    assert!(!h.xaxis.is_labelled());
    assert!(h.xaxis.labels.is_empty());
    assert_eq!(h.xaxis.bin_label(1), None);
}

#[test]
fn writes_and_round_trips_labels() {
    use oxiroot_hist::WriteRoot;
    use oxiroot_io_core::Compression;

    // Round-trip the fixture's labelled histogram through the write path.
    let f = RFile::open(fixture("analysis.root")).expect("open");
    let src = TH1::read_root(&f, "hl").expect("read");
    let out = std::env::temp_dir().join("oxiroot_labels_rt.root");
    src.write_root(&out, Compression::None).expect("write");
    let back = TH1::read_root(&RFile::open(&out).unwrap(), "hl").unwrap();
    assert_eq!(back.xaxis.labels, src.xaxis.labels);

    // Build a labelled histogram from scratch.
    let mut h = TH1::new(3, 0.0, 3.0).named("cuts").titled("selection");
    h.xaxis.set_label(1, "trigger");
    h.xaxis.set_label(2, "vertex");
    h.xaxis.set_label(3, "isolation");
    let out = std::env::temp_dir().join("oxiroot_labels_scratch.root");
    h.write_root(&out, Compression::None).expect("write");
    let r = TH1::read_root(&RFile::open(&out).unwrap(), "cuts").unwrap();
    assert_eq!(r.xaxis.labels, ["trigger", "vertex", "isolation"]);
    assert_eq!(r.xaxis.bin_label(2), Some("vertex"));
}
