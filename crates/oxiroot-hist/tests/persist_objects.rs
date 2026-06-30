//! Small persistable objects: `TObjString` (a labelled string) and
//! `TParameter<T>` (a named scalar). oxiroot reads the ROOT-C++-written
//! `persist_objs.root` fixture and round-trips its own writes; ROOT C++ reads
//! oxiroot's output (checked out of band), and uproot reads the `TObjString`.

use std::path::PathBuf;

use oxiroot_hist::{ParamValue, ReadRoot, RootFile, TObjString, TParameter};
use oxiroot_io_core::{Compression, RFile};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/persist_objs.root"))
        .expect("open fixture")
}

#[test]
fn reads_root_written_objects() {
    let f = fixture();
    assert_eq!(
        TObjString::read_root(&f, "label").unwrap().value(),
        "hello world"
    );
    assert_eq!(
        TParameter::read_root(&f, "lumi").unwrap().value(),
        ParamValue::Double(137.5)
    );
    assert_eq!(
        TParameter::read_root(&f, "nevents").unwrap().value(),
        ParamValue::Int(42)
    );
    assert_eq!(
        TParameter::read_root(&f, "bignum").unwrap().value(),
        ParamValue::Long64(9_000_000_000)
    );
}

#[test]
fn round_trips_objects_through_oxiroot() {
    let out = std::env::temp_dir().join("oxiroot_persist_rt.root");
    RootFile::create(&out)
        .add(&TObjString::new("hello world").named("label"))
        .add(&TParameter::f64("lumi", 137.5))
        .add(&TParameter::i32("nevents", 42))
        .add(&TParameter::i64("bignum", 9_000_000_000))
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&out).unwrap();
    assert_eq!(
        TObjString::read_root(&f, "label").unwrap().value(),
        "hello world"
    );
    assert_eq!(
        TParameter::read_root(&f, "lumi").unwrap().value().as_f64(),
        137.5
    );
    assert_eq!(
        TParameter::read_root(&f, "nevents").unwrap().value(),
        ParamValue::Int(42)
    );
    assert_eq!(
        TParameter::read_root(&f, "bignum").unwrap().value(),
        ParamValue::Long64(9_000_000_000)
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn byte_exact_against_root() {
    // oxiroot's serialized object bytes must equal ROOT's, key-for-key.
    use oxiroot_hist::WriteRoot;
    let f = fixture();
    let cases: [(&str, &dyn WriteRoot); 4] = [
        ("label", &TObjString::new("hello world").named("label")),
        ("lumi", &TParameter::f64("lumi", 137.5)),
        ("nevents", &TParameter::i32("nevents", 42)),
        ("bignum", &TParameter::i64("bignum", 9_000_000_000)),
    ];
    for (name, obj) in cases {
        let key = f.key(name).unwrap();
        let root_bytes =
            oxiroot_compress::decompress(key.payload(f.data()).unwrap(), key.obj_len as usize)
                .unwrap();
        assert_eq!(
            obj.to_root_bytes(),
            root_bytes,
            "object bytes differ for {name:?}"
        );
    }
}
