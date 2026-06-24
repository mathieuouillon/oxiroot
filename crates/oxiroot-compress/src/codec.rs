//! Codec backends for ROOT compression blocks.
//!
//! Decode-only for the read path. Zstd is decoded with the pure-Rust `ruzstd`,
//! zlib with `miniz_oxide`. LZ4 and LZMA decode, plus all encoders, arrive in
//! later milestones.

use std::io::Read;

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
