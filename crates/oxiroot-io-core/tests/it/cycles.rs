//! Reading an object at an explicit cycle (`"name;N"`).
//!
//! `fixtures/cycles.root` is written by ROOT 6.40 (`scripts/gen_cycles.cpp`):
//! three cycles of `h` whose single bin holds the cycle number, a deleted `gone`,
//! and two cycles of `sub/d` holding 10 and 20. ROOT reads `h` as `h;3`, `h;1` as
//! the first cycle, and `gone` as nothing; the assertions here are those answers.

use oxiroot_io_core::{split_cycle, FileReader, Value};

fn fixture() -> String {
    format!("{}/../../fixtures/cycles.root", env!("CARGO_MANIFEST_DIR"))
}

/// The one bin of the `TH1D` at `name`, through the generic reader.
fn bin(file: &FileReader, name: &str) -> Option<f64> {
    let v = file.get_value(name).ok()?;
    v.get("fArray")
        .and_then(Value::as_array)
        .and_then(|a| a.get(1))
        .and_then(Value::as_f64)
}

#[test]
fn a_name_reads_its_highest_cycle_and_a_cycle_reads_that_one() {
    let f = FileReader::open(fixture()).unwrap();

    // Three cycles are on file; the name alone reads the current one.
    assert_eq!(f.keys().iter().filter(|k| k.name == "h").count(), 3);
    assert_eq!(f.key("h").map(|k| k.cycle), Some(3));
    assert_eq!(bin(&f, "h"), Some(3.0));

    // Each cycle reads back the value written at that cycle.
    assert_eq!(f.key("h;1").map(|k| k.cycle), Some(1));
    assert_eq!(bin(&f, "h;1"), Some(1.0));
    assert_eq!(bin(&f, "h;2"), Some(2.0));
    assert_eq!(bin(&f, "h;3"), Some(3.0));

    // A cycle the file does not hold is not found, and does not fall back to
    // another cycle.
    assert!(f.key("h;4").is_none());
    assert!(f.get_value("h;4").is_err());
}

#[test]
fn a_cycle_reads_the_same_way_in_a_subdirectory() {
    let f = FileReader::open(fixture()).unwrap();
    let dir = f.subdir("sub").unwrap();
    assert_eq!(dir.keys.iter().filter(|k| k.name == "d").count(), 2);

    let value = |name: &str| {
        f.get_value_in("sub", name).ok().and_then(|v| {
            v.get("fArray")
                .and_then(Value::as_array)
                .and_then(|a| a.get(1))
                .and_then(Value::as_f64)
        })
    };
    assert_eq!(value("d"), Some(20.0));
    assert_eq!(value("d;1"), Some(10.0));
    assert_eq!(value("d;2"), Some(20.0));
    assert!(f.get_value_in("sub", "d;3").is_err());
}

#[test]
fn a_deleted_key_is_not_read_even_at_its_cycle() {
    // `gone` was written and then deleted, which takes its key out of the
    // directory; neither the name nor its cycle finds anything, as in ROOT.
    let f = FileReader::open(fixture()).unwrap();
    assert!(!f.keys().iter().any(|k| k.name == "gone"));
    assert!(f.key("gone").is_none());
    assert!(f.key("gone;1").is_none());
    assert!(f.get_value("gone;1").is_err());
}

#[test]
fn a_name_that_carries_a_semicolon_stays_whole() {
    assert_eq!(split_cycle("h"), ("h", None));
    assert_eq!(split_cycle("h;2"), ("h", Some(2)));
    // Only a cycle number separates: anything else is part of the name.
    assert_eq!(split_cycle("a;b"), ("a;b", None));
    assert_eq!(split_cycle("h;*"), ("h;*", None));
    assert_eq!(split_cycle("h;-1"), ("h;-1", None));
    // ROOT writes cycles from 1 up, so the last `;` is the one that separates.
    assert_eq!(split_cycle("a;b;3"), ("a;b", Some(3)));
}
