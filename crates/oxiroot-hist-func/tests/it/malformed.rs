//! Hardening: corrupt/truncated files must make the function readers return
//! `Err`, never panic. Every reader is pointed at every fixture, histogram and
//! graph files included, so a wrong class must fail cleanly too.

use std::path::PathBuf;

use oxiroot_hist::ReadRoot;
use oxiroot_hist_func::{TF1, TF2, TF3};
use oxiroot_io_core::FileReader;

/// Fixtures spanning the function layouts plus a few other classes, with one
/// key name each.
const FIXTURES: &[(&str, &str)] = &[
    ("tf1.root", "myfunc"),
    ("tf23.root", "f2"),
    ("tf23.root", "f3"),
    ("graphs.root", "ge"),
    ("th1d_uncompressed.root", "h1"),
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
fn poke(f: &FileReader, name: &str) {
    let _ = TF1::read_root(f, name);
    let _ = TF2::read_root(f, name);
    let _ = TF3::read_root(f, name);
    let _ = TF1::read_root_in(f, "", name);
}

/// Stride that keeps each fixture to roughly `samples` probes regardless of size.
fn stride(len: usize, samples: usize) -> usize {
    (len / samples).max(1)
}

#[test]
fn function_byte_flips_never_panic() {
    for (fix, key) in FIXTURES {
        let data = fixture(fix);
        let step = stride(data.len(), 3000);
        for i in (0..data.len()).step_by(step) {
            for v in [0x00u8, 0xff] {
                let mut c = data.clone();
                c[i] = v;
                if let Ok(f) = FileReader::from_bytes(c) {
                    poke(&f, key);
                }
            }
        }
    }
}

#[test]
fn function_truncations_never_panic() {
    for (fix, key) in FIXTURES {
        let data = fixture(fix);
        let step = stride(data.len(), 2000);
        for len in (0..=data.len()).step_by(step) {
            if let Ok(f) = FileReader::from_bytes(data[..len].to_vec()) {
                poke(&f, key);
            }
        }
    }
}
