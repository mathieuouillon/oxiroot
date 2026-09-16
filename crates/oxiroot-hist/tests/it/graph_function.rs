//! A `TGraph` carrying a fitted function (`fFunctions`): read ROOT's `TF1`/
//! `TFormula`, round-trip it, and build one from scratch. Cross-checked against
//! compiled ROOT C++ and uproot, which both read the oxiroot-written file and
//! re-evaluate the formula (`Eval(2) == 5` for `[0]+[1]*x` with params `1, 2`).

use std::path::PathBuf;

use oxiroot_hist::{GraphFunction, ReadRoot, TGraph, WriteRoot};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// ROOT's `graph_function.root` holds a `TGraph` "gfit" with one `TF1` "line"
/// (`[0]+[1]*x`, params `1, 2`) attached. We parse the `TF1`/`TFormula` faithfully.
#[test]
fn reads_root_graph_function() {
    let f = RFile::open(fixture("graph_function.root")).expect("open");
    let g = TGraph::read_root(&f, "gfit").expect("read gfit");
    assert_eq!(g.title, "fitted");
    assert_eq!(g.x, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(g.y, vec![1.0, 3.0, 5.0, 7.0, 9.0]);

    assert_eq!(g.functions.len(), 1);
    let fun = &g.functions[0];
    assert_eq!(fun.name, "line");
    assert_eq!(fun.title, "[0]+[1]*x"); // ROOT's title is the [0]-form formula
    assert_eq!(fun.formula, "[p0]+[p1]*x"); // TFormula stores the [pN] form
    assert_eq!(fun.params, vec![1.0, 2.0]);
    assert_eq!(fun.npar(), 2);
    assert_eq!(fun.xmin, 0.0);
    assert_eq!(fun.xmax, 4.0);
    assert_eq!(fun.chi2, 0.0);
    assert_eq!(fun.ndf, 0);

    // The plain graphs in graphs.root carry no functions.
    let g0 = TGraph::read_root(&RFile::open(fixture("graphs.root")).unwrap(), "g").unwrap();
    assert!(g0.functions.is_empty());
}

/// ROOT's graph-with-function round-trips through oxiroot byte-faithfully enough
/// that re-reading yields the identical struct.
#[test]
fn graph_function_round_trips_from_root() {
    let f = RFile::open(fixture("graph_function.root")).expect("open");
    let g = TGraph::read_root(&f, "gfit").expect("read");
    let out = std::env::temp_dir().join("oxiroot_graph_function_rt.root");
    g.write_root(&out, Compression::None).expect("write");
    let back = TGraph::read_root(&RFile::open(&out).unwrap(), "gfit").unwrap();
    assert_eq!(back, g, "round-trip changed the graph/function");
    let _ = std::fs::remove_file(&out);
}

/// A function built from scratch with [`GraphFunction::new`] attaches, persists,
/// and round-trips. `new` normalizes the `[0]`-form formula to `[pN]` form.
#[test]
fn graph_function_built_from_scratch() {
    let g = TGraph::new(vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![1.0, 3.0, 5.0, 7.0, 9.0])
        .named("gfit")
        .titled("fitted")
        .with_function(GraphFunction::new(
            "line",
            "[0]+[1]*x",
            vec![1.0, 2.0],
            0.0,
            4.0,
        ));

    assert_eq!(g.functions.len(), 1);
    assert_eq!(g.functions[0].formula, "[p0]+[p1]*x");
    assert_eq!(g.functions[0].par_errors, vec![0.0, 0.0]); // defaulted, sized to params

    let out = std::env::temp_dir().join("oxiroot_graph_function_scratch.root");
    g.write_root(&out, Compression::Zstd(3)).expect("write");
    let back = TGraph::read_root(&RFile::open(&out).unwrap(), "gfit").unwrap();
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}

/// Several functions can be attached and all round-trip in order.
#[test]
fn multiple_functions_round_trip() {
    let g = TGraph::new(vec![0.0, 1.0], vec![0.0, 1.0])
        .named("g")
        .with_function(GraphFunction::new(
            "lin",
            "[0]+[1]*x",
            vec![0.0, 1.0],
            0.0,
            1.0,
        ))
        .with_function(GraphFunction::new("sq", "[0]*x*x", vec![2.0], -1.0, 1.0));

    let out = std::env::temp_dir().join("oxiroot_graph_multifn.root");
    g.write_root(&out, Compression::None).expect("write");
    let back = TGraph::read_root(&RFile::open(&out).unwrap(), "g").unwrap();
    assert_eq!(back.functions.len(), 2);
    assert_eq!(back.functions[0].name, "lin");
    assert_eq!(back.functions[1].name, "sq");
    assert_eq!(back.functions[1].formula, "[p0]*x*x");
    assert_eq!(back, g);
    let _ = std::fs::remove_file(&out);
}
