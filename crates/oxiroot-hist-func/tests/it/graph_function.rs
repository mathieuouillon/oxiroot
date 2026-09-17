//! A `TF1` and a graph's `GraphFunction` are the same ROOT record: converting one
//! into the other keeps the bytes, and a function attached to a graph survives a
//! file round trip as something `TF1` can evaluate.

use oxiroot_hist::{Compression, GraphFunction, ReadRoot, TGraph, WriteRoot};
use oxiroot_hist_func::TF1;
use oxiroot_io_core::buffer::WBuffer;
use oxiroot_io_core::RFile;

fn fitted() -> TF1 {
    TF1::new("fit", "[0]*exp(-0.5*((x-[1])/[2])^2) + [3]", -2.0, 6.0)
        .unwrap()
        .with_params(vec![4.0, 1.5, 0.75, 0.25])
}

#[test]
fn graph_function_record_has_the_tf1_key_bytes() {
    let f = fitted();
    let mut w = WBuffer::new();
    f.to_graph_function().write_tf1_body(&mut w, 1, 100);
    assert_eq!(w.into_vec(), f.to_root_bytes());
}

#[test]
fn conversion_round_trips_in_memory() {
    let f = fitted();
    let record = f.to_graph_function();
    assert_eq!(record.formula, f.formula());
    assert_eq!(record.params, f.params());
    assert_eq!((record.xmin, record.xmax), f.range());
    assert_eq!(TF1::from_graph_function(record).unwrap(), f);
}

#[test]
fn attached_function_evaluates_after_a_file_round_trip() {
    let f = fitted();
    let g = TGraph::new(vec![0.0, 1.0, 2.0, 3.0], vec![1.3, 4.1, 3.2, 1.0])
        .named("g")
        .with_function(f.to_graph_function());
    let path = std::env::temp_dir().join("oxiroot_hist_func_graph_function.root");
    g.write_root(&path, Compression::None).unwrap();

    let back = TGraph::read_root(&RFile::open(&path).unwrap(), "g").unwrap();
    assert_eq!(back.functions.len(), 1);
    let h = TF1::from_graph_function(back.functions[0].clone()).unwrap();
    for x in [-1.0, 0.0, 1.5, 2.25, 5.0] {
        assert_eq!(h.eval(x), f.eval(x), "x = {x}");
    }
    assert_eq!(h.name(), "fit");
    assert_eq!(h.range(), (-2.0, 6.0));
}

#[test]
fn a_bad_formula_is_an_error() {
    let bad = GraphFunction::new("bad", "[0]*(x", vec![1.0], 0.0, 1.0);
    assert!(TF1::from_graph_function(bad).is_err());
}
