//! End-to-end `hadd`-style file merging via [`oxiroot::hadd`].
use oxiroot::hadd::{merge_files, MergeKind, Merger};
use oxiroot::prelude::*;

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("oxiroot_hadd_{name}.root"))
}

#[test]
fn sums_histograms_and_copies_other_objects() {
    // Two files, each a TH1 "h" (2 in-range fills) plus a TObjString "meta".
    for (tag, xs) in [("hist_a", [0.5, 1.5]), ("hist_b", [2.5, 3.5])] {
        let mut h = Hist::reg(4, 0.0, 4.0).double().named("h").titled("h");
        for x in xs {
            h.fill(x);
        }
        RootFile::create(tmp(tag))
            .add(&h)
            .add(&TObjString::new("provenance").named("meta"))
            .write(Compression::None)
            .unwrap();
    }

    let out = tmp("hist_out");
    let report = merge_files(&out, &[tmp("hist_a"), tmp("hist_b")], Compression::None).unwrap();

    assert_eq!(report.kind, MergeKind::Histograms);
    assert!(report.merged.contains(&"h".to_string()), "{report}");
    assert!(report.copied.contains(&"meta".to_string()), "{report}");
    assert!(report.skipped.is_empty(), "{report}");

    // The merged histogram is the bin-by-bin sum: 4 in-range entries total.
    let fo = RFile::open(&out).unwrap();
    let h = TH1::read_root(&fo, "h").unwrap();
    assert_eq!(h.integral(), 4.0);
    // The non-summable object was carried over verbatim.
    assert!(TObjString::read_root(&fo, "meta").is_ok());
}

#[test]
fn concatenates_tree_files() {
    Tree::new("Events", vec![Branch::i32("i", vec![0, 1])])
        .write_root(tmp("tree_a"), Compression::None)
        .unwrap();
    Tree::new("Events", vec![Branch::i32("i", vec![2, 3, 4])])
        .write_root(tmp("tree_b"), Compression::None)
        .unwrap();

    let out = tmp("tree_out");
    let report = Merger::new()
        .input(tmp("tree_a"))
        .input(tmp("tree_b"))
        .compression(Compression::None)
        .merge(&out)
        .unwrap();

    assert_eq!(report.kind, MergeKind::Tree("Events".into()));
    assert_eq!(report.entries, Some(5));

    let fo = RFile::open(&out).unwrap();
    let t = TTree::open(&fo, "Events").unwrap();
    assert_eq!(t.num_entries(), 5);
    assert_eq!(
        t.read_branch(&fo, "i").unwrap(),
        BranchValues::I32(vec![0, 1, 2, 3, 4])
    );
}

#[test]
fn concatenates_rntuple_files() {
    Ntuple::new("ntpl", vec![Field::f64("x", vec![0.0, 1.0])])
        .write_root(tmp("rn_a"), Compression::None)
        .unwrap();
    Ntuple::new("ntpl", vec![Field::f64("x", vec![2.0, 3.0, 4.0])])
        .write_root(tmp("rn_b"), Compression::None)
        .unwrap();

    let out = tmp("rn_out");
    let report = merge_files(&out, &[tmp("rn_a"), tmp("rn_b")], Compression::None).unwrap();

    assert_eq!(report.kind, MergeKind::RNTuple("ntpl".into()));
    assert_eq!(report.entries, Some(5));

    let fo = RFile::open(&out).unwrap();
    let nt = RNTuple::open(&fo, "ntpl").unwrap();
    assert_eq!(nt.num_entries(), 5);
    assert_eq!(
        nt.read_field(&fo, "x").unwrap(),
        FieldValues::F64(vec![0.0, 1.0, 2.0, 3.0, 4.0])
    );
}

#[test]
fn refuses_a_fileset_mixing_a_tree_with_histograms() {
    Tree::new("Events", vec![Branch::i32("i", vec![0, 1])])
        .write_root(tmp("mix_tree"), Compression::None)
        .unwrap();
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("h");
    h.fill(1.5);
    h.write_root(tmp("mix_hist"), Compression::None).unwrap();

    let out = tmp("mix_out");
    let Err(err) = merge_files(&out, &[tmp("mix_tree"), tmp("mix_hist")], Compression::None) else {
        panic!("expected a mixed-fileset error");
    };
    assert!(err.to_string().contains("cannot combine"), "{err}");
}
