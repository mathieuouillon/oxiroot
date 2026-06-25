//! Write a split (`fSplitLevel > 0`) `std::vector<MyStruct>` `TBranchElement`,
//! then read it back through our own reader. The /tmp file is also checked by
//! uproot / ROOT C++ when run by hand (see `scripts/`).

use std::path::PathBuf;

use oxiroot_io_core::{Compression, RFile};
use oxiroot_tree::{write_tree_file, Branch, BranchValues, SplitMember, TTree};

fn split_branch() -> Branch {
    // Mirrors `fixtures/tree_split.root`: Hit = {float x; float y; int id;}.
    let x = vec![vec![0.0_f32], vec![0.0, 1.0], vec![0.0, 1.0, 2.0]];
    let y = vec![vec![0.5_f32], vec![0.5, 1.5], vec![0.5, 1.5, 2.5]];
    let id = vec![vec![0_i32], vec![0, 10], vec![0, 10, 20]];
    Branch::split_vector(
        "hits",
        "Hit",
        vec![
            SplitMember::f32("x", x),
            SplitMember::f32("y", y),
            SplitMember::i32("id", id),
        ],
    )
}

fn assert_hits_roundtrip(f: &RFile) {
    let t = TTree::open(f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_names(), ["hits.x", "hits.y", "hits.id"]);
    assert_eq!(
        t.read_branch(f, "hits.x").unwrap(),
        BranchValues::VecF32(vec![vec![0.0], vec![0.0, 1.0], vec![0.0, 1.0, 2.0]])
    );
    assert_eq!(
        t.read_branch(f, "hits.id").unwrap(),
        BranchValues::VecI32(vec![vec![0], vec![0, 10], vec![0, 10, 20]])
    );
}

#[test]
fn writes_split_vector_zstd() {
    // Compression is orthogonal to the split layout, but verify the basket
    // payloads round-trip when Zstd-compressed. (/tmp file also checked by ROOT.)
    let out = PathBuf::from("/tmp/oxiroot_tree_split_zstd.root");
    write_tree_file(&out, "T", &[split_branch()], Compression::Zstd(5)).expect("write");
    assert_hits_roundtrip(&RFile::open(&out).expect("reopen"));
}

#[test]
fn writes_split_vector_of_struct() {
    let out = PathBuf::from("/tmp/oxiroot_tree_split.root");
    write_tree_file(&out, "T", &[split_branch()], Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);
    assert_eq!(t.branch_names(), ["hits.x", "hits.y", "hits.id"]);

    assert_eq!(
        t.read_branch(&f, "hits.x").unwrap(),
        BranchValues::VecF32(vec![vec![0.0], vec![0.0, 1.0], vec![0.0, 1.0, 2.0]])
    );
    assert_eq!(
        t.read_branch(&f, "hits.y").unwrap(),
        BranchValues::VecF32(vec![vec![0.5], vec![0.5, 1.5], vec![0.5, 1.5, 2.5]])
    );
    assert_eq!(
        t.read_branch(&f, "hits.id").unwrap(),
        BranchValues::VecI32(vec![vec![0], vec![0, 10], vec![0, 10, 20]])
    );
}

/// A different struct (double/float/Long64_t members, an empty entry, more
/// entries) to prove the writer is not specialised to the `Hit` fixture.
#[test]
fn writes_split_vector_general_struct() {
    let px = vec![
        vec![],
        vec![1.0_f64],
        vec![2.0, 3.0],
        vec![4.0],
        vec![5.0, 6.0, 7.0],
    ];
    let py = vec![
        vec![],
        vec![1.5_f64],
        vec![2.5, 3.5],
        vec![4.5],
        vec![5.5, 6.5, 7.5],
    ];
    let w = vec![
        vec![],
        vec![0.5_f32],
        vec![1.0, 1.5],
        vec![2.0],
        vec![2.5, 3.0, 3.5],
    ];
    let id = vec![
        vec![],
        vec![10_i64],
        vec![20, 30],
        vec![40],
        vec![50, 60, 70],
    ];
    let branch = Branch::split_vector(
        "parts",
        "Particle",
        vec![
            SplitMember::f64("px", px.clone()),
            SplitMember::f64("py", py.clone()),
            SplitMember::f32("w", w.clone()),
            SplitMember::i64("id", id.clone()),
        ],
    );
    let out = PathBuf::from("/tmp/oxiroot_tree_split_particle.root");
    write_tree_file(&out, "T", &[branch], Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 5);
    assert_eq!(
        t.read_branch(&f, "parts.px").unwrap(),
        BranchValues::VecF64(px)
    );
    assert_eq!(
        t.read_branch(&f, "parts.py").unwrap(),
        BranchValues::VecF64(py)
    );
    assert_eq!(
        t.read_branch(&f, "parts.w").unwrap(),
        BranchValues::VecF32(w)
    );
    assert_eq!(
        t.read_branch(&f, "parts.id").unwrap(),
        BranchValues::VecI64(id)
    );
}
