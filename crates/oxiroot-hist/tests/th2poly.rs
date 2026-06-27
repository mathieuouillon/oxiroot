//! TH2Poly: read a ROOT-written fixture, resolving the bins through ROOT's
//! object-reference map (the bins are written full inside `fCells`, with
//! back-references in `fBins`).

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, TH2Poly, WriteRoot};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_root_written_th2poly() {
    // Fixture `hp`: TH2Poly("hp","poly",0,2,0,2) with two unit-square bins,
    // AddBin(0,0,1,1) filled once (content 1) and AddBin(1,1,2,2) filled with
    // weight 3 (content 3).
    let f = RFile::open(fixture("th2poly.root")).expect("open");
    let h = TH2Poly::read_root(&f, "hp").expect("read");

    assert_eq!(h.name, "hp");
    assert_eq!(h.title, "poly");
    assert_eq!(h.entries, 2.0);
    assert_eq!(h.tsumw, 4.0);
    assert_eq!(h.nbins(), 2);

    let b1 = h.bin(1).expect("bin 1");
    assert_eq!(b1.content, 1.0);
    assert_eq!((b1.xmin, b1.ymin, b1.xmax, b1.ymax), (0.0, 0.0, 1.0, 1.0));
    assert_eq!(b1.x, vec![0.0, 0.0, 1.0, 1.0, 0.0]);
    assert_eq!(b1.y, vec![0.0, 1.0, 1.0, 0.0, 0.0]);

    let b2 = h.bin(2).expect("bin 2");
    assert_eq!(b2.content, 3.0);
    assert_eq!((b2.xmin, b2.ymin, b2.xmax, b2.ymax), (1.0, 1.0, 2.0, 2.0));
    assert_eq!(b2.x, vec![1.0, 1.0, 2.0, 2.0, 1.0]);
    assert_eq!(b2.y, vec![1.0, 2.0, 2.0, 1.0, 1.0]);
}

#[test]
fn reads_honeycomb_th2poly() {
    // A 4×4 honeycomb (14 hexagonal bins). Each bin appears in several `fCells`
    // grid cells and again in `fBins`, so this exercises ROOT's object-reference
    // map heavily: every bin must be read in full exactly once. Cross-checked
    // bit-for-bit against compiled ROOT C++ (`GetBins()` / `GetPolygon()`).
    let f = RFile::open(fixture("th2poly_honeycomb.root")).expect("open");
    let h = TH2Poly::read_root(&f, "hc").expect("read");

    assert_eq!(h.title, "honeycomb");
    assert_eq!(h.nbins(), 14);
    // Every bin is a hexagon (6 vertices).
    assert!(h.bins.iter().all(|b| b.x.len() == 6 && b.y.len() == 6));
    // Numbers are the full 1..=14 set, no duplicates dropped or doubled.
    let mut nums: Vec<i32> = h.bins.iter().map(|b| b.number).collect();
    nums.sort_unstable();
    assert_eq!(nums, (1..=14).collect::<Vec<_>>());

    // The fills landed in three bins (cross-checked with ROOT C++).
    assert_eq!(h.bin(1).unwrap().content, 3.5);
    assert_eq!(h.bin(2).unwrap().content, 1.0);
    assert_eq!(h.bin(5).unwrap().content, 4.0);

    // A regular hexagon with side 1 has area 3√3/2 ≈ 2.598; ROOT computes the
    // same value lazily in GetArea().
    let expected = 1.5 * 3.0_f64.sqrt();
    assert!((h.bin(1).unwrap().polygon_area() - expected).abs() < 1e-9);
}

/// Read a ROOT fixture, write it back with oxiroot, and read the result: the
/// bins must survive byte-for-byte. (Cross-checked against compiled ROOT C++,
/// which reads the oxiroot-written file identically to ROOT's own.)
#[test]
fn th2poly_round_trips() {
    for (file, name) in [("th2poly.root", "hp"), ("th2poly_honeycomb.root", "hc")] {
        let h = TH2Poly::read_root(&RFile::open(fixture(file)).unwrap(), name).unwrap();
        let out = std::env::temp_dir().join(format!("oxiroot_{name}.root"));
        h.write_root(&out, Compression::None).expect("write");
        let back = TH2Poly::read_root(&RFile::open(&out).unwrap(), name).unwrap();
        assert_eq!(back.bins, h.bins, "{name} bins changed across round-trip");
        assert_eq!(back.name, h.name);
        assert_eq!(back.title, h.title);
        assert_eq!(back.entries, h.entries);
    }
}

/// Build a `TH2Poly` from scratch (rectangular + polygon bins, filled), write
/// it, and read it back. (Cross-checked: ROOT C++ reads this file correctly,
/// routing each fill to its bin via point-in-polygon, including the triangle.)
#[test]
fn th2poly_build_from_scratch() {
    let mut h = TH2Poly::new(0.0, 3.0, 0.0, 3.0)
        .named("scratch")
        .titled("built");
    assert_eq!(h.add_bin_rect(0.0, 0.0, 1.0, 1.0), 1);
    assert_eq!(h.add_bin_rect(1.0, 1.0, 2.0, 2.0), 2);
    assert_eq!(h.add_bin(&[2.0, 3.0, 2.5], &[2.0, 2.0, 3.0]), 3); // a triangle
    assert_eq!(h.fill(0.5, 0.5), 1);
    assert_eq!(h.fill_weight(1.5, 1.5, 4.0), 2);
    assert_eq!(h.fill(2.5, 2.3), 3); // inside the triangle
    assert_eq!(h.fill(2.5, 2.3), 3);
    assert_eq!(h.fill(10.0, 10.0), 0); // outside every bin -> overflow

    let out = std::env::temp_dir().join("oxiroot_th2poly_scratch.root");
    h.write_root(&out, Compression::Zstd(5)).expect("write");
    let back = TH2Poly::read_root(&RFile::open(&out).unwrap(), "scratch").unwrap();

    assert_eq!(back.nbins(), 3);
    assert_eq!(back.bin(1).unwrap().content, 1.0);
    assert_eq!(back.bin(2).unwrap().content, 4.0);
    assert_eq!(back.bin(3).unwrap().content, 2.0);
    assert_eq!(back.bin(3).unwrap().x, vec![2.0, 3.0, 2.5]);
    assert_eq!(back.entries, 5.0);
}
