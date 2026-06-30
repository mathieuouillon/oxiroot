//! Linear-algebra objects: `TVectorD`, `TMatrixD`, and the symmetric
//! `TMatrixDSym` (a covariance shape). oxiroot reads the ROOT-C++-written
//! `linalg.root` fixture — `TMatrixDSym` stores only its upper triangle on disk —
//! and round-trips its own writes; ROOT C++ and uproot read oxiroot's output
//! (checked out of band).

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, RootFile, TMatrixD, TMatrixDSym, TVectorD};
use oxiroot_io_core::{Compression, RFile};

fn fixture() -> RFile {
    RFile::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/linalg.root"))
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
fn round_trips_linalg_through_oxiroot() {
    let out = std::env::temp_dir().join("oxiroot_linalg_rt.root");
    RootFile::create(&out)
        .add(&TVectorD::new(vec![1.5, 2.5, 3.5]).named("v"))
        .add(&TMatrixD::new(2, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).named("m"))
        .add(&TMatrixDSym::new(3, vec![1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0]).named("s"))
        .write(Compression::None)
        .unwrap();

    let f = RFile::open(&out).unwrap();
    assert_eq!(
        TVectorD::read_root(&f, "v").unwrap().elements(),
        &[1.5, 2.5, 3.5]
    );
    let m = TMatrixD::read_root(&f, "m").unwrap();
    assert_eq!(m.get(0, 0), 1.0);
    assert_eq!(m.get(1, 2), 6.0);
    let s = TMatrixDSym::read_root(&f, "s").unwrap();
    assert_eq!(s.get(0, 1), 0.5);
    assert_eq!(s.get(1, 0), 0.5);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn matrix_bytes_are_byte_exact_against_root() {
    // oxiroot's serialized matrix/vector bytes must equal ROOT's, key-for-key.
    use oxiroot_hist::WriteRoot;
    let f = fixture();
    let cases: [(&str, &dyn WriteRoot); 3] = [
        ("v", &TVectorD::new(vec![1.5, 2.5, 3.5])),
        (
            "m",
            &TMatrixD::new(2, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        ),
        (
            "s",
            &TMatrixDSym::new(3, vec![1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0]),
        ),
    ];
    for (name, obj) in cases {
        let key = f.key(name).unwrap();
        let root =
            oxiroot_compress::decompress(key.payload(f.data()).unwrap(), key.obj_len as usize)
                .unwrap();
        assert_eq!(
            obj.to_root_bytes(),
            root,
            "object bytes differ for {name:?}"
        );
    }
}
