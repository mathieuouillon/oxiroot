//! TGraph / TGraphErrors / TGraphAsymmErrors: read ROOT fixtures, self
//! round-trip, and build from scratch. (Cross-checked against compiled ROOT C++
//! and uproot, which read the oxiroot-written files with the right classes and
//! values.)

use std::path::PathBuf;

use oxiroot_hist::{GraphErrors, ReadRoot, TGraph, WriteRoot};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_root_written_graphs() {
    let f = RFile::open(fixture("graphs.root")).expect("open");

    let g = TGraph::read_root(&f, "g").expect("read g");
    assert_eq!(g.class_name(), "TGraph");
    assert_eq!(g.title, "plain");
    assert_eq!(g.x, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(g.y, vec![10.0, 20.0, 30.0, 40.0]);
    assert_eq!(g.errors, GraphErrors::None);

    let ge = TGraph::read_root(&f, "ge").expect("read ge");
    assert_eq!(ge.class_name(), "TGraphErrors");
    assert_eq!(
        ge.errors,
        GraphErrors::Symmetric {
            ex: vec![0.1, 0.2, 0.3, 0.4],
            ey: vec![1.0, 2.0, 3.0, 4.0],
        }
    );

    let gae = TGraph::read_root(&f, "gae").expect("read gae");
    assert_eq!(gae.class_name(), "TGraphAsymmErrors");
    assert_eq!(
        gae.errors,
        GraphErrors::Asymmetric {
            ex_low: vec![0.1, 0.1, 0.1, 0.1],
            ex_high: vec![0.2, 0.2, 0.2, 0.2],
            ey_low: vec![1.0, 1.0, 1.0, 1.0],
            ey_high: vec![2.0, 2.0, 2.0, 2.0],
        }
    );
}

#[test]
fn graphs_round_trip() {
    let f = RFile::open(fixture("graphs.root")).expect("open");
    for name in ["g", "ge", "gae"] {
        let g = TGraph::read_root(&f, name).unwrap();
        let out = std::env::temp_dir().join(format!("oxiroot_graph_{name}.root"));
        g.write_root(&out, Compression::None).expect("write");
        let back = TGraph::read_root(&RFile::open(&out).unwrap(), name).unwrap();
        assert_eq!(back, g, "{name} changed across round-trip");
    }
}

#[test]
fn graphs_build_from_scratch() {
    let plain = TGraph::new("p", "plain", vec![0.0, 1.0, 2.0], vec![1.0, 4.0, 9.0]);
    let sym = TGraph::with_errors(
        "s",
        "sym",
        vec![0.0, 1.0],
        vec![2.0, 3.0],
        vec![0.1, 0.1],
        vec![0.5, 0.5],
    );
    let asym = TGraph::with_asymm_errors(
        "a",
        "asym",
        vec![0.0, 1.0],
        vec![2.0, 3.0],
        vec![0.1, 0.2],
        vec![0.3, 0.4],
        vec![1.0, 1.0],
        vec![2.0, 2.0],
    );

    for g in [&plain, &sym, &asym] {
        let out = std::env::temp_dir().join(format!("oxiroot_graph_scratch_{}.root", g.name));
        g.write_root(&out, Compression::Zstd(3)).expect("write");
        let back = TGraph::read_root(&RFile::open(&out).unwrap(), &g.name).unwrap();
        assert_eq!(back, *g);
    }

    assert_eq!(plain.class_name(), "TGraph");
    assert_eq!(sym.class_name(), "TGraphErrors");
    assert_eq!(asym.class_name(), "TGraphAsymmErrors");
}

/// Zero-point graphs are a boundary worth pinning: the writer must still emit a
/// valid (empty) `fNpoints`/`fX`/`fY`, and the reader must round-trip them.
#[test]
fn empty_graphs_round_trip() {
    let plain = TGraph::new("e0", "empty", vec![], vec![]);
    let sym = TGraph::with_errors("e1", "empty", vec![], vec![], vec![], vec![]);
    for g in [&plain, &sym] {
        assert_eq!(g.len(), 0);
        assert!(g.is_empty());
        let out = std::env::temp_dir().join(format!("oxiroot_graph_empty_{}.root", g.name));
        g.write_root(&out, Compression::None).expect("write");
        let back = TGraph::read_root(&RFile::open(&out).unwrap(), &g.name).unwrap();
        assert_eq!(back, *g);
    }
}
