//! Write a tree with several baskets per branch (B8), then read it back.

use oxiroot_io_core::{Compression, RFile};
use oxiroot_tree::{write_tree_file_baskets, Branch, BranchValues, TTree};

#[test]
fn multi_basket_round_trips() {
    let x: Vec<i32> = (0..7).collect();
    let y: Vec<Vec<f64>> = vec![
        vec![1.0],
        vec![2.0, 2.0],
        vec![],
        vec![3.0],
        vec![4.0, 4.0],
        vec![5.0],
        vec![6.0, 6.0, 6.0],
    ];
    let branches = vec![
        Branch::i32("x", x.clone()),
        Branch::jagged_f64("y", y.clone()),
    ];

    let out = std::env::temp_dir().join("oxiroot_multibasket.root");
    // 7 entries, 3 per basket -> 3 baskets per branch ([0,3), [3,6), [6,7)).
    write_tree_file_baskets(&out, "T", &branches, Compression::None, 3).expect("write");

    let f = RFile::open(&out).expect("reopen");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 7);

    // Full read concatenates the three baskets back into the original column.
    assert_eq!(t.read_branch(&f, "x").expect("x"), BranchValues::I32(x));
    assert_eq!(t.read_branch(&f, "y").expect("y"), BranchValues::VecF64(y));

    // A ranged read across a basket boundary still works.
    assert_eq!(
        t.read_branch_range(&f, "x", 2, 5).expect("range"),
        BranchValues::I32(vec![2, 3, 4])
    );
}
