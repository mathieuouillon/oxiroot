//! Constructors and fills that pair inputs reject inputs of different lengths
//! instead of truncating or padding them, and leave the object unchanged.

use oxiroot_hist::{Hist, TGraph, TGraph2D, TGraphMultiErrors, TH2Poly, THnSparse};
use oxiroot_io_core::Error;

fn mismatch(what: &str, expected: usize, found: usize) -> Error {
    Error::LengthMismatch {
        what: what.into(),
        expected,
        found,
    }
}

#[test]
fn graph_constructors_reject_vectors_of_different_lengths() {
    let two = || vec![1.0, 2.0];
    let one = || vec![1.0];
    assert_eq!(
        TGraph::new(two(), one()).unwrap_err(),
        mismatch("TGraph y", 2, 1)
    );
    assert_eq!(
        TGraph::with_errors(two(), two(), one(), two()).unwrap_err(),
        mismatch("TGraphErrors ex", 2, 1)
    );
    assert_eq!(
        TGraph::with_asymm_errors(two(), two(), two(), two(), two(), one()).unwrap_err(),
        mismatch("TGraphAsymmErrors ey_high", 2, 1)
    );
    assert_eq!(
        TGraph2D::new(two(), two(), one()).unwrap_err(),
        mismatch("TGraph2D z", 2, 1)
    );
    assert_eq!(
        TGraphMultiErrors::new(two(), two(), one(), two(), two(), two()).unwrap_err(),
        mismatch("TGraphMultiErrors ex_low", 2, 1)
    );
    let g = TGraphMultiErrors::new(two(), two(), two(), two(), two(), two()).unwrap();
    assert_eq!(
        g.add_y_error(two(), one()).unwrap_err(),
        mismatch("TGraphMultiErrors ey_high", 2, 1)
    );
    // Equal lengths still build.
    assert_eq!(TGraph::new(two(), vec![3.0, 4.0]).unwrap().len(), 2);
    assert_eq!(TGraph2D::new(two(), two(), two()).unwrap().len(), 2);
}

#[test]
fn fill_many_weighted_fills_nothing_on_a_mismatch() {
    let mut h = Hist::reg(4, 0.0, 4.0).double();
    assert_eq!(
        h.fill_many_weighted(&[0.5, 1.5, 2.5], &[1.0, 2.0])
            .unwrap_err(),
        mismatch("fill weights", 3, 2)
    );
    assert_eq!(h.entries, 0.0);
    assert!(h.values().iter().all(|&v| v == 0.0));
    h.fill_many_weighted(&[0.5, 1.5], &[1.0, 2.0]).unwrap();
    assert_eq!(h.values(), &[1.0, 2.0, 0.0, 0.0]);
}

#[test]
fn th2poly_add_bin_adds_nothing_on_a_mismatch() {
    let mut h = TH2Poly::new(0.0, 1.0, 0.0, 1.0);
    assert_eq!(
        h.add_bin(&[0.0, 1.0, 1.0], &[0.0, 0.0]).unwrap_err(),
        mismatch("TH2Poly bin y", 3, 2)
    );
    assert!(h.bins.is_empty());
    assert_eq!(h.add_bin(&[0.0, 1.0, 1.0], &[0.0, 0.0, 1.0]).unwrap(), 1);
    assert_eq!(h.add_bin_rect(0.0, 0.0, 0.5, 0.5), 2);
}

#[test]
fn thnsparse_fill_needs_one_coordinate_per_dimension() {
    let mut h = THnSparse::new(&[(2, 0.0, 2.0), (2, 0.0, 2.0)]);
    assert_eq!(
        h.fill(&[0.5]).unwrap_err(),
        mismatch("THnSparse fill coordinates", 2, 1)
    );
    assert_eq!(
        h.fill(&[0.5, 0.5, 0.5]).unwrap_err(),
        mismatch("THnSparse fill coordinates", 2, 3)
    );
    assert_eq!(h.entries, 0.0);
    assert!(h.bins.is_empty());
    h.fill(&[0.5, 1.5]).unwrap();
    assert_eq!(h.bins.len(), 1);
}
