//! Hardening: corrupt/truncated RNTuple bytes must yield `Err`, never panic.
//! A panic anywhere below fails the test.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{concat_ntuples, RNTuple};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("read fixture")
}

fn poke_rntuple(f: &RFile) {
    if let Ok(ntpl) = RNTuple::open(f, "ntpl") {
        let names: Vec<String> = ntpl.field_names().iter().map(|s| s.to_string()).collect();
        for name in names {
            let _ = ntpl.read_field(f, &name);
            // The cluster-bounded prefix read shares the decode path; exercise it
            // (including the cluster-slicing / view) on the malformed bytes too.
            let _ = ntpl.read_field_prefix(f, &name, 2);
        }
        // Run the (possibly corrupt) ntuple through the `hadd`-style merge —
        // exercises field concatenation + rebuild on malformed input.
        let _ = concat_ntuples("ntpl", &[(f, &ntpl)]);
    }
}

#[test]
fn rntuple_byte_flips_never_panic() {
    for fix in [
        "rntuple_scalars_uncompressed.root",
        "rntuple_scalars_zstd.root",
        "rntuple_multicluster_vec.root",
    ] {
        let data = fixture(fix);
        for i in 0..data.len() {
            for v in [0x00u8, 0xff] {
                let mut c = data.clone();
                c[i] = v;
                if let Ok(f) = RFile::from_bytes(c) {
                    poke_rntuple(&f);
                }
            }
        }
    }
}

#[test]
fn rntuple_truncation_never_panics() {
    let data = fixture("rntuple_scalars_uncompressed.root");
    for len in 0..=data.len() {
        if let Ok(f) = RFile::from_bytes(data[..len].to_vec()) {
            poke_rntuple(&f);
        }
    }
}
