//! M2 integration test: reconstruct a `TH2D` from a real ROOT fixture and
//! check it against the committed golden values.

use std::path::PathBuf;

use oxiroot_hist::read_th2d;
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_th2d_uncompressed() {
    let f = RFile::open(fixture("th2d_uncompressed.root")).expect("open fixture");
    let h = read_th2d(&f, "h2").expect("read TH2D");

    assert_eq!(h.name, "h2");
    assert_eq!(h.nx(), 3);
    assert_eq!(h.ny(), 2);
    assert_eq!(h.ncells(), 20); // (3+2) * (2+2)

    assert_eq!(h.xaxis.xmin, 0.0);
    assert_eq!(h.xaxis.xmax, 3.0);
    assert_eq!(h.yaxis.xmin, 0.0);
    assert_eq!(h.yaxis.xmax, 2.0);

    // values[ix][iy] from fixtures/golden/th2d_uncompressed.json.
    let expected = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
    assert_eq!(h.values(), expected, "bin grid (x rows, y cols)");

    assert_eq!(h.entries, 21.0);
    assert_eq!(h.tsumw, 21.0);

    // Edges.
    assert_eq!(h.xaxis.edges(), vec![0.0, 1.0, 2.0, 3.0]);
    assert_eq!(h.yaxis.edges(), vec![0.0, 1.0, 2.0]);
}
