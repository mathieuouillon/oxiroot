//! Read alphanumeric (labelled) histogram axes (`TAxis::fLabels`).

use std::path::PathBuf;

use oxiroot_hist::read_th1d;
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_alphanumeric_axis_labels() {
    let f = RFile::open(fixture("analysis.root")).expect("open");
    let h = read_th1d(&f, "hl").expect("read labelled TH1D");

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
    let h = read_th1d(&f, "h").expect("read numeric TH1D");
    assert!(!h.xaxis.is_labelled());
    assert!(h.xaxis.labels.is_empty());
    assert_eq!(h.xaxis.bin_label(1), None);
}
