//! Hardening: corrupt/truncated histogram files must yield `Err`, never panic.

use std::path::PathBuf;

use oxiroot_hist::{read_th1, read_th1d, read_th2d, read_th3d, read_tprofile};
use oxiroot_io_core::RFile;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("read fixture")
}

fn poke_hist(f: &RFile, name: &str) {
    let _ = read_th1(f, name);
    let _ = read_th1d(f, name);
    let _ = read_th2d(f, name);
    let _ = read_th3d(f, name);
    let _ = read_tprofile(f, name);
}

#[test]
fn histogram_byte_flips_never_panic() {
    for (fix, key) in [
        ("th1d_uncompressed.root", "h1"),
        ("th2d_uncompressed.root", "h2"),
        ("tprofile_uncompressed.root", "p"),
    ] {
        let data = fixture(fix);
        for i in 0..data.len() {
            for v in [0x00u8, 0xff] {
                let mut c = data.clone();
                c[i] = v;
                if let Ok(f) = RFile::from_bytes(c) {
                    poke_hist(&f, key);
                }
            }
        }
    }
}
