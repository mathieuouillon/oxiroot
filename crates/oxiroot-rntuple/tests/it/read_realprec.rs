//! Reduced-precision real column encodings written by official ROOT:
//! `Real16` (half), `Real32Trunc` (mantissa-truncated), and `Real32Quant`
//! (linearly quantized, including a sub-byte 12-bit width). All surface as f32.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

fn f32s(fv: FieldValues) -> Vec<f32> {
    match fv {
        FieldValues::F32(v) => v,
        other => panic!("expected F32, got {other:?}"),
    }
}

fn check(name: &str) {
    let file = open(name);
    let ntpl = RNTuple::open(&file, "ntpl").expect("open RNTuple");
    assert_eq!(ntpl.num_entries(), 5, "{name}");
    let field = |n| ntpl.read_field(&file, n).expect("read field");

    // half (Real16) and trunc (Real32Trunc, 16-bit) are exact for these values
    // (powers of two have an all-zero mantissa).
    assert_eq!(
        f32s(field("half")),
        [0.5, 1.0, 2.0, 4.0, 8.0],
        "{name} half"
    );
    assert_eq!(
        f32s(field("trunc")),
        [0.5, 1.0, 2.0, 4.0, 8.0],
        "{name} trunc"
    );

    // quant (Real32Quant, 16-bit, range [0,100]) carries a small quantization
    // error on the mid values — compare to ROOT/uproot's exact reconstruction.
    let q = f32s(field("quant"));
    let expect_q = [0.0, 25.000381, 50.000763, 74.99962, 100.0];
    for (i, (&got, &want)) in q.iter().zip(&expect_q).enumerate() {
        assert!(
            (got - want).abs() < 1e-4,
            "{name} quant[{i}] = {got}, want {want}"
        );
    }

    // quant12 (Real32Quant, 12-bit) exercises the sub-byte bit unpacking.
    let q12 = f32s(field("quant12"));
    let expect_q12 = [0.0, 25.006105, 50.01221, 74.9939, 100.0];
    for (i, (&got, &want)) in q12.iter().zip(&expect_q12).enumerate() {
        assert!(
            (got - want).abs() < 1e-3,
            "{name} quant12[{i}] = {got}, want {want}"
        );
    }
}

#[test]
fn reads_reduced_precision_uncompressed() {
    check("rntuple_realprec_uncompressed.root");
}

#[test]
fn reads_reduced_precision_zstd() {
    check("rntuple_realprec_zstd.root");
}
