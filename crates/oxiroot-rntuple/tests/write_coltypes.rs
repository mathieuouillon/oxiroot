//! Write the small/medium integer encodings (Int8/UInt8/Int16/UInt16, scalar +
//! vector) and the reduced-precision reals (half / truncated / quantized), then
//! read them back through our own reader.

use oxiroot_io_core::{Compression, RFile};
use oxiroot_rntuple::{Field, FieldValues, RNTuple};

fn round_trip(fields: &[Field], tag: &str) -> RFile {
    let out = std::env::temp_dir().join(format!("oxiroot_write_coltypes_{tag}.root"));
    oxiroot_rntuple::write_rntuple_file(&out, "ntpl", fields, Compression::None).expect("write");
    RFile::open(&out).expect("reopen")
}

#[test]
fn writes_small_integers() {
    let fields = vec![
        Field::i8("i8", vec![-2, -1, 0, 1, 2]),
        Field::u8("u8", vec![250, 251, 252, 253, 254]),
        Field::i16("i16", vec![-2000, -1000, 0, 1000, 2000]),
        Field::u16("u16", vec![5, 10005, 20005, 30005, 40005]),
        Field::vec_i16(
            "vi16",
            vec![
                vec![],
                vec![101],
                vec![102, 102],
                vec![103; 3],
                vec![104; 4],
            ],
        ),
    ];
    let file = round_trip(&fields, "ints");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open");
    let field = |n| ntpl.read_field(&file, n).expect("read");

    assert_eq!(field("i8"), FieldValues::I8(vec![-2, -1, 0, 1, 2]));
    assert_eq!(field("u8"), FieldValues::U8(vec![250, 251, 252, 253, 254]));
    assert_eq!(
        field("i16"),
        FieldValues::I16(vec![-2000, -1000, 0, 1000, 2000])
    );
    assert_eq!(
        field("u16"),
        FieldValues::U16(vec![5, 10005, 20005, 30005, 40005])
    );
    assert_eq!(
        field("vi16"),
        FieldValues::VecI16(vec![
            vec![],
            vec![101],
            vec![102, 102],
            vec![103; 3],
            vec![104; 4]
        ])
    );
}

#[test]
fn writes_reduced_precision_reals() {
    let exact = vec![0.5_f32, 1.0, 2.0, 4.0, 8.0]; // exact under half + 16-bit trunc
    let fields = vec![
        Field::half("h", exact.clone()),
        Field::truncated("t", exact.clone(), 16),
        Field::quantized("q", vec![0.0, 25.0, 50.0, 75.0, 100.0], 0.0, 100.0, 16),
        Field::quantized("q12", vec![0.0, 25.0, 50.0, 75.0, 100.0], 0.0, 100.0, 12),
    ];
    let file = round_trip(&fields, "reals");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open");
    let f32s = |n| match ntpl.read_field(&file, n).expect("read") {
        FieldValues::F32(v) => v,
        other => panic!("expected F32, got {other:?}"),
    };

    // Half and 16-bit truncation are lossless for these values.
    assert_eq!(f32s("h"), exact);
    assert_eq!(f32s("t"), exact);

    // Quantized values round-trip to within the quantization step.
    for (n, tol) in [("q", 1e-3_f32), ("q12", 3e-2)] {
        let got = f32s(n);
        for (i, (&g, &w)) in got.iter().zip(&[0.0, 25.0, 50.0, 75.0, 100.0]).enumerate() {
            assert!((g - w).abs() <= tol, "{n}[{i}] = {g}, want ~{w}");
        }
    }
}
