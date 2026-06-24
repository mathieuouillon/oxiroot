//! Generative properties for the compression framing: `compress` then
//! `decompress` is the identity for any input, and `decompress` never panics on
//! arbitrary bytes / declared lengths.

use oxiroot_compress::{compress, decompress};
use proptest::prelude::*;

proptest! {
    // Zstd-encode arbitrary bytes (including empty and >1-block inputs), then
    // decode back to exactly the original.
    #[test]
    fn zstd_round_trips(data in proptest::collection::vec(any::<u8>(), 0..5000)) {
        let compressed = compress(&data, 505).expect("compress");
        let out = decompress(&compressed, data.len()).expect("decompress");
        prop_assert_eq!(out, data);
    }

    // Uncompressed passthrough (settings 0) round-trips for any input.
    #[test]
    fn uncompressed_round_trips(data in proptest::collection::vec(any::<u8>(), 0..5000)) {
        let stored = compress(&data, 0).expect("store");
        prop_assert_eq!(decompress(&stored, data.len()).expect("read"), data);
    }

    // Arbitrary bytes with an arbitrary declared length must yield Ok or Err,
    // never a panic or a runaway allocation.
    #[test]
    fn decompress_arbitrary_never_panics(
        data in proptest::collection::vec(any::<u8>(), 0..2000),
        len in 0usize..(8 << 20),
    ) {
        let _ = decompress(&data, len);
    }
}
