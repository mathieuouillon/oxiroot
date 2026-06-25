//! Write a flat tree, read it back through our own reader.
use oxiroot_io_core::{Compression, RFile};
use oxiroot_tree::{write_tree_file, Branch, BranchValues, TTree};

#[test]
fn write_then_read_roundtrips() {
    let out = std::path::PathBuf::from("/tmp/oxiroot_written_tree.root");
    let branches = vec![
        Branch::i32("i4", vec![0, 1, 2, 3, 4]),
        Branch::f64("f8", vec![0.25, 1.25, 2.25, 3.25, 4.25]),
        Branch::bools("b1", vec![true, false, true, false, true]),
        Branch::u32("u4", vec![100, 200, 300, 400, 4_000_000_000]),
    ];
    write_tree_file(&out, "Events", &branches, Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 5);
    assert_eq!(t.branch_names(), ["i4", "f8", "b1", "u4"]);
    assert_eq!(
        t.read_branch(&f, "i4").unwrap(),
        BranchValues::I32(vec![0, 1, 2, 3, 4])
    );
    assert_eq!(
        t.read_branch(&f, "f8").unwrap(),
        BranchValues::F64(vec![0.25, 1.25, 2.25, 3.25, 4.25])
    );
    assert_eq!(
        t.read_branch(&f, "b1").unwrap(),
        BranchValues::Bool(vec![true, false, true, false, true])
    );
    assert_eq!(
        t.read_branch(&f, "u4").unwrap(),
        BranchValues::U32(vec![100, 200, 300, 400, 4_000_000_000])
    );
}

#[test]
fn write_then_read_arrays_and_strings() {
    let out = std::path::PathBuf::from("/tmp/oxiroot_written_arrays.root");
    let xs = vec![
        vec![1.0, 2.0, 3.0],
        vec![4.0, 5.0, 6.0],
        vec![7.0, 8.0, 9.0],
    ];
    let ss = vec!["alpha".to_string(), String::new(), "gamma!".to_string()];
    let ns = vec![
        vec![10i32, 11, 12, 13],
        vec![20, 21, 22, 23],
        vec![30, 31, 32, 33],
    ];
    let branches = vec![
        Branch::vec_f64("x", xs.clone()),
        Branch::strings("s", ss.clone()),
        Branch::vec_i32("n", ns.clone()),
    ];
    write_tree_file(&out, "Events", &branches, Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "Events").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_names(), ["x", "s", "n"]);
    assert_eq!(t.read_branch(&f, "x").unwrap(), BranchValues::VecF64(xs));
    assert_eq!(t.read_branch(&f, "s").unwrap(), BranchValues::Str(ss));
    assert_eq!(t.read_branch(&f, "n").unwrap(), BranchValues::VecI32(ns));
}

#[test]
fn jagged_arrays_are_rejected() {
    use oxiroot_tree::tree_file_bytes;
    let branches = vec![Branch::vec_i32("j", vec![vec![1, 2], vec![3]])];
    let err = tree_file_bytes("f.root", "T", &branches, Compression::None).unwrap_err();
    assert!(format!("{err}").contains("jagged"), "got: {err}");
}
