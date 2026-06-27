//! Item 2: weighted per-bin errors (Sumw2) and variable bin edges, round-tripped
//! through our reader (and, separately, validated in official ROOT).

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, WriteRoot, TH1};
use oxiroot_io_core::RFile;

#[test]
fn weighted_sumw2_round_trips() {
    let mut h = TH1::new(3, 0.0, 3.0).named("hw").titled("weighted");
    h.sumw2(); // enable per-bin error tracking before filling
    h.fill_weight(0.5, 2.0); // bin 1
    h.fill_weight(0.5, 3.0); // bin 1
    h.fill_weight(1.5, 1.0); // bin 2

    // bin 1: content 5, sumw2 = 4 + 9 = 13; bin 2: content 1, sumw2 = 1.
    assert_eq!(h.contents[1], 5.0);
    assert_eq!(h.sumw2[1], 13.0);
    assert!((h.bin_error(1) - 13.0_f64.sqrt()).abs() < 1e-12);
    assert_eq!(h.bin_error(2), 1.0);

    let out = PathBuf::from("/tmp/rootrs_weighted.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    let h2 = TH1::read_root(&f, "hw").expect("read back");
    assert_eq!(h2, h, "weighted histogram (incl. Sumw2) must round-trip");
    assert_eq!(h2.sumw2[1], 13.0, "Sumw2 survived write->read");
}

#[test]
fn variable_bins_round_trip() {
    let edges = [0.0, 1.0, 4.0, 10.0]; // 3 bins: [0,1) [1,4) [4,10)
    let mut h = TH1::new_variable(&edges).named("hv").titled("var");
    h.fill(0.5); // bin 1
    h.fill(2.0); // bin 2
    h.fill(3.9); // bin 2
    h.fill(5.0); // bin 3
    h.fill(-1.0); // underflow
    h.fill(99.0); // overflow

    assert_eq!(h.edges(), edges, "variable edges");
    assert_eq!(h.values(), [1.0, 2.0, 1.0], "bin contents");
    assert_eq!(h.entries, 6.0);

    let out = PathBuf::from("/tmp/rootrs_varbins.root");
    h.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    let h2 = TH1::read_root(&f, "hv").expect("read back");
    assert_eq!(h2, h, "variable-bin histogram must round-trip");
    assert_eq!(h2.edges(), edges, "edges survived write->read");
}
