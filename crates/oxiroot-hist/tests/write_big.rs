//! The 64-bit ("big") TFile container form for the one-shot object writers, used
//! automatically once a file would exceed 2 GiB. The big-file threshold is
//! lowered here (via `RootFile::write_threshold`) so the wide header/directory/
//! key path is exercised without producing a 2 GiB file; the result must read
//! back through our own reader with every object intact.

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, RootFile, TH1, TH2};
use oxiroot_io_core::RFile;

fn th1(name: &str) -> TH1 {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named(name).titled("1-D");
    for x in [0.5, 1.5, 1.5, 3.5] {
        h.fill(x);
    }
    h
}

#[test]
fn big_container_flat_round_trips() {
    let out = PathBuf::from("/tmp/oxiroot_big_flat.root");
    let h1 = th1("hx");
    let mut h2 = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("hxy")
        .titled("2-D");
    h2.fill(0.5, 0.5);
    h2.fill(1.5, 1.5);

    // Threshold 0 forces the 64-bit container form even for this tiny file.
    RootFile::create(&out)
        .add(&h1)
        .add(&h2)
        .write_threshold(oxiroot_io_core::Compression::None, 0)
        .expect("write big");

    let f = RFile::open(&out).expect("reopen");
    assert!(
        f.header().is_big(),
        "forced-big file must use fVersion>=1e6"
    );
    assert_eq!(f.header().units, 8, "big-format fUnits is 8");
    let keys: Vec<(&str, &str)> = f
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert_eq!(keys, vec![("hx", "TH1D"), ("hxy", "TH2D")]);
    assert_eq!(TH1::read_root(&f, "hx").expect("read hx"), h1);
    assert_eq!(TH2::read_root(&f, "hxy").expect("read hxy"), h2);
}

#[test]
fn big_container_with_subdirs_round_trips() {
    let out = PathBuf::from("/tmp/oxiroot_big_dirs.root");
    let top = th1("top");
    let inner = th1("inner");

    RootFile::create(&out)
        .add(&top)
        .dir("region_a", |d| d.add(&inner))
        .write_threshold(oxiroot_io_core::Compression::None, 0)
        .expect("write big dirs");

    let f = RFile::open(&out).expect("reopen");
    assert!(f.header().is_big());
    assert_eq!(TH1::read_root(&f, "top").expect("top"), top);
    // The histogram inside the big-format subdirectory reads back too.
    assert_eq!(
        TH1::read_root_in(&f, "region_a", "inner").expect("inner"),
        inner
    );
}
