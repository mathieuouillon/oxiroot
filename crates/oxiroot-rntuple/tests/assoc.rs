//! Associative containers. `std::set` round-trips both ways: ROOT writes the
//! `rntuple_set_*` fixtures (read here) and reads oxiroot's own set output.
//! `std::map` is written by oxiroot and read back by uproot — ROOT 6.40's
//! collection proxy crashes on a `std::map` RNTuple (it can neither write nor
//! read one), so only uproot grounds the map.

use std::path::PathBuf;

use oxiroot_io_core::{Compression, RFile};
use oxiroot_rntuple::{Field, FieldValues, Ntuple, RNTuple, StructRole};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

/// A `std::set<int32_t>` ROOT wrote reads as a collection of its elements.
fn check_set_fixture(name: &str) {
    let file = open(name);
    let ntpl = RNTuple::open(&file, "ntpl").expect("open ntpl");
    assert_eq!(ntpl.num_entries(), 4, "{name}");
    // ROOT records the field as a Collection named with the set type.
    let s = &ntpl.header().fields[0];
    assert_eq!(s.struct_role, StructRole::Collection);
    assert_eq!(s.type_name, "std::set<std::int32_t>");
    assert_eq!(
        ntpl.read_field(&file, "s").unwrap(),
        FieldValues::VecI32(vec![
            vec![1, 2, 3],
            vec![4, 5],
            vec![],
            vec![10, 20, 30, 40]
        ]),
        "{name}"
    );
}

#[test]
fn reads_root_set_uncompressed() {
    check_set_fixture("rntuple_set_uncompressed.root");
}

#[test]
fn reads_root_set_zstd() {
    check_set_fixture("rntuple_set_zstd.root");
}

/// oxiroot writes a `std::set` that round-trips (and ROOT C++ reads it — checked
/// out of band). The on-disk schema is a collection tagged with the set type.
#[test]
fn writes_set_round_trip() {
    let nt = Ntuple::new(
        "ntpl",
        vec![Field::set_i32("s", vec![vec![1, 2, 3], vec![4, 5]])],
    );
    let out = std::env::temp_dir().join("oxiroot_set_rt.root");
    nt.write_root(&out, Compression::None).expect("write");
    let file = RFile::open(&out).unwrap();
    let ntpl = RNTuple::open(&file, "ntpl").unwrap();
    assert_eq!(ntpl.header().fields[0].type_name, "std::set<std::int32_t>");
    assert_eq!(
        ntpl.read_field(&file, "s").unwrap(),
        FieldValues::VecI32(vec![vec![1, 2, 3], vec![4, 5]])
    );
    let _ = std::fs::remove_file(&out);
}

/// oxiroot writes a `std::map<int32_t, double>` (a collection of key/value
/// records, tagged with the map type) that round-trips; uproot reads it as pairs.
#[test]
fn writes_map_round_trip() {
    let nt = Ntuple::new(
        "ntpl",
        vec![Field::map_i32_f64(
            "m",
            vec![vec![(1, 1.5), (2, 2.5)], vec![(3, 3.5)]],
        )],
    );
    let out = std::env::temp_dir().join("oxiroot_map_rt.root");
    nt.write_root(&out, Compression::None).expect("write");
    let file = RFile::open(&out).unwrap();
    let ntpl = RNTuple::open(&file, "ntpl").unwrap();

    let m = &ntpl.header().fields[0];
    assert_eq!(m.struct_role, StructRole::Collection);
    assert_eq!(m.type_name, "std::map<std::int32_t,double>");
    // The element is a std::pair record of key (`_0`) and value (`_1`).
    assert_eq!(
        ntpl.header().fields[1].type_name,
        "std::pair<std::int32_t,double>"
    );

    // Read back as a nested collection of key/value records.
    match ntpl.read_field(&file, "m").unwrap() {
        FieldValues::Nested { offsets, items } => {
            assert_eq!(offsets, vec![2, 3]);
            assert_eq!(
                *items,
                FieldValues::Record(vec![
                    ("_0".into(), FieldValues::I32(vec![1, 2, 3])),
                    ("_1".into(), FieldValues::F64(vec![1.5, 2.5, 3.5])),
                ])
            );
        }
        other => panic!("expected a nested map collection, got {other:?}"),
    }
    let _ = std::fs::remove_file(&out);
}
