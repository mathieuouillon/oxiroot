//! An index-based friend join (`TTree::BuildIndex`).
//!
//! `fixtures/tree_index.root` is written by ROOT 6.40
//! (`scripts/gen_tree_index.cpp`): a `main` tree with `(run, event)` keys in
//! order, and a friend `fr` holding the same keys shuffled, with an index on
//! them. The friend's `weight` is `run * 1000 + event`, so a wrong pairing shows
//! itself. The pairing asserted here is what ROOT's own
//! `GetEntryNumberWithIndex` reports for this file.

use oxiroot_io_core::FileReader;
use oxiroot_tree::{BranchValues, TreeReader};

fn fixture() -> String {
    format!(
        "{}/../../fixtures/tree_index.root",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn an_index_reads_back_with_its_keys() {
    let file = FileReader::open(fixture()).unwrap();
    let friend = TreeReader::open(&file, "fr").unwrap();

    let index = friend.index().expect("the friend carries an index");
    assert_eq!(index.major_name(), "run");
    assert_eq!(index.minor_name(), "event");
    assert_eq!(index.len(), 6);

    // The keys come back sorted, each naming the entry that holds it.
    let pairs: Vec<((i64, i64), u64)> = index.iter().collect();
    assert_eq!(
        pairs,
        vec![
            ((1, 10), 2),
            ((1, 11), 5),
            ((1, 12), 4),
            ((2, 10), 1),
            ((2, 11), 3),
            ((2, 12), 0),
        ]
    );
    assert_eq!(index.entry_of(2, 11), Some(3));
    assert_eq!(index.entry_of(3, 10), None); // a key the friend does not hold

    // The main tree has no index of its own.
    assert!(TreeReader::open(&file, "main").unwrap().index().is_none());
}

#[test]
fn a_friend_joins_on_its_index_as_root_does() {
    let file = FileReader::open(fixture()).unwrap();
    let main = TreeReader::open(&file, "main").unwrap();
    let friend = TreeReader::open(&file, "fr").unwrap();

    let rows = main.join_by_index(&file, &friend, &file).expect("join");
    assert_eq!(
        rows,
        vec![Some(2), Some(5), Some(4), Some(1), Some(3), Some(0)]
    );

    // Permuting the friend's column by that mapping lines it up with the main
    // tree: entry i of `main` has key (run, event), and the weight that carries.
    let BranchValues::F64(weights) = friend.read_branch(&file, "weight").unwrap() else {
        panic!("weight is a double branch")
    };
    let aligned: Vec<Option<f64>> = rows
        .iter()
        .map(|entry| entry.and_then(|e| weights.get(e as usize).copied()))
        .collect();
    assert_eq!(
        aligned,
        vec![
            Some(1010.0),
            Some(1011.0),
            Some(1012.0),
            Some(2010.0),
            Some(2011.0),
            Some(2012.0)
        ]
    );
}

#[test]
fn joining_without_an_index_is_refused() {
    let file = FileReader::open(fixture()).unwrap();
    let main = TreeReader::open(&file, "main").unwrap();
    let friend = TreeReader::open(&file, "fr").unwrap();

    // `main` carries no index, so it cannot be joined on — rather than falling
    // back to pairing entry with entry, which would pair the wrong ones.
    let err = friend
        .join_by_index(&file, &main, &file)
        .expect_err("a friend with no index must be refused");
    assert!(err.to_string().contains("BuildIndex"), "{err}");
}
