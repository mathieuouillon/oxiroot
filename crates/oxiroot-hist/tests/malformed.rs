//! Hardening: corrupt/truncated histogram files must yield `Err`, never panic.
//!
//! Every reader is pointed at every fixture (a wrong class fails fast), so a
//! crafted file can't drive any of them out of bounds. Large fixtures are
//! sampled with a stride to keep the sweep fast while still hitting each
//! structural field.

use std::path::PathBuf;

use oxiroot_hist::{
    ReadRoot, TEfficiency, TGraph, TH2Poly, THnSparse, TProfile, TProfile2D, TProfile3D, TH1, TH2,
    TH3,
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
    let _ = TH1::read_root(f, name);
    let _ = TH2::read_root(f, name);
    let _ = TH3::read_root(f, name);
    let _ = TProfile::read_root(f, name);
    let _ = TProfile2D::read_root(f, name);
    let _ = TProfile3D::read_root(f, name);
    let _ = TEfficiency::read_root(f, name);
    let _ = THnSparse::read_root(f, name);
    let _ = TH2Poly::read_root(f, name);
    let _ = TGraph::read_root(f, name);
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
