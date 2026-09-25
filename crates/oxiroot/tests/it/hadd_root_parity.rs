//! `merge_files` against ROOT's own `hadd`.
//!
//! `fixtures/hadd_a.root` and `fixtures/hadd_b.root` hold one object of every
//! class oxiroot's merge handles, written by ROOT
//! (`scripts/gen_hadd_inputs.cpp`). `fixtures/hadd_merged.root` is what ROOT's
//! `hadd` made of them. oxiroot must reach the same answer for every object
//! ROOT merges, and copy the first input's object for the rest — which is what
//! ROOT does too, except that it writes one key per input, which oxiroot cannot
//! do (it rejects two objects of the same name in one directory).

use std::path::PathBuf;

use oxiroot::hadd::merge_files;
use oxiroot::hist::{
    Efficiency, Graph, Graph2D, GraphStack, Hist1D, HistStack, Parameter, PolyHist, Profile1D,
    ReadRoot, SparseHist,
};
use oxiroot::{Compression, FileReader};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"))
}

/// oxiroot's merge of the two ROOT inputs, and ROOT's `hadd` of the same two.
fn merged(tag: &str) -> (FileReader, FileReader) {
    let out = std::env::temp_dir().join(format!("oxiroot_hadd_parity_{tag}.root"));
    let inputs = [fixture("hadd_a.root"), fixture("hadd_b.root")];
    let report = merge_files(&out, &inputs, Compression::None).expect("merge");
    for key in ["h", "p", "eff", "poly", "sp", "g", "ge", "ga", "st", "par"] {
        assert!(
            report.merged.iter().any(|k| k == key),
            "{key} was not merged: {report}"
        );
    }
    for key in ["fn", "g2", "mg", "s"] {
        assert!(
            report.copied.iter().any(|k| k == key),
            "{key} was not copied: {report}"
        );
    }
    assert!(report.skipped.is_empty(), "{report}");
    (
        FileReader::open(&out).expect("open oxiroot's merge"),
        FileReader::open(fixture("hadd_merged.root")).expect("open ROOT's merge"),
    )
}

#[test]
fn histograms_and_profiles_match_root() {
    let (ours, root) = merged("hists");
    let h = Hist1D::read_root(&ours, "h").unwrap();
    assert_eq!(h.contents, Hist1D::read_root(&root, "h").unwrap().contents);
    assert_eq!(h.entries, 2.0);

    let p = Profile1D::read_root(&ours, "p").unwrap();
    let root_p = Profile1D::read_root(&root, "p").unwrap();
    assert_eq!(p.sums, root_p.sums);
    assert_eq!(p.bin_entries, root_p.bin_entries);
}

#[test]
fn an_efficiency_sums_its_histograms_like_root() {
    let (ours, root) = merged("eff");
    let e = Efficiency::read_root(&ours, "eff").unwrap();
    let root_e = Efficiency::read_root(&root, "eff").unwrap();
    assert_eq!(e.passed.contents, root_e.passed.contents);
    assert_eq!(e.total.contents, root_e.total.contents);
    // One passed of two in the first file, two of three in the second.
    assert_eq!(e.passed.integral(), 3.0);
    assert_eq!(e.total.integral(), 5.0);
}

#[test]
fn a_poly_and_a_sparse_histogram_sum_their_bins_like_root() {
    let (ours, root) = merged("bins");
    let poly = PolyHist::read_root(&ours, "poly").unwrap();
    let root_poly = PolyHist::read_root(&root, "poly").unwrap();
    let contents: Vec<f64> = poly.bins.iter().map(|b| b.content).collect();
    let root_contents: Vec<f64> = root_poly.bins.iter().map(|b| b.content).collect();
    assert_eq!(contents, root_contents);
    assert_eq!(contents, vec![3.0, 4.0]);
    assert_eq!(poly.entries, root_poly.entries);

    let sp = SparseHist::read_root(&ours, "sp").unwrap();
    let root_sp = SparseHist::read_root(&root, "sp").unwrap();
    let mut ours_bins: Vec<(Vec<i32>, f64)> = sp
        .bins
        .iter()
        .map(|b| (b.coords.clone(), b.content))
        .collect();
    let mut root_bins: Vec<(Vec<i32>, f64)> = root_sp
        .bins
        .iter()
        .map(|b| (b.coords.clone(), b.content))
        .collect();
    ours_bins.sort_by(|a, b| a.0.cmp(&b.0));
    root_bins.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(ours_bins, root_bins);
    assert_eq!(sp.bins.iter().map(|b| b.content).sum::<f64>(), 9.0);
}

#[test]
fn graphs_append_their_points_like_root() {
    let (ours, root) = merged("graphs");
    for key in ["g", "ge", "ga"] {
        let g = Graph::read_root(&ours, key).unwrap();
        let root_g = Graph::read_root(&root, key).unwrap();
        assert_eq!((key, &g.x, &g.y), (key, &root_g.x, &root_g.y));
        assert_eq!(g.errors, root_g.errors, "{key}");
        // Two points from the first file, three from the second.
        assert_eq!(g.x.len(), 5, "{key}");
    }
}

#[test]
fn a_stack_merges_its_histograms_and_a_parameter_sums_like_root() {
    let (ours, root) = merged("stack");
    let st = HistStack::read_root(&ours, "st").unwrap();
    let root_st = HistStack::read_root(&root, "st").unwrap();
    assert_eq!(st.hists().len(), 1);
    assert_eq!(st.hists()[0].contents, root_st.hists()[0].contents);
    assert_eq!(st.hists()[0].integral(), 3.0);

    let par = Parameter::read_root(&ours, "par").unwrap();
    assert_eq!(
        par.value(),
        Parameter::read_root(&root, "par").unwrap().value()
    );
}

#[test]
fn what_root_does_not_merge_is_the_first_input() {
    let (ours, _) = merged("copied");
    let first = FileReader::open(fixture("hadd_a.root")).unwrap();

    // ROOT writes one key per input for these; oxiroot keeps the first.
    let g2 = Graph2D::read_root(&ours, "g2").unwrap();
    assert_eq!(g2.x, Graph2D::read_root(&first, "g2").unwrap().x);
    let mg = GraphStack::read_root(&ours, "mg").unwrap();
    assert_eq!(mg.graphs().len(), 1);
    assert_eq!(mg.graphs()[0].x, [0.0, 1.0]);
}
