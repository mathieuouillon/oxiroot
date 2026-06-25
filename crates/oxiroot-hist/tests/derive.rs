//! Tier-2 derived histograms: rebin, cumulative, projections, profiles. Checks
//! contents, that moment sums propagate (so `mean`/`std_dev` stay correct), and
//! that the derived `TH1`s round-trip through write→read.

use std::path::PathBuf;

use oxiroot_hist::{read_th1d, write_th1d_file, TH1, TH2};
use oxiroot_io_core::{Compression, RFile};

#[test]
fn rebin_sums_groups_and_keeps_moments() {
    let mut h = TH1::new("h", "", 6, 0.0, 6.0);
    for (bin, n) in (1..=6).enumerate() {
        for _ in 0..n {
            h.fill(bin as f64 + 0.5); // bin `bin+1` gets `n` entries → contents [1..6]
        }
    }
    assert_eq!(h.values(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

    let r = h.rebin(2);
    assert_eq!(r.xaxis.nbins, 3);
    assert_eq!(r.values(), &[3.0, 7.0, 11.0], "adjacent bins summed");
    assert_eq!(r.edges(), vec![0.0, 2.0, 4.0, 6.0]);
    // Moments are the original data's, not recomputed from the coarse bins.
    assert!(
        (r.mean() - h.mean()).abs() < 1e-12,
        "mean preserved by rebin"
    );
    assert_eq!(r.entries, h.entries);
}

#[test]
fn cumulative_forward_and_backward() {
    let mut h = TH1::new("h", "", 4, 0.0, 4.0);
    for (bin, n) in [(0, 1), (1, 2), (2, 3), (3, 4)] {
        for _ in 0..n {
            h.fill(bin as f64 + 0.5);
        }
    }
    assert_eq!(h.cumulative(true).values(), &[1.0, 3.0, 6.0, 10.0]);
    assert_eq!(h.cumulative(false).values(), &[10.0, 9.0, 7.0, 4.0]);
}

fn sample_th2() -> TH2 {
    // 2x2 over [0,2)². Cells (ix,iy): (1,1)=1,(1,2)=1,(2,1)=3,(2,2)=0.
    let mut h = TH2::new("h", "", 2, 0.0, 2.0, 2, 0.0, 2.0);
    h.fill(0.5, 0.5);
    h.fill(0.5, 1.5);
    for _ in 0..3 {
        h.fill(1.5, 0.5);
    }
    h
}

#[test]
fn projection_x_sums_y_and_keeps_x_moments() {
    let h = sample_th2();
    let px = h.projection_x("px");
    assert_eq!(px.xaxis.nbins, 2);
    assert_eq!(px.values(), &[2.0, 3.0], "sum over y per x bin");
    assert_eq!(px.entries, h.entries);
    assert!(
        (px.mean() - h.mean_x()).abs() < 1e-12,
        "x mean carries over"
    );

    // The projection is an ordinary TH1D — round-trips through ROOT's format.
    let out = PathBuf::from("/tmp/oxiroot_projx.root");
    write_th1d_file(&out, &px, Compression::None).expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(read_th1d(&f, "px").unwrap(), px, "projection round-trips");
}

#[test]
fn projection_y_sums_x() {
    let h = sample_th2();
    let py = h.projection_y("py");
    // y bin1 = (1,1)+(2,1) = 1+3 = 4; y bin2 = (1,2)+(2,2) = 1+0 = 1.
    assert_eq!(py.values(), &[4.0, 1.0]);
    assert!(
        (py.mean() - h.mean_y()).abs() < 1e-12,
        "y mean carries over"
    );
}

#[test]
fn profile_x_means_per_x_bin() {
    let h = sample_th2();
    let p = h.profile_x("p");
    // x bin1: y centers 0.5,1.5 with counts 1,1 → mean 1.0.
    // x bin2: y center 0.5 count 3 → mean 0.5.
    let v = p.values();
    assert!((v[0] - 1.0).abs() < 1e-12, "x bin1 profile mean");
    assert!((v[1] - 0.5).abs() < 1e-12, "x bin2 profile mean");
}
