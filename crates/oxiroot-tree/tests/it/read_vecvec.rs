//! Unsplit `std::vector<std::vector<T>>` branches: a doubly-nested collection
//! per entry. oxiroot reads them as [`BranchValues::Nested`] — a flat list of
//! inner vectors partitioned per entry by cumulative `offsets`. Cross-checked
//! against ROOT C++ and uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open fixture")
}

#[test]
fn reads_vector_of_vector_int() {
    let f = fixture("tree_vecvec.root");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_names(), vec!["vi", "vd"]);
    assert!(t.unsupported_branches().is_empty());

    // vi = [[[0]], [[100], [110, 111]], [[200], [210, 211], [220, 221, 222]]]
    let vi = t.read_branch(&f, "vi").unwrap();
    assert_eq!(vi.len(), 3);
    assert_eq!(
        vi,
        BranchValues::Nested {
            offsets: vec![0, 1, 3, 6],
            items: Box::new(BranchValues::VecI32(vec![
                vec![0],
                vec![100],
                vec![110, 111],
                vec![200],
                vec![210, 211],
                vec![220, 221, 222],
            ])),
        }
    );
}

#[test]
fn reads_vector_of_vector_double() {
    let f = fixture("tree_vecvec.root");
    let t = TTree::open(&f, "T").expect("open tree");
    // vd = [[[0.0]], [[1.0], [1.1, 1.11]], [[2.0], [2.1, 2.11], [2.2, 2.21, 2.22]]]
    assert_eq!(
        t.read_branch(&f, "vd").unwrap(),
        BranchValues::Nested {
            offsets: vec![0, 1, 3, 6],
            items: Box::new(BranchValues::VecF64(vec![
                vec![0.0],
                vec![1.0],
                vec![1.1, 1.11],
                vec![2.0],
                vec![2.1, 2.11],
                vec![2.2, 2.21, 2.22],
            ])),
        }
    );
}

/// The `Nested` shape's `offsets` partition `items` into per-entry lists of
/// inner vectors — this reconstructs entry 1's `[[100], [110, 111]]`.
#[test]
fn nested_offsets_partition_entries() {
    let f = fixture("tree_vecvec.root");
    let t = TTree::open(&f, "T").expect("open tree");
    let BranchValues::Nested { offsets, items } = t.read_branch(&f, "vi").unwrap() else {
        panic!("expected a nested branch");
    };
    let BranchValues::VecI32(inner) = *items else {
        panic!("expected i32 inner vectors");
    };
    // Entry 1 spans items[offsets[1]..offsets[2]].
    let entry1 = &inner[offsets[1] as usize..offsets[2] as usize];
    assert_eq!(entry1, &[vec![100], vec![110, 111]]);
}
