//! Naming is optional at construction and a write-time/file-key concern — and
//! same-name collisions are a loud error, not ROOT's silent shadow-on-read.

use oxiroot_hist::{Compression, ReadRoot, RootFile, WriteRoot, TH1};
use oxiroot_io_core::{Error, RFile};

fn filled(name: &str) -> TH1 {
    let mut h = TH1::new(4, 0.0, 4.0).named(name);
    for b in 0..4 {
        h.fill(b as f64 + 0.5);
    }
    h
}

#[test]
fn histograms_are_anonymous_until_named() {
    // No name forced at construction.
    let h = TH1::new(10, 0.0, 1.0);
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
    let err = TH1::new(4, 0.0, 4.0).write_root(&path, Compression::None);
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
