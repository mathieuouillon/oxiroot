//! TH2Poly: read a ROOT-written fixture, resolving the bins through ROOT's
//! object-reference map (the bins are written full inside `fCells`, with
//! back-references in `fBins`).

use std::path::PathBuf;

use oxiroot_hist::read_th2poly;
use oxiroot_io_core::RFile;

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
    let h = read_th2poly(&f, "hp").expect("read");

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
    let h = read_th2poly(&f, "hc").expect("read");

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
