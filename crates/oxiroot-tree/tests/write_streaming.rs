//! Streaming, bounded-memory writes via `TTreeWriter` (B13): append entries in
//! batches (one basket per branch each), then read the file back.

use oxiroot_io_core::{Compression, RFile};
use oxiroot_tree::{Branch, BranchValues, TTree, TTreeWriter};

/// A unique temp path per test (tests run in parallel in one process).
fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("oxiroot_stream_{name}.root"))
}

#[test]
fn streaming_scalars_round_trip() {
    let out = tmp("scalars");
    let mut w = TTreeWriter::create(&out, "T", Compression::None).expect("create");
    // Three batches of differing sizes -> three baskets per branch.
    let batches: [(Vec<i32>, Vec<f64>); 3] = [
        (vec![0, 1, 2], vec![0.0, 1.0, 2.0]),
        (vec![3, 4], vec![3.0, 4.0]),
        (vec![5, 6, 7, 8], vec![5.0, 6.0, 7.0, 8.0]),
    ];
    for (x, y) in &batches {
        w.write_batch(&[Branch::i32("x", x.clone()), Branch::f64("y", y.clone())])
            .expect("batch");
    }
    assert_eq!(w.num_entries(), 9);
    w.finish().expect("finish");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "T").expect("open");
    assert_eq!(t.num_entries(), 9);
    assert_eq!(
        t.read_branch(&f, "x").expect("x"),
        BranchValues::I32((0..9).collect())
    );
    assert_eq!(
        t.read_branch(&f, "y").expect("y"),
        BranchValues::F64((0..9).map(|i| i as f64).collect())
    );
    // A range straddling basket boundaries (batch 0 ends at 3, batch 1 at 5).
    assert_eq!(
        t.read_branch_range(&f, "x", 2, 6).expect("range"),
        BranchValues::I32(vec![2, 3, 4, 5])
    );
}

#[test]
fn streaming_many_baskets_grow_fmaxbaskets() {
    // More than ROOT's default fMaxBaskets (10): the writer must grow the basket
    // arrays so every basket is addressable.
    let out = tmp("many");
    let mut w = TTreeWriter::create(&out, "T", Compression::None).expect("create");
    let n_batches = 25;
    for b in 0..n_batches {
        let x: Vec<i64> = (0..4).map(|i| (b * 4 + i) as i64).collect();
        w.write_batch(&[Branch::i64("x", x)]).expect("batch");
    }
    w.finish().expect("finish");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "T").expect("open");
    let total = (n_batches * 4) as u64;
    assert_eq!(t.num_entries(), total);
    assert_eq!(
        t.read_branch(&f, "x").expect("x"),
        BranchValues::I64((0..total as i64).collect())
    );
    // A range deep in the file (basket 12-ish) reads correctly.
    assert_eq!(
        t.read_branch_range(&f, "x", 50, 54).expect("range"),
        BranchValues::I64(vec![50, 51, 52, 53])
    );
}

#[test]
fn streaming_jagged_vector_string_round_trip() {
    let out = tmp("mixed");
    let mut w = TTreeWriter::create(&out, "T", Compression::Zlib(6)).expect("create");

    let jag: Vec<Vec<f64>> = vec![vec![1.0], vec![], vec![2.0, 3.0, 4.0], vec![5.0, 6.0]];
    let vec_branch: Vec<Vec<i32>> = vec![vec![10], vec![20, 21], vec![], vec![30, 31, 32]];
    let strs: Vec<String> = ["a", "bb", "ccc", "dddd"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Two batches: [0,2) then [2,4).
    for r in [0usize..2, 2..4] {
        w.write_batch(&[
            Branch::jagged_f64("j", jag[r.clone()].to_vec()),
            Branch::vector_i32("v", vec_branch[r.clone()].to_vec()),
            Branch::strings("s", strs[r.clone()].to_vec()),
        ])
        .expect("batch");
    }
    w.finish().expect("finish");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "T").expect("open");
    assert_eq!(t.num_entries(), 4);
    assert_eq!(
        t.read_branch(&f, "j").expect("j"),
        BranchValues::VecF64(jag)
    );
    assert_eq!(
        t.read_branch(&f, "v").expect("v"),
        BranchValues::VecI32(vec_branch)
    );
    assert_eq!(t.read_branch(&f, "s").expect("s"), BranchValues::Str(strs));
    // The auto-generated count branch is present and correct.
    assert_eq!(
        t.read_branch(&f, "nj").expect("nj"),
        BranchValues::I32(vec![1, 0, 3, 2])
    );
}

#[test]
fn streaming_schema_mismatch_is_rejected() {
    let out = tmp("mismatch");
    let mut w = TTreeWriter::create(&out, "T", Compression::None).expect("create");
    w.write_batch(&[Branch::i32("x", vec![1, 2])])
        .expect("first");
    // A second batch with a different element type for the same branch.
    let err = w
        .write_batch(&[Branch::f64("x", vec![3.0])])
        .expect_err("schema change must error");
    assert!(format!("{err}").contains("schema"), "got: {err}");
}

#[test]
fn streaming_uneven_batch_entries_is_rejected() {
    let out = tmp("uneven");
    let mut w = TTreeWriter::create(&out, "T", Compression::None).expect("create");
    let err = w
        .write_batch(&[Branch::i32("x", vec![1, 2, 3]), Branch::i32("y", vec![1])])
        .expect_err("uneven entry counts must error");
    assert!(format!("{err}").contains("entries"), "got: {err}");
}

#[test]
fn streaming_no_batches_is_rejected() {
    let out = tmp("empty");
    let w = TTreeWriter::create(&out, "T", Compression::None).expect("create");
    assert!(w.finish().is_err(), "finishing with no batch must error");
}
