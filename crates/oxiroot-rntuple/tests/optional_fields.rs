//! Nullable / "late" RNTuple fields — `std::optional<T>` and `std::unique_ptr<T>`
//! — plus `std::atomic<T>`. On disk an optional/unique_ptr is a `Collection` of
//! 0-or-1 elements; oxiroot reads it back as a nullable [`FieldValues::Opt`].
//! `std::atomic<T>` is stored as the bare `T`. Round-tripped here; the written
//! files are also read by ROOT C++ (`RNTupleReader`) and uproot.

use oxiroot_io_core::{Compression, RFile};
use oxiroot_rntuple::{Field, FieldValues, Ntuple, RNTuple};

fn write(path: &str, compression: Compression) {
    Ntuple::new(
        "ntpl",
        vec![
            Field::optional_f32("maybe", vec![Some(1.5), None, Some(3.5), None]),
            Field::unique_ptr_i32("late", vec![Some(10), None, Some(30), None]),
            Field::atomic_i32("counter", vec![0, 100, 200, 300]),
        ],
    )
    .write_root(path, compression)
    .unwrap();
}

#[test]
fn optional_and_unique_ptr_round_trip_as_nullable() {
    let path = std::env::temp_dir().join("oxiroot_opt.root");
    let path = path.to_str().unwrap();
    write(path, Compression::None);
    let f = RFile::open(path).unwrap();
    let nt = RNTuple::open(&f, "ntpl").unwrap();

    // optional<float> reads back as a nullable.
    let maybe = nt.read_field(&f, "maybe").unwrap();
    assert!(matches!(maybe, FieldValues::Opt { .. }));
    assert_eq!(
        maybe.opt_f32(),
        Some(vec![Some(1.5), None, Some(3.5), None])
    );
    // unique_ptr<int> reads back as a nullable too.
    assert_eq!(
        nt.read_field(&f, "late").unwrap().opt_i32(),
        Some(vec![Some(10), None, Some(30), None])
    );
}

#[test]
fn atomic_reads_as_a_scalar() {
    let path = std::env::temp_dir().join("oxiroot_atomic.root");
    let path = path.to_str().unwrap();
    write(path, Compression::None);
    let f = RFile::open(path).unwrap();
    let nt = RNTuple::open(&f, "ntpl").unwrap();
    assert_eq!(
        nt.read_field(&f, "counter").unwrap(),
        FieldValues::I32(vec![0, 100, 200, 300])
    );
}

#[test]
fn nullable_fields_compress() {
    let path = std::env::temp_dir().join("oxiroot_opt_zstd.root");
    let path = path.to_str().unwrap();
    write(path, Compression::Zstd(5));
    let f = RFile::open(path).unwrap();
    let nt = RNTuple::open(&f, "ntpl").unwrap();
    assert_eq!(
        nt.read_field(&f, "maybe").unwrap().opt_f32(),
        Some(vec![Some(1.5), None, Some(3.5), None])
    );
    assert_eq!(
        nt.read_field(&f, "counter").unwrap(),
        FieldValues::I32(vec![0, 100, 200, 300])
    );
}

#[test]
fn all_present_and_all_absent() {
    let path = std::env::temp_dir().join("oxiroot_opt_edge.root");
    let path = path.to_str().unwrap();
    Ntuple::new(
        "ntpl",
        vec![
            Field::optional_f64("full", vec![Some(1.0), Some(2.0), Some(3.0)]),
            Field::optional_f64("empty", vec![None, None, None]),
        ],
    )
    .write_root(path, Compression::None)
    .unwrap();
    let f = RFile::open(path).unwrap();
    let nt = RNTuple::open(&f, "ntpl").unwrap();
    assert_eq!(
        nt.read_field(&f, "full").unwrap().opt_f64(),
        Some(vec![Some(1.0), Some(2.0), Some(3.0)])
    );
    assert_eq!(
        nt.read_field(&f, "empty").unwrap().opt_f64(),
        Some(vec![None, None, None])
    );
}
