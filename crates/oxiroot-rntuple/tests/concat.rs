//! Concatenating RNTuples with [`concat_ntuples`] — the `hadd` building block.
use oxiroot_io_core::{Compression, RFile};
use oxiroot_rntuple::{concat_ntuples, Field, FieldValues, Ntuple, RNTuple};

/// Write `fields` as RNTuple `ntpl` to a fresh temp file and reopen it.
fn write_open(tag: &str, fields: Vec<Field>) -> RFile {
    let path = std::env::temp_dir().join(format!("oxiroot_rnconcat_{tag}.root"));
    Ntuple::new("ntpl", fields)
        .write_root(&path, Compression::None)
        .expect("write");
    RFile::open(&path).expect("reopen")
}

#[test]
fn concatenates_scalar_vector_and_string_fields() {
    let fa = write_open(
        "a",
        vec![
            Field::i32("i", vec![0, 1, 2]),
            Field::f64("x", vec![0.0, 1.0, 2.0]),
            Field::strings("s", vec!["a".into(), "b".into(), "c".into()]),
            Field::vec_i32("v", vec![vec![1], vec![], vec![2, 3]]),
        ],
    );
    let fb = write_open(
        "b",
        vec![
            Field::i32("i", vec![3, 4]),
            Field::f64("x", vec![3.0, 4.0]),
            Field::strings("s", vec!["d".into(), "e".into()]),
            Field::vec_i32("v", vec![vec![4, 5], vec![6]]),
        ],
    );
    let na = RNTuple::open(&fa, "ntpl").unwrap();
    let nb = RNTuple::open(&fb, "ntpl").unwrap();

    let merged = concat_ntuples("ntpl", &[(&fa, &na), (&fb, &nb)]).expect("concat");
    let path = std::env::temp_dir().join("oxiroot_rnconcat_out.root");
    merged
        .write_root(&path, Compression::None)
        .expect("write merged");
    let fo = RFile::open(&path).unwrap();
    let no = RNTuple::open(&fo, "ntpl").unwrap();

    assert_eq!(no.num_entries(), 5);
    assert_eq!(
        no.read_field(&fo, "i").unwrap(),
        FieldValues::I32(vec![0, 1, 2, 3, 4])
    );
    assert_eq!(
        no.read_field(&fo, "x").unwrap(),
        FieldValues::F64(vec![0.0, 1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(
        no.read_field(&fo, "s").unwrap(),
        FieldValues::Str(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into()
        ])
    );
    assert_eq!(
        no.read_field(&fo, "v").unwrap(),
        FieldValues::VecI32(vec![vec![1], vec![], vec![2, 3], vec![4, 5], vec![6]])
    );
}

#[test]
fn rejects_a_type_mismatch() {
    let fa = write_open("ty_a", vec![Field::i32("v", vec![1, 2])]);
    let fb = write_open("ty_b", vec![Field::f64("v", vec![3.0])]);
    let na = RNTuple::open(&fa, "ntpl").unwrap();
    let nb = RNTuple::open(&fb, "ntpl").unwrap();

    let Err(err) = concat_ntuples("ntpl", &[(&fa, &na), (&fb, &nb)]) else {
        panic!("expected an error for a type mismatch");
    };
    assert!(err.to_string().contains("different types"), "{err}");
}
