//! Linear-algebra objects: `TVectorD`, `TMatrixD`, and the symmetric
//! `TMatrixDSym` (a covariance shape). oxiroot reads the ROOT-C++-written
//! `linalg.root` fixture — `TMatrixDSym` stores only its upper triangle on disk —
//! its serialized bytes match ROOT's key-for-key, and a single object round-trips
//! through its own `write_root`. ROOT C++ and uproot read oxiroot's output
//! (checked out of band via the interop harness).

use std::path::PathBuf;

use oxiroot_io_core::{object_bytes_any, Compression, FileReader, ReadRoot, WriteRoot};
use oxiroot_linalg::{TMatrixD, TMatrixDSym, TVectorD};

fn fixture() -> FileReader {
    FileReader::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/linalg.root"))
        .expect("open fixture")
}

#[test]
fn reads_root_written_linalg() {
    let f = fixture();

    let v = TVectorD::read_root(&f, "v").unwrap();
    assert_eq!(v.elements(), &[1.5, 2.5, 3.5]);

    let m = TMatrixD::read_root(&f, "m").unwrap();
    assert_eq!((m.rows(), m.cols()), (2, 3));
    assert_eq!(m.elements(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(m.get(0, 0), 1.0);
    assert_eq!(m.get(1, 2), 6.0);

    let s = TMatrixDSym::read_root(&f, "s").unwrap();
    assert_eq!(s.dim(), 3);
    // Symmetric: the off-diagonal is reflected.
    assert_eq!(s.get(0, 1), 0.5);
    assert_eq!(s.get(1, 0), 0.5);
    assert_eq!(s.get(2, 2), 3.0);
    assert_eq!(s.elements(), &[1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0]);
}

#[test]
fn matrix_bytes_are_byte_exact_against_root() {
    // oxiroot's serialized matrix/vector bytes must equal ROOT's, key-for-key.
    let f = fixture();
    let cases: [(&str, &dyn WriteRoot); 3] = [
        ("v", &TVectorD::new(vec![1.5, 2.5, 3.5])),
        (
            "m",
            &TMatrixD::new(2, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap(),
        ),
        (
            "s",
            &TMatrixDSym::new(3, vec![1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0]).unwrap(),
        ),
    ];
    for (name, obj) in cases {
        let (_class, root) = object_bytes_any(&f, name).unwrap();
        assert_eq!(
            obj.to_root_bytes(),
            root,
            "object bytes differ for {name:?}"
        );
    }
}

#[test]
fn single_object_write_root_round_trips() {
    // Exercise the io-core `WriteRoot::write_root` default + the matrix's
    // `streamer_classes` (which describes its own class from scratch).
    let out = std::env::temp_dir().join("oxiroot_linalg_single.root");
    TMatrixDSym::new(3, vec![1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0])
        .unwrap()
        .named("cov")
        .write_root(&out, Compression::None)
        .unwrap();

    let f = FileReader::open(&out).unwrap();
    let s = TMatrixDSym::read_root(&f, "cov").unwrap();
    assert_eq!(s.dim(), 3);
    assert_eq!(s.get(0, 1), 0.5);
    assert_eq!(s.get(1, 0), 0.5);
    assert_eq!(s.elements(), &[1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0]);
    let _ = std::fs::remove_file(&out);
}
