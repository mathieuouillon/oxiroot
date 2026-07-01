//! M6: append histograms to an existing ROOT file (update mode). The append is
//! in place — existing objects (including subdirectories) never move — and the
//! file must hold the original objects plus the new ones, readable by our reader
//! (and, as separately verified, by official ROOT and uproot).

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, RootFile, WriteRoot, TH1, TH2};
use oxiroot_io_core::{Compression, RFile};

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
    RootFile::open(&out)
        .expect("open for append")
        .add(&b)
        .add(&c)
        .write(oxiroot_io_core::Compression::None)
        .expect("append");

    // All three are present and intact.
    let f = RFile::open(&out).expect("reopen");
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
    RootFile::open(&out)
        .expect("open for append")
        .add(&v2)
        .write(oxiroot_io_core::Compression::None)
        .expect("append v2");

    let f = RFile::open(&out).expect("reopen");
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
    RootFile::create(&out)
        .add(&a)
        .dir("region", |d| d.add(&s))
        .write(Compression::None)
        .expect("create with subdir");

    let mut b = Hist::reg(2, 0.0, 2.0)
        .double()
        .named("b")
        .titled("appended");
    b.fill(0.5);
    RootFile::open(&out)
        .expect("open for append")
        .add(&b)
        .write(Compression::None)
        .expect("append");

    let f = RFile::open(&out).expect("reopen");
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
    let err = RootFile::open(&out)
        .expect("open")
        .dir("new", |d| d.add(&s))
        .write(Compression::None)
        .unwrap_err();
    assert!(format!("{err}").contains("new subdirectories"), "{err}");
}
