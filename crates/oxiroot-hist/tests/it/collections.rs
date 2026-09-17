//! Collection objects: `THStack` (a stack of histograms) and `TMultiGraph`
//! (several graphs). oxiroot reads the ROOT-C++-written `collections.root`
//! fixture — whose second list member uses a class back-reference — and
//! round-trips its own writes; ROOT C++ and uproot read oxiroot's output
//! (checked out of band).

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, RootFile, TGraph, THStack, TMultiGraph};
use oxiroot_io_core::{Compression, RFile};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/collections.root"))
        .expect("open fixture")
}

#[test]
fn reads_root_written_collections() {
    let f = fixture();

    let hs = THStack::read_root(&f, "hs").unwrap();
    assert_eq!(hs.name(), "hs");
    assert_eq!(hs.title(), "my stack");
    assert_eq!(hs.hists().len(), 2);
    assert_eq!(hs.hists()[0].name(), "ha");
    assert_eq!(hs.hists()[1].name(), "hb");
    // ha has bins 1,2 set; hb has bins 3,4 set (in-range bins only shown here).
    assert_eq!(hs.hists()[0].values(), &[1.0, 2.0, 0.0, 0.0]);
    assert_eq!(hs.hists()[1].values(), &[0.0, 0.0, 3.0, 4.0]);

    let mg = TMultiGraph::read_root(&f, "mg").unwrap();
    assert_eq!(mg.name(), "mg");
    assert_eq!(mg.title(), "my multigraph");
    assert_eq!(mg.graphs().len(), 2);
    assert_eq!(mg.graphs()[0].name, "g1");
    assert_eq!(mg.graphs()[0].y, vec![1.0, 2.0, 3.0]);
    assert_eq!(mg.graphs()[1].name, "g2");
    assert_eq!(mg.graphs()[1].y, vec![3.0, 2.0, 1.0]);
}

#[test]
fn round_trips_collections_through_oxiroot() {
    let mut ha = Hist::reg(4, 0.0, 4.0).double().named("ha").titled("a");
    ha.fill(0.5);
    let hb = Hist::reg(4, 0.0, 4.0).double().named("hb").titled("b");
    let stack = THStack::new()
        .named("hs")
        .titled("my stack")
        .add(ha)
        .add(hb);

    let multi = TMultiGraph::new()
        .named("mg")
        .titled("my multigraph")
        .add(TGraph::new(vec![0.0, 1.0, 2.0], vec![1.0, 2.0, 3.0]).named("g1"))
        .add(TGraph::new(vec![0.0, 1.0, 2.0], vec![3.0, 2.0, 1.0]).named("g2"));

    let out = std::env::temp_dir().join("oxiroot_collections_rt.root");
    RootFile::create(&out)
        .add(&stack)
        .add(&multi)
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&out).unwrap();
    let hs = THStack::read_root(&f, "hs").unwrap();
    assert_eq!(hs.hists().len(), 2);
    assert_eq!(hs.hists()[0].name(), "ha");
    assert_eq!(hs.hists()[0].values(), &[1.0, 0.0, 0.0, 0.0]); // the one fill
    let mg = TMultiGraph::read_root(&f, "mg").unwrap();
    assert_eq!(mg.graphs().len(), 2);
    assert_eq!(mg.graphs()[1].y, vec![3.0, 2.0, 1.0]);
    let _ = std::fs::remove_file(&out);
}
