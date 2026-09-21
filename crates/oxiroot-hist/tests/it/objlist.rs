//! A bare collection of objects under one key: `ObjList`, as a `TList` or
//! `TObjArray`. oxiroot reads the ROOT-C++-written `objlist.root` fixture — whose
//! members use the object protocol (repeats via class back-references) — and
//! round-trips its own writes; ROOT C++ and uproot read oxiroot's output
//! (checked out of band).

use std::path::PathBuf;

use oxiroot_hist::{Hist, ListKind, ObjList, ReadRoot, RootFile, TObjString, TParameter, TH1};
use oxiroot_io_core::{Compression, RFile};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/objlist.root"))
        .expect("open fixture")
}

#[test]
fn reads_root_written_list_and_array() {
    let f = fixture();

    let l = ObjList::read_root(&f, "mylist").unwrap();
    assert_eq!(l.name(), "mylist");
    assert_eq!(l.kind(), ListKind::List);
    assert_eq!(l.len(), 3);
    assert_eq!(
        l.class_names().collect::<Vec<_>>(),
        ["TH1F", "TObjString", "TParameter<double>"]
    );
    // Pull members out by type.
    assert_eq!(l.items::<TH1>().unwrap().len(), 1);
    assert_eq!(l.items::<TObjString>().unwrap()[0].value(), "hello");
    assert_eq!(l.items::<TParameter>().unwrap()[0].value().as_f64(), 12.5);

    let a = ObjList::read_root(&f, "myarr").unwrap();
    assert_eq!(a.kind(), ListKind::Array);
    assert_eq!(a.len(), 2);
    assert_eq!(a.items::<TH1>().unwrap().len(), 2);
}

#[test]
fn round_trips_list_and_array_through_oxiroot() {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("h");
    h.fill(0.5);
    let list = ObjList::list()
        .named("mylist")
        .add(&h)
        .add(&TObjString::new("hello"))
        .add(&TParameter::f64("lumi", 12.5));
    let arr = ObjList::array()
        .named("myarr")
        .add(&Hist::reg(3, 0.0, 3.0).double().named("a0"))
        .add(&Hist::reg(3, 0.0, 3.0).double().named("a1"));

    let out = std::env::temp_dir().join("oxiroot_objlist_rt.root");
    RootFile::create(&out)
        .add(&list)
        .add(&arr)
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&out).unwrap();
    let l = ObjList::read_root(&f, "mylist").unwrap();
    assert_eq!(l.len(), 3);
    assert_eq!(l.items::<TH1>().unwrap().len(), 1);
    assert_eq!(l.items::<TObjString>().unwrap()[0].value(), "hello");
    assert_eq!(l.items::<TParameter>().unwrap()[0].value().as_f64(), 12.5);
    let a = ObjList::read_root(&f, "myarr").unwrap();
    assert_eq!(a.kind(), ListKind::Array);
    assert_eq!(a.items::<TH1>().unwrap().len(), 2);
    let _ = std::fs::remove_file(&out);
}
