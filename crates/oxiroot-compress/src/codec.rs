//! Codec backends for ROOT compression blocks.
//!
//! All pure-Rust: Zstd via `ruzstd`, zlib via `miniz_oxide`, LZ4 via `lz4_flex`
//! (block format), and LZMA via `lzma-rs` (an XZ stream). Decode is available
//! for every algorithm ROOT writes except the legacy `CS`; encode is available
//! for Zstd, zlib, and LZ4.

use std::io::Read;

use xxhash_rust::xxh64::xxh64;

use crate::CompressError;

/// Decode a single Zstd-compressed block payload (a standard Zstd frame).
pub(crate) fn zstd_decode(
    payload: &[u8],
    uncompressed_size: usize,
) -> Result<Vec<u8>, CompressError> {
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(payload)
        .map_err(|e| CompressError::Codec(format!("zstd: {e:?}")))?;
    // `uncompressed_size` is the block header's (untrusted) declared size. Cap the
    // speculative reservation so a forged header can't force a large allocation;
    // the caller verifies the produced length matches afterward.
    let mut out = Vec::with_capacity(uncompressed_size.min(crate::MAX_CHUNK_SIZE));
    decoder
        .read_to_end(&mut out)
        .map_err(|e| CompressError::Codec(format!("zstd: {e}")))?;
    Ok(out)
}

/// Decode a single zlib-compressed block payload (a standard zlib stream).
pub(crate) fn zlib_decode(payload: &[u8]) -> Result<Vec<u8>, CompressError> {
    miniz_oxide::inflate::decompress_to_vec_zlib(payload)
        .map_err(|e| CompressError::Codec(format!("zlib: {e:?}")))
}

/// Decode a ROOT LZ4 block payload: an 8-byte big-endian XXH64 checksum (over
/// the *compressed* bytes, seed 0) followed by an LZ4 block. `uncompressed_size`
/// is the block header's declared output size (always ≤ 16 MiB, so it safely
/// bounds the codec's allocation).
pub(crate) fn lz4_decode(
    payload: &[u8],
    uncompressed_size: usize,
) -> Result<Vec<u8>, CompressError> {
    if payload.len() < 8 {
        return Err(CompressError::Truncated {
            needed: 8,
            available: payload.len(),
        });
    }
    let (checksum, data) = payload.split_at(8);
    let stored = u64::from_be_bytes(checksum.try_into().expect("8 bytes"));
    if xxh64(data, 0) != stored {
        return Err(CompressError::Codec("lz4: xxh64 checksum mismatch".into()));
    }
    lz4_flex::block::decompress(data, uncompressed_size)
        .map_err(|e| CompressError::Codec(format!("lz4: {e}")))
}

/// Decode a ROOT LZMA block payload (a complete XZ stream).
pub(crate) fn lzma_decode(
    payload: &[u8],
    uncompressed_size: usize,
) -> Result<Vec<u8>, CompressError> {
    let mut out = Vec::with_capacity(uncompressed_size.min(crate::MAX_CHUNK_SIZE));
    let mut input = payload;
    lzma_rs::xz_decompress(&mut input, &mut out)
        .map_err(|e| CompressError::Codec(format!("lzma: {e}")))?;
    Ok(out)
}

/// Encode `data` as a single zlib stream — the payload of a ROOT `ZL` block.
/// `level` is ROOT's 1..=9, clamped to the `miniz_oxide` 0..=10 range.
pub(crate) fn zlib_encode(data: &[u8], level: u8) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, level.min(10))
}

/// Encode `data` as a ROOT LZ4 block payload: the 8-byte big-endian XXH64
/// checksum of the compressed bytes, then the LZ4 block. Matches ROOT's framing
/// so official ROOT and uproot read it back.
pub(crate) fn lz4_encode(data: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(data);
    let mut out = Vec::with_capacity(8 + compressed.len());
    out.extend_from_slice(&xxh64(&compressed, 0).to_be_bytes());
    out.extend_from_slice(&compressed);
    out
}

/// Encode `data` as a single standard Zstd frame (pure-Rust `ruzstd`). The frame
/// is what ROOT stores after a block's 9-byte header.
///
/// `_level` is the requested ROOT level (1..=9). The pure-Rust `ruzstd` encoder
/// exposes only one compressing level (`Fastest`), so the numeric level does not
/// change the ratio here — the output is always valid Zstd that ROOT reads back
/// correctly, just not tuned per level. (A higher-ratio backend would be a build
/// option, not an interop concern.)
pub(crate) fn zstd_encode(data: &[u8], _level: u8) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
}
