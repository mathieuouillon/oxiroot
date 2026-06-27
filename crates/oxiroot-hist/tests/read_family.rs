//! Integration tests for the full classic histogram family (float + 3-D +
//! profile), validated against golden values from ROOT-written fixtures.

use std::path::PathBuf;

use oxiroot_hist::{read_th1f, read_th2f, read_th3d, read_th3f, read_tprofile};
use oxiroot_io_core::RFile;

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

#[test]
fn reads_th1f() {
    let h = read_th1f(&open("th1f_uncompressed.root"), "h1f").expect("TH1F");
    assert_eq!(h.class_name(), "TH1F");
    assert_eq!(h.name, "h1f");
    assert_eq!(h.xaxis.nbins, 5);
    assert_eq!(h.values(), [1.5, 3.0, 4.5, 6.0, 7.5]);
    assert_eq!(h.edges(), vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
}

#[test]
fn reads_th2f() {
    let h = read_th2f(&open("th2f_uncompressed.root"), "h2f").expect("TH2F");
    assert_eq!(h.class_name(), "TH2F");
    assert_eq!((h.nx(), h.ny()), (3, 2));
    assert_eq!(
        h.values(),
        vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]]
    );
}

#[test]
fn reads_th3d() {
    let h = read_th3d(&open("th3d_uncompressed.root"), "h3").expect("TH3D");
    assert_eq!(h.class_name(), "TH3D");
    assert_eq!((h.nx(), h.ny(), h.nz()), (2, 2, 2));
    assert_eq!(h.entries, 8.0);
    let expected = vec![
        vec![vec![1.0, 5.0], vec![3.0, 7.0]],
        vec![vec![2.0, 6.0], vec![4.0, 8.0]],
    ];
    assert_eq!(h.values(), expected);
}

#[test]
fn th3f_matches_th3d_values() {
    let d = read_th3d(&open("th3d_uncompressed.root"), "h3").unwrap();
    let f = read_th3f(&open("th3f_uncompressed.root"), "h3").unwrap();
    assert_eq!(f.class_name(), "TH3F");
    assert_eq!(f.values(), d.values());
}

#[test]
fn reads_tprofile() {
    let p = read_tprofile(&open("tprofile_uncompressed.root"), "p").expect("TProfile");
    assert_eq!(p.name, "p");
    assert_eq!(p.xaxis.nbins, 4);
    assert_eq!(p.edges(), vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    // Per-bin entry counts (with flow): [under, 2, 2, 1, 0, over].
    assert_eq!(p.bin_entries, vec![0.0, 2.0, 2.0, 1.0, 0.0, 0.0]);
    // Profiled means: 30/2, 15/2, 30/1, 0.
    assert_eq!(p.values(), vec![15.0, 7.5, 30.0, 0.0]);
    assert_eq!(p.entries, 5.0);
}
