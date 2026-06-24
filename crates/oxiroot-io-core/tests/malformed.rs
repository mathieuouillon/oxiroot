//! Hardening regression tests: malformed, truncated, and byte-flipped input must
//! return `Err` (or `Ok` with valid data) — never panic or allocate unboundedly.
//! A panic anywhere below fails the test.

use std::path::PathBuf;

use oxiroot_io_core::{RFile, TDatime, TKey};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    std::fs::read(p).expect("read fixture")
}

/// Exercise every read path on an opened file (each must be panic-free).
fn poke(f: &RFile) {
    for k in f.keys() {
        let _ = k.payload(f.data());
    }
    let _ = f.streamer_registry();
    let _ = f.streamer_info_object();
    for name in ["h1", "h", "ntpl", "p", ""] {
        let _ = f.object_in("missing", name);
    }
}

#[test]
fn from_bytes_never_panics_on_garbage() {
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0u8; 3],
        b"root".to_vec(),
        b"root\x00\x00\x00\x64\xff\xff\xff\xff".to_vec(),
        (0u8..=255).cycle().take(4096).collect(),
        vec![0xffu8; 4096],
    ];
    for c in cases {
        if let Ok(f) = RFile::from_bytes(c) {
            poke(&f);
        }
    }
}

#[test]
fn truncation_never_panics() {
    let data = fixture("th1d_uncompressed.root");
    for len in 0..=data.len() {
        if let Ok(f) = RFile::from_bytes(data[..len].to_vec()) {
            poke(&f);
        }
    }
}

#[test]
fn single_byte_flips_never_panic() {
    // Flip every byte of a real file to 0x00 and 0xFF and re-parse + read.
    let data = fixture("th1d_uncompressed.root");
    for i in 0..data.len() {
        for v in [0x00u8, 0xff] {
            let mut c = data.clone();
            c[i] = v;
            if let Ok(f) = RFile::from_bytes(c) {
                poke(&f);
            }
        }
    }
}

#[test]
fn tkey_payload_rejects_inconsistent_headers() {
    let base = TKey {
        nbytes: 100,
        version: 4,
        obj_len: 50,
        datime: TDatime(0),
        key_len: 32,
        cycle: 1,
        seek_key: 0,
        seek_pdir: 0,
        class_name: "TH1D".into(),
        name: "h".into(),
        title: String::new(),
    };
    let data = vec![0u8; 64];

    // key_len > |nbytes| would underflow payload_len — must Err, not panic.
    let bad_len = TKey {
        key_len: 200,
        ..base.clone()
    };
    assert!(bad_len.payload(&data).is_err());

    // seek_key + key_len + payload runs past the buffer — must Err.
    let past_end = TKey {
        seek_key: 1_000_000,
        ..base.clone()
    };
    assert!(past_end.payload(&data).is_err());

    // A consistent key inside the buffer succeeds.
    let ok = TKey {
        nbytes: 40,
        key_len: 32,
        seek_key: 0,
        ..base
    };
    assert_eq!(ok.payload(&data).unwrap().len(), 8); // 40 - 32
}

#[test]
fn decompress_huge_declared_length_errors_without_ooming() {
    // A tiny source claiming a 1 TiB uncompressed length must Err, not attempt a
    // multi-GB allocation (the initial reservation is capped).
    assert!(oxiroot_compress::decompress(&[1, 2, 3], 1usize << 40).is_err());
    // Crafted block headers with absurd sizes also error rather than panic/OOM.
    let crafted = b"ZS\x01\x03\x00\x00\xff\xff\xff".to_vec();
    assert!(oxiroot_compress::decompress(&crafted, 1usize << 40).is_err());
}
