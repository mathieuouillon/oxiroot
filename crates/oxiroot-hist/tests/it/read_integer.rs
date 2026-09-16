//! Integration test for integer-typed histograms (TArrayC/S/I/L64), read via
//! `TH1::read_root` which detects the precision from the stored class.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TH1};
use oxiroot_io_core::RFile;

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

#[test]
fn reads_integer_histograms() {
    // (fixture, key, class, expected bin contents) from the golden JSON.
    let cases: [(&str, &str, &str, [f64; 5]); 4] = [
        (
            "th1c_uncompressed.root",
            "h1c",
            "TH1C",
            [1.0, 2.0, 3.0, 4.0, 5.0],
        ),
        (
            "th1s_uncompressed.root",
            "h1s",
            "TH1S",
            [100.0, 200.0, 300.0, 400.0, 500.0],
        ),
        (
            "th1i_uncompressed.root",
            "h1i",
            "TH1I",
            [100000.0, 200000.0, 300000.0, 400000.0, 500000.0],
        ),
        (
            "th1l_uncompressed.root",
            "h1l",
            "TH1L",
            // i * 2^40 — beyond 32 bits, exercising the 64-bit TArrayL64 path.
            [
                1099511627776.0,
                2199023255552.0,
                3298534883328.0,
                4398046511104.0,
                5497558138880.0,
            ],
        ),
    ];

    for (file, key, class, expected) in cases {
        let h = TH1::read_root(&open(file), key).expect("read integer TH1");
        assert_eq!(h.class_name(), class, "{file}");
        assert_eq!(h.xaxis.nbins, 5, "{file}");
        assert_eq!(h.values(), expected, "{file}: bin contents");
    }
}

#[test]
fn th1_read_root_rejects_wrong_dimension() {
    // th2f holds a TH2F; asking for a TH1 must fail rather than mis-read.
    let err = TH1::read_root(&open("th2f_uncompressed.root"), "h2f").unwrap_err();
    assert!(matches!(err, oxiroot_io_core::Error::Format(_)));
}
