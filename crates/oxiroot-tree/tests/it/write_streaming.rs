//! Streaming, bounded-memory writes via `TreeWriter` (B13): append entries in
//! batches (one basket per branch each), then read the file back.

use oxiroot_io_core::{Compression, FileReader};
use oxiroot_tree::{Branch, BranchValues, TreeReader, TreeWriter};

/// A unique temp path per test (tests run in parallel in one process).
fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("oxiroot_stream_{name}.root"))
}

#[test]
fn streaming_scalars_round_trip() {
    let out = tmp("scalars");
    let mut w = TreeWriter::create(&out, "T", Compression::None).expect("create");
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

    let f = FileReader::open(&out).expect("reopen");
    let t = TreeReader::open(&f, "T").expect("open");
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
    let mut w = TreeWriter::create(&out, "T", Compression::None).expect("create");
    let n_batches = 25;
    for b in 0..n_batches {
        let x: Vec<i64> = (0..4).map(|i| (b * 4 + i) as i64).collect();
        w.write_batch(&[Branch::i64("x", x)]).expect("batch");
    }
    w.finish().expect("finish");

    let f = FileReader::open(&out).expect("reopen");
    let t = TreeReader::open(&f, "T").expect("open");
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
    let mut w = TreeWriter::create(&out, "T", Compression::Zlib(6)).expect("create");

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

    let f = FileReader::open(&out).expect("reopen");
    let t = TreeReader::open(&f, "T").expect("open");
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
fn streaming_create_large_writes_big_container_round_trip() {
    // `create_large` writes the 64-bit ("big") container form. Even with tiny
    // content the file must be a valid big-format TFile that reads back with all
    // entries in order — exercising the same wide header/dir/key path a genuine
    // >2 GiB file uses, without producing 2 GiB.
    let out = tmp("large_scalars");
    let mut w = TreeWriter::create_large(&out, "T", Compression::None).expect("create_large");
    let batches: [(Vec<i32>, Vec<f64>); 3] = [
        (vec![0, 1, 2], vec![0.0, 1.0, 2.0]),
        (vec![3, 4], vec![3.0, 4.0]),
        (vec![5, 6, 7, 8], vec![5.0, 6.0, 7.0, 8.0]),
    ];
    for (x, y) in &batches {
        w.write_batch(&[Branch::i32("x", x.clone()), Branch::f64("y", y.clone())])
            .expect("batch");
    }
    w.finish().expect("finish");

    let f = FileReader::open(&out).expect("reopen");
    assert!(
        f.header().is_big(),
        "create_large must write the 64-bit container"
    );
    assert_eq!(f.header().units, 8, "big-format fUnits is 8");
    let t = TreeReader::open(&f, "T").expect("open");
    assert_eq!(t.num_entries(), 9);
    assert_eq!(
        t.read_branch(&f, "x").expect("x"),
        BranchValues::I32((0..9).collect())
    );
    assert_eq!(
        t.read_branch(&f, "y").expect("y"),
        BranchValues::F64((0..9).map(|i| i as f64).collect())
    );
}

#[test]
fn streaming_jagged_big_container_round_trips() {
    // A jagged + vector + string tree in the big container form: the wider keys
    // must not disturb the count/offset machinery.
    let out = tmp("large_mixed");
    let mut w = TreeWriter::create_large(&out, "T", Compression::Zlib(6)).expect("create_large");
    let jag: Vec<Vec<f64>> = vec![vec![1.0], vec![], vec![2.0, 3.0, 4.0], vec![5.0, 6.0]];
    for r in [0usize..2, 2..4] {
        w.write_batch(&[Branch::jagged_f64("j", jag[r.clone()].to_vec())])
            .expect("batch");
    }
    w.finish().expect("finish");

    let f = FileReader::open(&out).expect("reopen");
    assert!(f.header().is_big());
    let t = TreeReader::open(&f, "T").expect("open");
    assert_eq!(
        t.read_branch(&f, "j").expect("j"),
        BranchValues::VecF64(jag)
    );
}

#[test]
fn streaming_schema_mismatch_is_rejected() {
    let out = tmp("mismatch");
    let mut w = TreeWriter::create(&out, "T", Compression::None).expect("create");
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
    let mut w = TreeWriter::create(&out, "T", Compression::None).expect("create");
    let err = w
        .write_batch(&[Branch::i32("x", vec![1, 2, 3]), Branch::i32("y", vec![1])])
        .expect_err("uneven entry counts must error");
    assert_eq!(
        err,
        oxiroot_io_core::Error::LengthMismatch {
            what: "branch \"y\" entries".into(),
            expected: 3,
            found: 1
        }
    );
}

#[test]
fn streaming_no_batches_is_rejected() {
    let out = tmp("empty");
    let w = TreeWriter::create(&out, "T", Compression::None).expect("create");
    assert!(w.finish().is_err(), "finishing with no batch must error");
}

/// A genuine >2 GiB file end to end: entries whose baskets, tree object, and key
/// all land past the 2 GiB mark, so the 64-bit seek pointers are exercised for
/// real (not just via a lowered threshold). Ignored by default — it writes
/// ~2.4 GiB to a temp file and takes a while; run with
/// `cargo test -p oxiroot-tree --test write_streaming -- --ignored`.
#[test]
#[ignore = "writes a real >2 GiB file"]
fn streaming_real_over_two_gib() {
    let out = tmp("real_big");
    let mut w = TreeWriter::create_large(&out, "T", Compression::None).expect("create_large");
    // 300 batches × 1_000_000 f64 (8 MB each) ≈ 2.4 GiB of basket payload, well
    // past kStartBigFile. The last batch's baskets sit above 2 GiB.
    let per_batch = 1_000_000i64;
    let n_batches = 300i64;
    for b in 0..n_batches {
        let base = b * per_batch;
        let x: Vec<f64> = (0..per_batch).map(|i| (base + i) as f64).collect();
        w.write_batch(&[Branch::f64("x", x)]).expect("batch");
    }
    w.finish().expect("finish");

    let total = (per_batch * n_batches) as u64;
    let meta = std::fs::metadata(&out).expect("stat");
    assert!(
        meta.len() > 2_000_000_000,
        "file should exceed 2 GiB, got {} bytes",
        meta.len()
    );

    let f = FileReader::open(&out).expect("reopen");
    assert!(f.header().is_big(), "a >2 GiB file must be the big form");
    let t = TreeReader::open(&f, "T").expect("open");
    assert_eq!(t.num_entries(), total);
    // Read the final four entries — their basket sits past the 2 GiB mark, so
    // this proves the 64-bit fBasketSeek round-trips.
    let last = total - 4;
    assert_eq!(
        t.read_branch_range(&f, "x", last, total).expect("tail"),
        BranchValues::F64((last..total).map(|i| i as f64).collect())
    );
    let _ = std::fs::remove_file(&out);
}
