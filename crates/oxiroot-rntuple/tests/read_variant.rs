//! A `std::variant<int32_t, float>` field written by official ROOT: the Switch
//! column (index + alternative tag) and the Variant field role.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

#[test]
fn reads_variant_field() {
    let file = open("rntuple_variant_uncompressed.root");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5);

    let v = ntpl.read_field(&file, "v").expect("read v");

    // Raw columnar form: two densely-packed alternatives, per-entry tags+indices.
    assert_eq!(
        v,
        FieldValues::Variant {
            alternatives: vec![
                ("_0".to_string(), FieldValues::I32(vec![0, 20, 40])),
                ("_1".to_string(), FieldValues::F32(vec![1.5, 3.5])),
            ],
            tags: vec![1, 2, 1, 2, 1],
            indices: vec![0, 0, 1, 1, 2],
        }
    );

    // Resolving each entry (alternatives[tag-1] at index) gives the original
    // alternating int / float sequence: 0, 1.5, 20, 3.5, 40.
    let FieldValues::Variant {
        alternatives,
        tags,
        indices,
    } = v
    else {
        panic!("expected a variant");
    };
    let resolved: Vec<String> = tags
        .iter()
        .zip(&indices)
        .map(
            |(&tag, &idx)| match (&alternatives[(tag - 1) as usize].1, idx as usize) {
                (FieldValues::I32(xs), i) => xs[i].to_string(),
                (FieldValues::F32(xs), i) => xs[i].to_string(),
                _ => unreachable!(),
            },
        )
        .collect();
    assert_eq!(resolved, ["0", "1.5", "20", "3.5", "40"]);
}
