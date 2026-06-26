//! Hardening: corrupt/truncated histogram files must yield `Err`, never panic.
//!
//! Every reader is pointed at every fixture (a wrong class fails fast), so a
//! crafted file can't drive any of them out of bounds. Large fixtures are
//! sampled with a stride to keep the sweep fast while still hitting each
//! structural field.

use std::path::PathBuf;

use oxiroot_hist::{
    read_tefficiency, read_tgraph, read_th1, read_th1d, read_th2d, read_th2poly, read_th3d,
    read_thnsparse, read_tprofile, read_tprofile2d, read_tprofile3d,
};
use oxiroot_io_core::RFile;

/// Fixtures spanning every histogram/graph layout, with one key name each.
const FIXTURES: &[(&str, &str)] = &[
    ("th1d_uncompressed.root", "h1"),
    ("th2d_uncompressed.root", "h2"),
    ("tprofile_uncompressed.root", "p"),
    ("graphs.root", "ge"),
    ("tefficiency.root", "eff"),
    ("tprofile2d.root", "p2"),
    ("th2poly.root", "hp"),
    ("thnsparse.root", "hs"),
];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("read fixture")
}

/// Try every reader; the point is that none panics regardless of the bytes.
fn poke_hist(f: &RFile, name: &str) {
    let _ = read_th1(f, name);
    let _ = read_th1d(f, name);
    let _ = read_th2d(f, name);
    let _ = read_th3d(f, name);
    let _ = read_tprofile(f, name);
    let _ = read_tprofile2d(f, name);
    let _ = read_tprofile3d(f, name);
    let _ = read_tefficiency(f, name);
    let _ = read_thnsparse(f, name);
    let _ = read_th2poly(f, name);
    let _ = read_tgraph(f, name);
}

/// Stride that keeps each fixture to roughly `samples` probes regardless of size.
fn stride(len: usize, samples: usize) -> usize {
    (len / samples).max(1)
}

#[test]
fn histogram_byte_flips_never_panic() {
    for (fix, key) in FIXTURES {
        let data = fixture(fix);
        let step = stride(data.len(), 3000);
        for i in (0..data.len()).step_by(step) {
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

#[test]
fn histogram_truncations_never_panic() {
    for (fix, key) in FIXTURES {
        let data = fixture(fix);
        let step = stride(data.len(), 2000);
        for len in (0..=data.len()).step_by(step) {
            if let Ok(f) = RFile::from_bytes(data[..len].to_vec()) {
                poke_hist(&f, key);
            }
        }
    }
}
