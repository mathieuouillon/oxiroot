//! The `rayon` feature adds `read_branch_par` and friends. They must return
//! exactly what the serial methods return, and enabling the feature must not
//! change the serial methods (they used to switch to parallel decoding
//! silently).
#![cfg(feature = "rayon")]

use std::path::PathBuf;

use oxiroot_io_core::{Compression, FileReader};
use oxiroot_tree::{write_tree_file_baskets, Branch, TreeReader};

fn fixture(name: &str) -> FileReader {
    FileReader::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open fixture")
}

fn assert_par_matches_serial(f: &FileReader, t: &TreeReader) {
    for name in t.branch_names() {
        let serial = t.read_branch(f, name);
        assert_eq!(t.read_branch_par(f, name), serial, "branch {name}");
        let n = t.num_entries();
        assert_eq!(
            t.read_branch_range_par(f, name, 1, n.saturating_sub(1)),
            t.read_branch_range(f, name, 1, n.saturating_sub(1)),
            "range of {name}"
        );
        assert_eq!(
            t.read_branch_flat_par(f, name),
            t.read_branch_flat(f, name),
            "flat {name}"
        );
    }
}

#[test]
fn parallel_reads_match_serial_on_a_root_written_tree() {
    let f = fixture("tree_multibasket.root");
    let t = TreeReader::open(&f, "Events").expect("open tree");
    assert_par_matches_serial(&f, &t);
}

#[test]
fn parallel_reads_match_serial_across_many_baskets() {
    let n: i32 = 1000;
    let x: Vec<f64> = (0..n).map(|i| f64::from(i) * 0.5).collect();
    let k: Vec<i32> = (0..n).map(|i| i % 17).collect();
    let jag: Vec<Vec<f64>> = (0..n)
        .map(|i| vec![f64::from(i); (i % 4) as usize])
        .collect();
    let out = std::env::temp_dir().join(format!("oxiroot_read_par_{}.root", std::process::id()));
    write_tree_file_baskets(
        &out,
        "T",
        &[
            Branch::f64("x", x),
            Branch::i32("k", k),
            Branch::jagged_f64("jag", jag),
        ],
        Compression::Zstd(1),
        37, // many baskets per branch
    )
    .expect("write");
    let f = FileReader::open(&out).expect("open");
    let _ = std::fs::remove_file(&out);
    let t = TreeReader::open(&f, "T").expect("open tree");
    assert_par_matches_serial(&f, &t);
}
