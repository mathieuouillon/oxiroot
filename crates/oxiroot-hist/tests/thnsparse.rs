//! THnSparse: read a ROOT-written fixture + self-round-trip.

use std::path::PathBuf;

use oxiroot_hist::{read_thnsparse, write_thnsparse_file, SparseBin, THnSparse};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn sorted(mut b: Vec<SparseBin>) -> Vec<SparseBin> {
    b.sort_by(|a, c| a.coords.cmp(&c.coords));
    b
}

#[test]
fn reads_root_written_thnsparse() {
    let f = RFile::open(fixture("thnsparse.root")).expect("open");
    let h = read_thnsparse(&f, "hs").expect("read");
    assert_eq!(h.ndim(), 2);
    assert_eq!(h.entries, 4.0);
    assert_eq!(
        sorted(h.bins),
        vec![
            SparseBin {
                coords: vec![1, 1],
                content: 1.0
            },
            SparseBin {
                coords: vec![2, 2],
                content: 3.0
            },
        ]
    );
}

#[test]
fn thnsparse_round_trips() {
    let mut h = THnSparse::new("hs", "", &[(2, 0.0, 2.0), (2, 0.0, 2.0)]);
    h.fill(&[0.5, 0.5]);
    h.fill(&[1.5, 1.5]);
    h.fill(&[1.5, 1.5]);
    h.fill(&[1.5, 1.5]);
    let out = PathBuf::from("/tmp/oxiroot_thnsparse.root");
    write_thnsparse_file(&out, &h, Compression::None).expect("write");
    let f = RFile::open(&out).expect("reopen");
    let back = read_thnsparse(&f, "hs").unwrap();
    assert_eq!(sorted(back.bins), sorted(h.bins.clone()));
    assert_eq!(back.entries, 4.0);
}
