//! Write an RNTuple with *nested* collection fields and read it back. Run:
//!
//! ```sh
//! cargo run -p oxiroot --example rntuple_nested
//! ```
//!
//! Shows the three nesting shapes — a vector of strings, a vector of vectors,
//! and a vector of records (a `std::pair`) — written so that official ROOT and
//! uproot read them too. Pass an output path as the first argument to keep the
//! file (default: a temporary file that is read back and removed).

use oxiroot::prelude::*;

fn main() -> Result<()> {
    // (a) std::vector<std::string>: per-entry lists of tags.
    let tags = Field::vec_str(
        "tags",
        vec![
            vec![],
            vec!["mu".to_string(), "iso".to_string()],
            vec!["jet".to_string()],
        ],
    );

    // (b) std::vector<std::vector<int32_t>>: per-entry hit patterns.
    let hits = Field::vec_vec_i32(
        "hits",
        vec![vec![], vec![vec![1, 2]], vec![vec![3], vec![4, 5]]],
    );

    // (c) std::vector<std::pair<int32_t,double>>: a vector of records, built from
    //     per-entry offsets over the flattened (id, energy) struct-of-arrays.
    let clusters = Field::new(
        "clusters",
        Column::Nested {
            offsets: vec![0, 1, 3], // entry 0: none, entry 1: 1, entry 2: 2
            items: Box::new(Column::Record(vec![
                ("_0".to_string(), Column::I32(vec![10, 20, 21])),
                ("_1".to_string(), Column::F64(vec![1.5, 2.5, 3.5])),
            ])),
        },
    );

    let fields = vec![tags, hits, clusters];

    let keep = std::env::args().nth(1);
    let path = keep.clone().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("oxiroot_nested.root")
            .display()
            .to_string()
    });

    write_rntuple_file(&path, "events", &fields, Compression::Zstd(5))?;
    println!("wrote {path}");

    // Read it back through oxiroot.
    let file = RFile::open(&path)?;
    let ntpl = RNTuple::open(&file, "events")?;
    println!(
        "{} entries, fields: {:?}",
        ntpl.num_entries(),
        ntpl.field_names()
    );
    for name in ["tags", "hits", "clusters"] {
        println!("  {name} = {:?}", ntpl.read_field(&file, name)?);
    }

    if keep.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}
