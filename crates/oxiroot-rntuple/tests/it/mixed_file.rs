//! An RNTuple next to other objects in one file, written without the histogram
//! crate: this crate depends only on io-core (and, for this test, linalg).

use oxiroot_io_core::{Compression, RFile, ReadRoot, RootFile, TParameter, WriteRoot};
use oxiroot_linalg::TMatrixD;
use oxiroot_rntuple::{Field, FieldValues, Ntuple, RNTuple};

#[test]
fn a_matrix_a_parameter_and_an_rntuple_share_a_file() {
    let path = std::env::temp_dir().join("oxiroot_rntuple_mixed.root");
    let cov = TMatrixD::new(2, 2, vec![1.0, 0.5, 0.5, 2.0]).named("cov");
    RootFile::create(&path)
        .add(&cov)
        .put(Ntuple::new("events", vec![Field::f64("x", vec![0.5, 1.5])]))
        .add(&TParameter::f64("lumi", 12.5))
        .dir("aux", |d| {
            d.put(Ntuple::new("runs", vec![Field::i32("run", vec![7])]))
        })
        .write(Compression::Zstd(3))
        .unwrap();

    let f = RFile::open(&path).unwrap();
    let names: Vec<&str> = f.keys().iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, ["cov", "events", "lumi", "aux"]);
    assert_eq!(TMatrixD::read_root(&f, "cov").unwrap(), cov);
    assert_eq!(
        TParameter::read_root(&f, "lumi").unwrap().value().as_f64(),
        12.5
    );
    assert_eq!(
        RNTuple::open(&f, "events")
            .unwrap()
            .read_field(&f, "x")
            .unwrap(),
        FieldValues::F64(vec![0.5, 1.5])
    );
    assert_eq!(
        RNTuple::open_in(&f, "aux", "runs")
            .unwrap()
            .read_field(&f, "run")
            .unwrap(),
        FieldValues::I32(vec![7])
    );
    // Every class is described, and the histogram list is not needed.
    let registry = f.streamer_registry().unwrap();
    for class in ["ROOT::RNTuple", "TMatrixT<double>", "TParameter<double>"] {
        assert!(registry.get(class).is_some(), "{class} is described");
    }
    assert!(registry.get("TH1D").is_none());
}

#[test]
fn an_rntuple_can_be_appended_to_an_existing_file() {
    let path = std::env::temp_dir().join("oxiroot_rntuple_append_put.root");
    TParameter::i32("n", 3)
        .write_root(&path, Compression::None)
        .unwrap();
    RootFile::open(&path)
        .unwrap()
        .put(Ntuple::new("late", vec![Field::i32("x", vec![4, 5])]))
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&path).unwrap();
    assert_eq!(
        TParameter::read_root(&f, "n").unwrap().value().as_f64(),
        3.0
    );
    assert_eq!(
        RNTuple::open(&f, "late")
            .unwrap()
            .read_field(&f, "x")
            .unwrap(),
        FieldValues::I32(vec![4, 5])
    );
    assert!(f
        .streamer_registry()
        .unwrap()
        .get("ROOT::RNTuple")
        .is_some());
}

#[test]
fn a_forced_64_bit_file_with_rntuples_reads_back() {
    let path = std::env::temp_dir().join("oxiroot_rntuple_put_big.root");
    RootFile::create(&path)
        .put(Ntuple::new("a", vec![Field::f32("x", vec![1.0, 2.0])]))
        .dir("d", |d| {
            d.put(Ntuple::new("b", vec![Field::i64("y", vec![3])]))
        })
        .write_threshold(Compression::None, 0)
        .unwrap();
    let f = RFile::open(&path).unwrap();
    assert!(f.header().is_big());
    assert_eq!(
        RNTuple::open(&f, "a").unwrap().read_field(&f, "x").unwrap(),
        FieldValues::F32(vec![1.0, 2.0])
    );
    assert_eq!(
        RNTuple::open_in(&f, "d", "b")
            .unwrap()
            .read_field(&f, "y")
            .unwrap(),
        FieldValues::I64(vec![3])
    );
}
