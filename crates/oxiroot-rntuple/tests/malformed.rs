//! Hardening: corrupt/truncated RNTuple bytes must yield `Err`, never panic.
//! A panic anywhere below fails the test.

use std::path::PathBuf;

use oxiroot_io_core::{BytesSource, RFile};
use oxiroot_rntuple::{concat_ntuples, read_column, ColumnType, Locator, PageInfo, RNTuple};

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

#[test]
fn read_column_rejects_bits_type_mismatch() {
    // A bare in-memory byte source (not a full ROOT file): the column decoder
    // only needs `read_at`, so it can be unit-tested directly.
    let src = BytesSource::new(vec![0u8; 16]);
    let pages = vec![PageInfo {
        num_elements: 4,
        has_checksum: false,
        locator: Locator {
            size: 16,
            offset: 0,
        },
    }];

    // Int32 declared with 64 bits would slice 8-byte chunks into a 4-byte type
    // (try_into().unwrap() panic) — the guard rejects it first.
    assert!(read_column(&src, ColumnType::Int32, 64, &pages, None).is_err());

    // Bit declared with 0 bits would size the page to 0 and index out of range.
    let bit_pages = vec![PageInfo {
        num_elements: 8,
        has_checksum: false,
        locator: Locator { size: 1, offset: 0 },
    }];
    assert!(read_column(&src, ColumnType::Bit, 0, &bit_pages, None).is_err());

    // The matching width still decodes.
    assert!(read_column(&src, ColumnType::Int32, 32, &pages, None).is_ok());
}
