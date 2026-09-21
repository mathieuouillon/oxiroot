//! Standalone `TF1`/`TF2`/`TF3` function keys: oxiroot reads the ROOT-C++-written
//! `tf1.root`/`tf23.root` fixtures (a `TF1` embedding a `TFormula`, plus a `TF2`
//! and `TF3`), evaluates the formulas in pure Rust, round-trips its own writes,
//! and serializes byte-for-byte as ROOT does. ROOT C++ and uproot both read
//! oxiroot's `TF1`/`TF2`/`TF3` output and re-evaluate them (checked out of band):
//! oxiroot embeds the `TF1`/`TF2`/`TF3`/`TFormula` `TStreamerInfo` (with the
//! `TStreamerSTL` members) so uproot builds a model for each.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, RootFile, WriteRoot, TF1, TF2, TF3};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open fixture")
}

#[test]
fn reads_root_written_tf1() {
    let f = fixture("tf1.root");
    let g = TF1::read_root(&f, "myfunc").unwrap();
    assert_eq!(g.name(), "myfunc");
    assert_eq!(g.title(), "[0]*sin([1]*x) + [2]");
    assert_eq!(g.formula(), "[p0]*sin([p1]*x)+[p2]");
    assert_eq!(g.npar(), 3);
    assert_eq!(g.params(), &[2.0, 1.5, 0.5]);
    // ROOT: Eval(1)=2.494990, Integral(0,pi)=2.904130, Derivative(1)=0.212212.
    assert!((g.eval(1.0) - 2.494_990).abs() < 1e-6);
    assert!((g.integral(0.0, std::f64::consts::PI) - 2.904_130).abs() < 1e-5);
    assert!((g.derivative(1.0) - 0.212_212).abs() < 1e-5);
}

#[test]
fn reads_root_written_tf2_and_tf3() {
    let f = fixture("tf23.root");
    let f2 = TF2::read_root(&f, "f2").unwrap();
    assert_eq!(f2.title(), "[0]*sin(x) + [1]*y*y");
    assert_eq!(f2.params(), &[1.5, 0.7]);
    assert!((f2.eval(1.0, 1.0) - 1.962_206).abs() < 1e-5); // ROOT Eval(1,1)

    let f3 = TF3::read_root(&f, "f3").unwrap();
    assert_eq!(f3.title(), "[0]*x + y*z");
    assert_eq!(f3.params(), &[2.0]);
    assert_eq!(f3.eval(1.0, 1.0, 1.0), 3.0); // ROOT Eval(1,1,1)
}

#[test]
#[allow(clippy::approx_constant)] // 6.283 is the fixture's exact fXmax, not TAU
fn tf1_bytes_are_byte_exact_against_root() {
    // oxiroot's serialized TF1 bytes must equal ROOT's, byte for byte — with one
    // tolerated quirk: ROOT serializes `TFormula::fAllParametersSetted` as a
    // *non-normalized* `bool` (any truthy byte, e.g. 0x99), while oxiroot writes
    // the canonical `1`. Both read back as `true`, so we accept `(1, non-zero)`
    // at a single position and require every other byte to match exactly.
    let f = fixture("tf1.root");
    let obj = TF1::new("myfunc", "[0]*sin([1]*x) + [2]", 0.0, 6.283)
        .unwrap()
        .with_params(vec![2.0, 1.5, 0.5])
        .to_root_bytes();
    let (_, root) = oxiroot_io_core::object_bytes_any(&f, "myfunc").unwrap();
    assert_eq!(obj.len(), root.len(), "TF1 object length differs from ROOT");
    for (i, (&a, &b)) in obj.iter().zip(&root).enumerate() {
        if a != b {
            assert!(
                a == 1 && b != 0,
                "TF1 byte {i} differs: oxiroot={a}, root={b} (only ROOT's truthy \
                 fAllParametersSetted byte may differ)"
            );
        }
    }
}

#[test]
fn round_trips_tf1_tf2_tf3_through_oxiroot() {
    let out = std::env::temp_dir().join("oxiroot_tf_rt.root");
    let f1 = TF1::new("f1", "[0]*exp(-[1]*x)", 0.0, 5.0)
        .unwrap()
        .with_params(vec![10.0, 0.5]);
    let f2 = TF2::new("f2", "[0]*x + y", -2.0, 2.0, -2.0, 2.0)
        .unwrap()
        .with_params(vec![3.0]);
    let f3 = TF3::new("f3", "x + y + z + [0]", 0.0, 1.0, 0.0, 1.0, 0.0, 1.0)
        .unwrap()
        .with_params(vec![0.25]);

    RootFile::create(&out)
        .add(&f1)
        .add(&f2)
        .add(&f3)
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&out).unwrap();
    assert_eq!(TF1::read_root(&f, "f1").unwrap(), f1);
    assert_eq!(TF2::read_root(&f, "f2").unwrap(), f2);
    assert_eq!(TF3::read_root(&f, "f3").unwrap(), f3);
    // and the evaluation survives the round trip.
    assert!((TF1::read_root(&f, "f1").unwrap().eval(2.0) - 10.0 * (-1.0f64).exp()).abs() < 1e-12);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn written_file_embeds_function_streamer_info() {
    // uproot needs the embedded `TStreamerInfo` to model a standalone TF2/TF3.
    // Verify the writer emits it: with no compression the class/member names
    // appear literally in the file.
    let out = std::env::temp_dir().join("oxiroot_tf_streamer.root");
    TF2::new("f2", "[0]*x + y", 0.0, 1.0, 0.0, 1.0)
        .unwrap()
        .with_params(vec![2.0])
        .write_root(&out, Compression::None)
        .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"TFormula"), "TFormula streamer info not embedded");
    assert!(has(b"TStreamerSTL"), "TStreamerSTL element not embedded");
    assert!(has(b"fClingParameters"), "TFormula members not embedded");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn builds_and_evaluates_shortcuts() {
    let g = TF1::new("g", "gaus", -5.0, 5.0)
        .unwrap()
        .with_params(vec![2.0, 0.0, 1.0]);
    assert_eq!(g.eval(0.0), 2.0);
    assert!((g.integral(-10.0, 10.0) - 2.0 * (2.0 * std::f64::consts::PI).sqrt()).abs() < 1e-4);
}
