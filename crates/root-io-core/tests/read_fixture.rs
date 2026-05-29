//! M1 integration tests: read the committed ROOT fixture's container metadata
//! and enumerate its keys, plus a hand-crafted 64-bit-header round-trip.

use std::path::PathBuf;

use root_io_core::buffer::{RBuffer, WBuffer};
use root_io_core::{FileHeader, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn reads_th1d_uncompressed_container() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");

    // Header: small (32-bit) form, first record at byte 100.
    let h = f.header();
    assert!(!h.is_big(), "fixture should be small-format");
    assert_eq!(h.begin, 100);
    assert_eq!(h.units, 4);
    // Self-consistency: streamer-info record sits right before the free list.
    assert_eq!(h.seek_info + h.nbytes_info as u64, h.seek_free);

    // Exactly one key, the TH1D named "h1" (matches fixtures/golden/*.json).
    let keys: Vec<(&str, &str)> = f
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert_eq!(keys, vec![("h1", "TH1D")]);

    let k = f.key("h1").expect("key h1");
    assert_eq!(k.cycle, 1);
    assert!(!k.is_deleted());
    // The fixture is uncompressed: on-disk payload length == uncompressed length.
    assert!(k.is_uncompressed());
    assert_eq!(k.payload_len(), k.obj_len as usize);
    // The key's payload range must lie within the file.
    assert!(k.payload_range().end <= f.data().len());
}

#[test]
fn free_list_is_consistent() {
    let f = RFile::open(fixture("th1d_uncompressed.root")).expect("open fixture");
    let free = f.free_segments().expect("read free list");
    assert_eq!(free.len() as u32, f.header().nfree);
    for seg in &free {
        assert!(seg.first <= seg.last);
    }
}

#[test]
fn parses_hand_crafted_big_header() {
    // Build a 64-bit ("big") file header by hand and check the wide fields.
    let mut w = WBuffer::new();
    w.bytes(b"root");
    w.be_u32(1_000_004); // big-format version
    w.be_u32(100); // fBEGIN (always 4 bytes)
    w.be_u64(5_000_000_000); // fEND (8 bytes, > 2 GiB)
    w.be_u64(4_000_000_000); // fSeekFree (8 bytes)
    w.be_u32(60); // fNbytesFree
    w.be_u32(1); // nfree
    w.be_u32(80); // fNbytesName
    w.u8(8); // fUnits
    w.be_u32(505); // fCompress (Zstd level 5)
    w.be_u64(2_000_000_000); // fSeekInfo (8 bytes)
    w.be_u32(300); // fNbytesInfo
    w.be_u16(1); // UUID version
    w.bytes(&[0xAB; 16]); // UUID bytes
    let bytes = w.into_vec();

    let mut r = RBuffer::new(&bytes);
    let h = FileHeader::read(&mut r).expect("parse big header");
    assert!(h.is_big());
    assert_eq!(h.begin, 100);
    assert_eq!(h.end, 5_000_000_000);
    assert_eq!(h.seek_free, 4_000_000_000);
    assert_eq!(h.seek_info, 2_000_000_000);
    assert_eq!(h.units, 8);
    assert_eq!(h.compress, 505);
    assert_eq!(h.uuid.version, 1);
    assert_eq!(h.uuid.bytes, [0xAB; 16]);
}

#[test]
fn decompresses_zstd_object_matching_uncompressed() {
    let unc = RFile::open(fixture("th1d_uncompressed.root")).expect("open uncompressed");
    let zst = RFile::open(fixture("th1d_zstd.root")).expect("open zstd");

    let unc_key = unc.key("h1").expect("uncompressed h1");
    let zst_key = zst.key("h1").expect("zstd h1");

    // Same object, so the uncompressed object length must match.
    assert_eq!(unc_key.obj_len, zst_key.obj_len);

    // The uncompressed fixture stores the object verbatim.
    assert!(unc_key.is_uncompressed());
    let unc_obj = &unc.data()[unc_key.payload_range()];

    // The zstd fixture stores it compressed; decode it via the real ROOT block
    // framing + ruzstd and require a byte-for-byte match with the plain object.
    assert!(!zst_key.is_uncompressed());
    let zst_payload = &zst.data()[zst_key.payload_range()];
    let decoded =
        root_compress::decompress(zst_payload, zst_key.obj_len as usize).expect("zstd decode");

    assert_eq!(decoded.len(), zst_key.obj_len as usize);
    assert_eq!(
        decoded, unc_obj,
        "decompressed Zstd object must match the uncompressed one"
    );
}

#[test]
fn rejects_non_root_magic() {
    let err = RFile::from_bytes(b"NOPE....".to_vec()).unwrap_err();
    assert!(matches!(err, root_io_core::Error::BadMagic(_)));
}
