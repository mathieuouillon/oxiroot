//! Hardening regression tests: malformed, truncated, and byte-flipped input must
//! return `Err` (or `Ok` with valid data) — never panic or allocate unboundedly.
//! A panic anywhere below fails the test.

use std::path::PathBuf;

use oxiroot_io_core::buffer::WBuffer;
use oxiroot_io_core::{write_key_header_fmt, RFile, TDatime, TKey};

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

/// A minimal, parseable TFile whose root directory holds a single big-format
/// (version 1004, 64-bit seeks) `TDirectory` key named "d" with `fSeekKey` near
/// `u64::MAX`. `RFile::from_bytes` accepts it; navigating into "d" must reject
/// the offset rather than overflow.
fn file_with_hostile_big_directory_key() -> Vec<u8> {
    let mut w = WBuffer::new();

    // File header (100 bytes, small format). fNbytesName = 0 puts the root
    // directory record at fBEGIN (100); the fields not on the parse path are 0.
    w.bytes(b"root");
    w.be_u32(62400); // fVersion (small, < 1_000_000)
    w.be_u32(100); // fBEGIN
    w.be_u32(0); // fEND
    w.be_u32(0); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    w.be_u32(0); // fNbytesName -> dir record at 100 + 0
    w.u8(4); // fUnits
    w.be_u32(0); // fCompress
    w.be_u32(0); // fSeekInfo
    w.be_u32(0); // fNbytesInfo
    w.be_u16(1); // fUUID version
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // Root TDirectory record at offset 100 (small format).
    w.be_i16(5); // version (small, <= 1000)
    w.be_u32(0); // fDatimeC
    w.be_u32(0); // fDatimeM
    w.be_i32(0); // fNbytesKeys
    w.be_i32(0); // fNbytesName
    w.be_u32(100); // fSeekDir
    w.be_u32(0); // fSeekParent
    let p_seek_keys = w.reserve(4); // fSeekKeys (patched below)

    // Key list: a wrapper key, the count, then the one hostile entry.
    let keylist = w.len() as u32;
    w.patch_be_u32(p_seek_keys, keylist);
    write_key_header_fmt(
        &mut w,
        "TFile",
        "f",
        "",
        0,
        0,
        keylist as u64,
        100,
        1,
        false,
    );
    w.be_i32(1); // nkeys

    // The hostile key: a big-format TDirectory whose fSeekKey is near u64::MAX,
    // so `fSeekKey + fKeyLen` overflows usize.
    write_key_header_fmt(
        &mut w,
        "TDirectory",
        "d",
        "",
        0,
        0,
        u64::MAX - 8,
        100,
        1,
        true,
    );

    w.into_vec()
}

#[test]
fn subdir_rejects_overflowing_big_directory_key() {
    let f = RFile::from_bytes(file_with_hostile_big_directory_key())
        .expect("the crafted container parses");

    // Sanity: the key we parse back really is the hostile big-format one.
    let k = f
        .keys()
        .iter()
        .find(|k| k.name == "d")
        .expect("dir key present");
    assert_eq!(k.class_name, "TDirectory");
    assert!(k.seek_key > u64::MAX - 16, "fSeekKey near u64::MAX");

    // Navigating into it must Err — not overflow-panic (debug) or wrap to a
    // bogus small offset (release) in RFile::subdir -> TKey::payload_start.
    assert!(
        f.subdir("d").is_err(),
        "subdir must reject the overflowing directory key"
    );
    // object_in() reaches the same offset computation via subdir().
    assert!(f.object_in("d", "x").is_err());
}
