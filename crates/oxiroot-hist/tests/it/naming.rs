//! Naming is optional at construction and a write-time/file-key concern — and
//! same-name collisions are a loud error, not ROOT's silent shadow-on-read.

use oxiroot_hist::{Compression, Hist, ReadRoot, RootFile, WriteRoot, TH1};
use oxiroot_io_core::{Error, RFile};

fn filled(name: &str) -> TH1 {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named(name);
    for b in 0..4 {
        h.fill(b as f64 + 0.5);
    }
    h
}

#[test]
fn histograms_are_anonymous_until_named() {
    // No name forced at construction.
    let h = Hist::reg(10, 0.0, 1.0).double();
    assert_eq!(h.name, "");
    assert_eq!(h.title, "");
    // `named`/`titled` are chainable and set the fields.
    let h = h.named("pt").titled("p_{T}");
    assert_eq!(h.name, "pt");
    assert_eq!(h.title, "p_{T}");
}

#[test]
fn writing_an_unnamed_object_is_a_clear_error() {
    let path = std::env::temp_dir().join("oxiroot_naming_anon.root");
    let err = Hist::reg(4, 0.0, 4.0)
        .double()
        .write_root(&path, Compression::None);
    match err {
        Err(Error::Format(msg)) => assert!(msg.contains("unnamed"), "got: {msg}"),
        other => panic!("expected an unnamed-object error, got {other:?}"),
    }
}

#[test]
fn duplicate_key_in_one_directory_is_rejected() {
    let path = std::env::temp_dir().join("oxiroot_naming_dup.root");
    let err = RootFile::create(&path)
        .add(&filled("h"))
        .add(&filled("h")) // same key in the top directory
        .write(Compression::None);
    match err {
        Err(Error::DuplicateName { name, location }) => {
            assert_eq!(name, "h");
            assert!(location.contains("top"), "got: {location}");
        }
        other => panic!("expected DuplicateName, got {other:?}"),
    }
}

#[test]
fn same_name_in_different_directories_is_fine() {
    // A top-level "h" and a "h" inside a subdirectory are distinct keys.
    let path = std::env::temp_dir().join("oxiroot_naming_dirs.root");
    RootFile::create(&path)
        .add(&filled("h"))
        .dir("sub", |d| d.add(&filled("h")))
        .write(Compression::None)
        .expect("distinct namespaces — no collision");

    let f = RFile::open(&path).expect("open");
    assert_eq!(TH1::read_root(&f, "h").unwrap().entries, 4.0);
    assert_eq!(TH1::read_root_in(&f, "sub", "h").unwrap().entries, 4.0);
}

#[test]
fn duplicate_within_a_subdirectory_is_rejected() {
    let path = std::env::temp_dir().join("oxiroot_naming_subdup.root");
    let err = RootFile::create(&path)
        .dir("sub", |d| d.add(&filled("h")).add(&filled("h")))
        .write(Compression::None);
    assert!(
        matches!(err, Err(Error::DuplicateName { .. })),
        "expected DuplicateName, got {err:?}"
    );
}

#[test]
fn a_subdirectory_named_like_an_object_is_rejected() {
    // A key and a subdirectory share their parent's namespace.
    let path = std::env::temp_dir().join("oxiroot_naming_dir_clash.root");
    let err = RootFile::create(&path)
        .add(&filled("region"))
        .dir("region", |d| d.add(&filled("h")))
        .write(Compression::None);
    match err {
        Err(Error::DuplicateName { name, .. }) => assert_eq!(name, "region"),
        other => panic!("expected DuplicateName, got {other:?}"),
    }
}

#[test]
fn two_subdirectories_with_one_name_are_rejected() {
    let path = std::env::temp_dir().join("oxiroot_naming_dir_twice.root");
    let err = RootFile::create(&path)
        .dir("sub", |d| d.add(&filled("a")))
        .dir("sub", |d| d.add(&filled("b")))
        .write(Compression::None);
    assert!(
        matches!(err, Err(Error::DuplicateName { .. })),
        "expected DuplicateName, got {err:?}"
    );
}

#[test]
fn an_unnamed_subdirectory_is_rejected() {
    let path = std::env::temp_dir().join("oxiroot_naming_dir_empty.root");
    let err = RootFile::create(&path)
        .dir("", |d| d.add(&filled("a")))
        .write(Compression::None);
    assert!(matches!(err, Err(Error::Format(_))), "got {err:?}");
}

#[test]
fn names_longer_than_255_bytes_round_trip() {
    // ROOT encodes such strings with a five-byte length; the key header length
    // has to count it.
    let name = "h".repeat(300);
    let dir = "d".repeat(256);
    let path = std::env::temp_dir().join("oxiroot_naming_long.root");
    RootFile::create(&path)
        .add(&filled(&name))
        .dir(dir.as_str(), |d| d.add(&filled(&name)))
        .write(Compression::Zstd(3))
        .expect("write");
    let f = RFile::open(&path).expect("open");
    assert_eq!(TH1::read_root(&f, &name).unwrap(), filled(&name));
    assert_eq!(TH1::read_root_in(&f, &dir, &name).unwrap(), filled(&name));
}
