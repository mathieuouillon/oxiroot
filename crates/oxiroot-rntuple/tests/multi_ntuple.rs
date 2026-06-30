//! Several RNTuples in one file, and an RNTuple inside a `TDirectory`
//! (`NtupleFile` / `NtupleDir`). Round-tripped through oxiroot here; the same
//! files are read by ROOT C++ (`RNTupleReader`) and uproot.

use oxiroot_io_core::{Compression, RFile};
use oxiroot_rntuple::{Field, FieldValues, Ntuple, NtupleFile, RNTuple};

fn write(path: &str, compression: Compression) {
    NtupleFile::new()
        .add(Ntuple::new(
            "events",
            vec![
                Field::i32("x", vec![1, 2, 3]),
                Field::f64("y", vec![0.5, 1.5, 2.5]),
            ],
        ))
        .add(Ntuple::new("runs", vec![Field::i32("run", vec![7, 8])]))
        .dir("cal", |d| {
            d.add(Ntuple::new(
                "pedestals",
                vec![Field::f64("p", vec![9.5, 8.5])],
            ))
        })
        .write_root(path, compression)
        .unwrap();
}

#[test]
fn several_rntuples_in_top_directory() {
    let path = std::env::temp_dir().join("oxiroot_multi_top.root");
    let path = path.to_str().unwrap();
    write(path, Compression::None);
    let f = RFile::open(path).unwrap();

    // Both top-directory RNTuples open and read.
    let events = RNTuple::open(&f, "events").unwrap();
    assert_eq!(events.num_entries(), 3);
    assert_eq!(events.field_names(), vec!["x", "y"]);
    assert_eq!(
        events.read_field(&f, "x").unwrap(),
        FieldValues::I32(vec![1, 2, 3])
    );
    let runs = RNTuple::open(&f, "runs").unwrap();
    assert_eq!(runs.num_entries(), 2);
    assert_eq!(
        runs.read_field(&f, "run").unwrap(),
        FieldValues::I32(vec![7, 8])
    );
}

#[test]
fn rntuple_inside_a_subdirectory() {
    let path = std::env::temp_dir().join("oxiroot_multi_sub.root");
    let path = path.to_str().unwrap();
    write(path, Compression::None);
    let f = RFile::open(path).unwrap();

    // The subdirectory is navigable and its RNTuple opens via `open_in`.
    let cal = f.subdir("cal").unwrap();
    assert!(cal.keys.iter().any(|k| k.name == "pedestals"));
    let ped = RNTuple::open_in(&f, "cal", "pedestals").unwrap();
    assert_eq!(ped.num_entries(), 2);
    assert_eq!(
        ped.read_field(&f, "p").unwrap(),
        FieldValues::F64(vec![9.5, 8.5])
    );
}

#[test]
fn compressed_multi_file_round_trips() {
    let path = std::env::temp_dir().join("oxiroot_multi_zstd.root");
    let path = path.to_str().unwrap();
    write(path, Compression::Zstd(5));
    let f = RFile::open(path).unwrap();
    assert_eq!(RNTuple::open(&f, "events").unwrap().num_entries(), 3);
    assert_eq!(
        RNTuple::open_in(&f, "cal", "pedestals")
            .unwrap()
            .read_field(&f, "p")
            .unwrap(),
        FieldValues::F64(vec![9.5, 8.5])
    );
}

#[test]
fn duplicate_names_are_rejected() {
    let dup = NtupleFile::new()
        .add(Ntuple::new("a", vec![Field::i32("x", vec![1])]))
        .add(Ntuple::new("a", vec![Field::i32("x", vec![2])]));
    assert!(dup.to_root_bytes("f.root", Compression::None).is_err());

    // A subdirectory name colliding with a top-level RNTuple is also rejected.
    let clash = NtupleFile::new()
        .add(Ntuple::new("cal", vec![Field::i32("x", vec![1])]))
        .dir("cal", |d| {
            d.add(Ntuple::new("p", vec![Field::i32("y", vec![1])]))
        });
    assert!(clash.to_root_bytes("f.root", Compression::None).is_err());
}
