//! Write a `TTree` and read it back. Run:
//!
//! ```sh
//! cargo run -p oxiroot --example tree
//! ```
//!
//! Writes a scalar, a variable-length (jagged) array, and a string branch, then
//! reads them back — showing branch introspection (`branch_type`/`branch_shape`),
//! a whole-branch read, and an entry-range read that touches only the baskets it
//! needs. Pass an output path as the first argument to keep the file.

use oxiroot::prelude::*;

fn main() -> Result<()> {
    let keep = std::env::args().nth(1);
    let path = keep.clone().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("oxiroot_tree.root")
            .display()
            .to_string()
    });

    // A scalar, a jagged array (rows of differing length), and a string branch.
    let branches = vec![
        Branch::i32("event", (0..10).collect()),
        Branch::jagged_f64(
            "hits",
            (0..10).map(|i| vec![i as f64; (i as usize) % 3]).collect(),
        ),
        Branch::strings("label", (0..10).map(|i| format!("e{i}")).collect()),
    ];
    write_tree_file(&path, "Events", &branches, Compression::Zstd(5))?;
    println!("wrote {path}");

    let file = RFile::open(&path)?;
    let t = TTree::open(&file, "Events")?;
    println!(
        "{} entries, branches: {:?}",
        t.num_entries(),
        t.branch_names()
    );

    // Introspect each branch's type and shape without reading its data.
    for b in t.branch_names() {
        println!(
            "  {b}: type={:?} shape={:?}",
            t.branch_type(b).unwrap(),
            t.branch_shape(b).unwrap()
        );
    }

    // Read a whole branch (typed accessor), then a window of it — the ranged read
    // fetches only the baskets covering [3, 7).
    let event = t.read_branch(&file, "event")?;
    println!("event       = {:?}", event.as_i32().unwrap());
    let window = t.read_branch_range(&file, "event", 3, 7)?;
    println!("event[3..7] = {:?}", window.as_i32().unwrap());
    println!("hits        = {:?}", t.read_branch(&file, "hits")?);

    if keep.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}
