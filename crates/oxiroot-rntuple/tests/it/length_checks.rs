//! The writer rejects fields whose data does not line up — top-level fields of
//! different lengths, and composite fields whose parts disagree — instead of
//! writing a file whose columns do not match its entry count.

use oxiroot_io_core::{Compression, Error, FileReader};
use oxiroot_rntuple::{write_rntuple_file, Column, Field, FieldValues, NtupleReader};

fn write(fields: &[Field]) -> oxiroot_io_core::Result<()> {
    let path = std::env::temp_dir().join("oxiroot_rntuple_length_checks.root");
    write_rntuple_file(&path, "events", fields, Compression::None)
}

/// The error of a result that must have failed (`Field` has no `Debug`, so
/// `unwrap_err` is not available).
fn err<T>(result: oxiroot_io_core::Result<T>) -> Error {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    }
}

fn mismatch(what: &str, expected: usize, found: usize) -> Error {
    Error::LengthMismatch {
        what: what.into(),
        expected,
        found,
    }
}

#[test]
fn fields_of_different_lengths_are_rejected() {
    let err = write(&[
        Field::i32("x", vec![1, 2, 3]),
        Field::f64("y", vec![1.0, 2.0]),
    ])
    .unwrap_err();
    assert_eq!(err, mismatch("field \"y\" entries", 3, 2));
}

#[test]
fn record_members_of_different_lengths_are_rejected() {
    let record = Column::Record(vec![
        ("a".into(), Column::I32(vec![1, 2])),
        ("b".into(), Column::F64(vec![1.0])),
    ]);
    let err = write(&[Field::new("r", record)]).unwrap_err();
    assert_eq!(err, mismatch("field \"r\" member \"b\"", 2, 1));
}

#[test]
fn collection_offsets_must_end_at_the_item_count() {
    let nested = Column::Nested {
        offsets: vec![0, 2, 3],
        items: Box::new(Column::VecF64(vec![vec![1.0], vec![2.0]])),
    };
    let err = write(&[Field::new("v", nested)]).unwrap_err();
    assert_eq!(err, mismatch("field \"v\" items", 3, 2));

    // The offsets bound each entry, so they start at the first entry's start.
    let no_zero = Column::Nested {
        offsets: vec![1, 2],
        items: Box::new(Column::VecF64(vec![vec![1.0], vec![2.0]])),
    };
    let err = write(&[Field::new("v", no_zero)]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("start with 0")),
        "{err:?}"
    );

    let decreasing = Column::Nested {
        offsets: vec![0, 2, 1],
        items: Box::new(Column::VecF64(vec![vec![1.0], vec![2.0]])),
    };
    let err = write(&[Field::new("v", decreasing)]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("decrease")),
        "{err:?}"
    );

    // A map's keys and values are record members: they must match too.
    let map = Field::map(
        "m",
        "std::int32_t",
        "double",
        vec![0, 2],
        Column::I32(vec![1, 2]),
        Column::F64(vec![0.5]),
    );
    let err = write(&[map]).unwrap_err();
    assert_eq!(err, mismatch("field \"m\" items member \"_1\"", 2, 1));
}

#[test]
fn variant_tags_must_match_their_alternatives() {
    let err = write(&[Field::variant(
        "v",
        vec![Column::I32(vec![1]), Column::F64(vec![])],
        vec![1, 2],
    )])
    .unwrap_err();
    assert_eq!(err, mismatch("field \"v\" alternative 1", 1, 0));

    let err = write(&[Field::variant("v", vec![Column::I32(vec![1])], vec![3])]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("variant tag 3")),
        "{err:?}"
    );
}

#[test]
fn optional_values_must_match_the_presence_mask() {
    let optional = Column::Optional {
        unique: false,
        present: vec![true, false, true],
        values: Box::new(Column::F64(vec![1.0])),
    };
    let err = write(&[Field::new("o", optional)]).unwrap_err();
    assert_eq!(err, mismatch("field \"o\" values", 2, 1));
}

#[test]
fn fixed_size_fields_reject_ragged_entries() {
    let err = err(Field::array_f64("a", vec![vec![1.0, 2.0], vec![3.0]]));
    assert_eq!(err, mismatch("field \"a\" entry 1", 2, 1));
    let err = self::err(Field::bitset("b", vec![vec![true], vec![true, false]]));
    assert_eq!(err, mismatch("field \"b\" entry 1", 1, 2));

    // A hand-built array whose items do not divide into whole arrays.
    let array = Column::Array {
        len: 2,
        items: Box::new(Column::F64(vec![1.0, 2.0, 3.0])),
    };
    let err = write(&[Field::new("a", array)]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("do not divide")),
        "{err:?}"
    );
}

#[test]
fn a_zero_size_array_does_not_set_the_entry_count() {
    // `std::array<T, 0>` holds no values, whatever the entry count, so it neither
    // conflicts with the other fields nor, when first, decides the entry count.
    let path = std::env::temp_dir().join("oxiroot_rntuple_zero_array.root");
    let fields = [
        Field::array_f64("empty", vec![vec![]; 3]).ok().unwrap(),
        Field::i32("x", vec![1, 2, 3]),
    ];
    write_rntuple_file(&path, "events", &fields, Compression::None).unwrap();
    let f = FileReader::open(&path).unwrap();
    let nt = NtupleReader::open(&f, "events").unwrap();
    assert_eq!(nt.num_entries(), 3);
    assert_eq!(
        nt.read_field(&f, "x").unwrap(),
        FieldValues::I32(vec![1, 2, 3])
    );
}
