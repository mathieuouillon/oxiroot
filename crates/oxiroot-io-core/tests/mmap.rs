//! The memory-mapped read path (`mmap` feature) must yield the same parsed
//! container as the in-memory path.
#![cfg(feature = "mmap")]

use std::path::PathBuf;

use oxiroot_io_core::RFile;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn open_mmap_matches_open() {
    let path = fixture("th1d_uncompressed.root");
    let owned = RFile::open(&path).expect("open");
    let mapped = RFile::open_mmap(&path).expect("open_mmap");

    assert_eq!(mapped.data(), owned.data(), "same bytes");
    let names = |f: &RFile| -> Vec<String> { f.keys().iter().map(|k| k.name.clone()).collect() };
    assert_eq!(names(&mapped), names(&owned), "same keys");
}
