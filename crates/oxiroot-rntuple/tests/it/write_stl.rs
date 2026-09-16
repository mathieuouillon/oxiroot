//! Writing fixed-size STL fields (`std::array`, `std::bitset`) and a split
//! user-class record, then reading them back. Cross-checked against compiled
//! ROOT C++ (`RNTupleReader` typed views over `std::array`/`std::bitset`/the
//! `Hit` class) and uproot, which both read the oxiroot-written file — ROOT's
//! strict reader accepts the computed `Hit` class checksum.

use oxiroot_io_core::{Compression, RFile};
use oxiroot_rntuple::{Column, Field, FieldValues, Ntuple, RNTuple};

fn write_and_reopen(tag: &str, fields: Vec<Field>) -> (RFile, RNTuple) {
    let nt = Ntuple::new("ntpl", fields);
    let out = std::env::temp_dir().join(format!("oxiroot_write_stl_{tag}.root"));
    nt.write_root(&out, Compression::Zstd(3)).expect("write");
    let file = RFile::open(&out).expect("reopen");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open ntpl");
    (file, ntpl)
}

#[test]
fn array_field_round_trips() {
    let (file, ntpl) = write_and_reopen(
        "array",
        vec![Field::array_i32(
            "arr",
            vec![vec![10, 20, 30], vec![40, 50, 60]],
        )],
    );
    assert_eq!(ntpl.num_entries(), 2);
    assert_eq!(
        ntpl.read_field(&file, "arr").unwrap(),
        FieldValues::VecI32(vec![vec![10, 20, 30], vec![40, 50, 60]])
    );
    // The field is a fixed array of 3 (flags + array_size in the header).
    let arr = &ntpl.header().fields[0];
    assert_eq!(arr.array_size, Some(3));
    assert_eq!(arr.type_name, "std::array<std::int32_t,3>");
}

#[test]
fn bitset_field_round_trips() {
    let row0 = vec![true, false, true, false, false, false, false, false];
    let row1 = vec![false, true, false, true, false, false, false, false];
    let (file, ntpl) = write_and_reopen(
        "bitset",
        vec![Field::bitset("bits", vec![row0.clone(), row1.clone()])],
    );
    assert_eq!(
        ntpl.read_field(&file, "bits").unwrap(),
        FieldValues::VecBool(vec![row0, row1])
    );
    assert_eq!(ntpl.header().fields[0].array_size, Some(8));
    assert_eq!(ntpl.header().fields[0].type_name, "std::bitset<8>");
}

#[test]
fn user_class_field_round_trips() {
    let (file, ntpl) = write_and_reopen(
        "user",
        vec![Field::object(
            "hit",
            "Hit",
            vec![
                ("id".into(), Column::I32(vec![7, 42])),
                ("energy".into(), Column::F64(vec![3.25, -1.5])),
            ],
        )],
    );
    assert_eq!(
        ntpl.read_field(&file, "hit").unwrap(),
        FieldValues::Record(vec![
            ("id".into(), FieldValues::I32(vec![7, 42])),
            ("energy".into(), FieldValues::F64(vec![3.25, -1.5])),
        ])
    );
    // The record carries the ROOT class checksum so ROOT validates the schema.
    assert_eq!(ntpl.header().fields[0].type_name, "Hit");
}

#[test]
fn all_three_in_one_ntuple() {
    let (file, ntpl) = write_and_reopen(
        "all",
        vec![
            Field::array_f64("xyz", vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]),
            Field::bitset("flags", vec![vec![true, true], vec![false, true]]),
            Field::object(
                "hit",
                "Hit",
                vec![
                    ("id".into(), Column::I32(vec![1, 2])),
                    ("energy".into(), Column::F64(vec![0.5, 1.5])),
                ],
            ),
        ],
    );
    assert_eq!(ntpl.num_entries(), 2);
    assert_eq!(
        ntpl.read_field(&file, "xyz").unwrap(),
        FieldValues::VecF64(vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]])
    );
    assert_eq!(ntpl.field_names(), vec!["xyz", "flags", "hit"]);
}
