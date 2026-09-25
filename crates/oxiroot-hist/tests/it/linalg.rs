//! The linear-algebra objects themselves live in `oxiroot-linalg`; this test
//! only covers the `oxiroot-hist` `FileWriter` writing them alongside
//! other objects — the multi-object path collects their `TStreamerInfo` through
//! hist's central `streamer_info_for` (which delegates the matrix classes to
//! `oxiroot-linalg`). The per-type read/write and byte-exactness are tested in
//! `oxiroot-linalg`.

use oxiroot_hist::{FileWriter, ReadRoot};
use oxiroot_io_core::{Compression, FileReader};
use oxiroot_linalg::{Matrix, SymMatrix, Vector};

#[test]
fn root_file_writes_linalg_objects() {
    let out = std::env::temp_dir().join("oxiroot_hist_linalg_rt.root");
    FileWriter::create(&out)
        .add(&Vector::new(vec![1.5, 2.5, 3.5]).named("v"))
        .add(
            &Matrix::new(2, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
                .unwrap()
                .named("m"),
        )
        .add(
            &SymMatrix::new(3, vec![1.0, 0.5, 0.0, 0.5, 2.0, 0.0, 0.0, 0.0, 3.0])
                .unwrap()
                .named("s"),
        )
        .write(Compression::None)
        .unwrap();

    let f = FileReader::open(&out).unwrap();
    assert_eq!(
        Vector::read_root(&f, "v").unwrap().elements(),
        &[1.5, 2.5, 3.5]
    );
    let m = Matrix::read_root(&f, "m").unwrap();
    assert_eq!(m.get(0, 0), 1.0);
    assert_eq!(m.get(1, 2), 6.0);
    let s = SymMatrix::read_root(&f, "s").unwrap();
    assert_eq!(s.get(0, 1), 0.5);
    assert_eq!(s.get(1, 0), 0.5);
    let _ = std::fs::remove_file(&out);
}
