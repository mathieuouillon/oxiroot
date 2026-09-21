//! M6: append histograms to an existing ROOT file (update mode). The append is
//! in place — existing objects (including subdirectories) never move — and the
//! file must hold the original objects plus the new ones, readable by our reader
//! (and, as separately verified, by official ROOT and uproot).

use std::path::PathBuf;

use oxiroot_hist::{FileWriter, Hist, ReadRoot, WriteRoot, TH1, TH2};
use oxiroot_io_core::{Compression, FileReader};

#[test]
fn appends_objects_to_an_existing_file() {
    let out = PathBuf::from("/tmp/rootrs_update.root");

    // Start with a one-histogram file.
    let mut a = Hist::reg(4, 0.0, 4.0).double().named("a").titled("first");
    a.fill(0.5);
    a.fill(2.5);
    a.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("initial write");

    // Append two more histograms (a TH1D and a TH2D).
    let mut b = Hist::reg(3, 0.0, 3.0).double().named("b").titled("second");
    b.fill(1.5);
    let mut c = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("c")
        .titled("third");
    c.fill(0.5, 1.5);
    FileWriter::open(&out)
        .expect("open for append")
        .add(&b)
        .add(&c)
        .write(oxiroot_io_core::Compression::None)
        .expect("append");

    // All three are present and intact.
    let f = FileReader::open(&out).expect("reopen");
    let names: Vec<&str> = f.keys().iter().map(|k| k.name.as_str()).collect();
    assert!(
        names.contains(&"a") && names.contains(&"b") && names.contains(&"c"),
        "{names:?}"
    );
    assert_eq!(TH1::read_root(&f, "a").unwrap(), a, "original survived");
    assert_eq!(TH1::read_root(&f, "b").unwrap(), b);
    assert_eq!(TH2::read_root(&f, "c").unwrap(), c);

    // The embedded streamer info is preserved across the update.
    let reg = f.streamer_registry().expect("streamer info");
    assert!(reg.class_names().contains(&"TH2D"));
}

#[test]
fn re_adding_a_name_bumps_the_cycle() {
    let out = PathBuf::from("/tmp/rootrs_update_cycle.root");
    let mut v1 = Hist::reg(4, 0.0, 4.0).double().named("h").titled("v1");
    v1.fill(0.5);
    v1.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write v1");

    // Re-add "h" with different contents; ROOT keeps both at different cycles,
    // newest (highest cycle) wins for a plain lookup.
    let mut v2 = Hist::reg(4, 0.0, 4.0).double().named("h").titled("v2");
    v2.fill(1.5);
    v2.fill(1.5);
    FileWriter::open(&out)
        .expect("open for append")
        .add(&v2)
        .write(oxiroot_io_core::Compression::None)
        .expect("append v2");

    let f = FileReader::open(&out).expect("reopen");
    let cycles: Vec<u16> = f
        .keys()
        .iter()
        .filter(|k| k.name == "h")
        .map(|k| k.cycle)
        .collect();
    assert_eq!(cycles.len(), 2, "both cycles present: {cycles:?}");
    assert!(cycles.contains(&1) && cycles.contains(&2), "{cycles:?}");
    // Our reader returns the highest cycle (newest) -> v2.
    assert_eq!(TH1::read_root(&f, "h").unwrap(), v2, "newest cycle wins");
}

/// Appending into a file that already holds a subdirectory keeps the
/// subdirectory and its objects intact (append-in-place never moves them).
/// Verified separately against ROOT C++ and uproot.
#[test]
fn appends_to_a_file_with_a_subdirectory() {
    let out = PathBuf::from("/tmp/rootrs_update_subdir.root");

    let mut a = Hist::reg(4, 0.0, 4.0).double().named("a").titled("root");
    a.fill(0.5);
    let mut s = Hist::reg(3, 0.0, 3.0)
        .double()
        .named("s")
        .titled("in subdir");
    s.fill(1.5);
    FileWriter::create(&out)
        .add(&a)
        .dir("region", |d| d.add(&s))
        .write(Compression::None)
        .expect("create with subdir");

    let mut b = Hist::reg(2, 0.0, 2.0)
        .double()
        .named("b")
        .titled("appended");
    b.fill(0.5);
    FileWriter::open(&out)
        .expect("open for append")
        .add(&b)
        .write(Compression::None)
        .expect("append");

    let f = FileReader::open(&out).expect("reopen");
    let names: Vec<&str> = f.keys().iter().map(|k| k.name.as_str()).collect();
    assert!(
        names.contains(&"a") && names.contains(&"region") && names.contains(&"b"),
        "{names:?}"
    );
    assert_eq!(
        TH1::read_root(&f, "a").unwrap(),
        a,
        "original root object survived"
    );
    assert_eq!(TH1::read_root(&f, "b").unwrap(), b, "appended object");
    assert_eq!(
        TH1::read_root_in(&f, "region", "s").unwrap(),
        s,
        "subdirectory object survived"
    );
}

/// Adding a *new* subdirectory while appending is rejected (only top-directory
/// objects can be appended; existing subdirectories are preserved untouched).
#[test]
fn adding_a_new_subdir_during_append_is_rejected() {
    let out = PathBuf::from("/tmp/rootrs_update_newdir.root");
    let mut a = Hist::reg(4, 0.0, 4.0).double().named("a");
    a.fill(0.5);
    a.write_root(&out, Compression::None).expect("write");

    let s = Hist::reg(2, 0.0, 2.0).double().named("s");
    let err = FileWriter::open(&out)
        .expect("open")
        .dir("new", |d| d.add(&s))
        .write(Compression::None)
        .unwrap_err();
    assert!(format!("{err}").contains("new subdirectories"), "{err}");
}

#[test]
fn appending_adds_the_streamer_info_the_file_lacks() {
    // A histogram-only file describes the histogram classes; appending a
    // parameter adds its class to that list and keeps every existing entry.
    let out = std::env::temp_dir().join("oxiroot_update_streamers.root");
    let mut h = Hist::reg(2, 0.0, 2.0).double().named("h");
    h.fill(0.5);
    h.write_root(&out, Compression::None)
        .expect("initial write");
    let before = FileReader::open(&out).unwrap().streamer_registry().unwrap();
    assert!(before.get("TParameter<double>").is_none());

    FileWriter::open(&out)
        .expect("open")
        .add(&oxiroot_hist::TParameter::f64("lumi", 12.5))
        .write(Compression::None)
        .expect("append");

    let f = FileReader::open(&out).unwrap();
    let after = f.streamer_registry().expect("merged streamer info parses");
    assert!(after.get("TParameter<double>").is_some());
    assert_eq!(
        &after.infos()[..before.infos().len()],
        before.infos(),
        "existing entries are kept, in order"
    );
    assert_eq!(TH1::read_root(&f, "h").unwrap(), h);

    // Appending again with nothing new leaves the record where it is.
    let seek_info = f.header().seek_info;
    FileWriter::open(&out)
        .expect("open")
        .add(&oxiroot_hist::TParameter::f64("lumi2", 1.0))
        .write(Compression::None)
        .expect("append again");
    assert_eq!(
        FileReader::open(&out).unwrap().header().seek_info,
        seek_info
    );
}
