//! ROOT compression framing (the 9-byte block header) plus codec backends.
//!
//! ROOT stores compressed payloads as a sequence of independently-compressed
//! blocks; see [`header`] for the block layout. This crate is a leaf dependency
//! of the rest of the workspace and owns the (eventually feature-gated) choice
//! of codec backends.
//!
//! Status: block framing, uncompressed passthrough, and Zstd + zlib **decode**
//! are implemented and validated against real ROOT output. LZ4/LZMA decode and
//! all encoders arrive in later milestones.

mod codec;
mod header;
pub use header::{Algorithm, BlockHeader, HDR_SIZE, MAX_CHUNK_SIZE};

use std::fmt;

/// Errors raised while (de)compressing ROOT payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressError {
    /// The input was shorter than required to read a header or block payload.
    Truncated { needed: usize, available: usize },
    /// Decompression produced a different number of bytes than expected.
    SizeMismatch { expected: usize, got: usize },
    /// A block uses an algorithm whose codec is not compiled in yet.
    CodecUnavailable(Algorithm),
    /// The underlying codec reported an error.
    Codec(String),
}

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressError::Truncated { needed, available } => {
                write!(
                    f,
                    "truncated compressed data: needed {needed} bytes, {available} available"
                )
            }
            CompressError::SizeMismatch { expected, got } => {
                write!(
                    f,
                    "decompressed size mismatch: expected {expected} bytes, got {got}"
                )
            }
            CompressError::CodecUnavailable(algo) => {
                write!(f, "no codec available for algorithm {algo:?}")
            }
            CompressError::Codec(msg) => write!(f, "codec error: {msg}"),
        }
    }
}

impl std::error::Error for CompressError {}

/// Build the ROOT compression-settings integer: `algorithm * 100 + level`.
///
/// `algorithm_code` follows ROOT's `ECompressionAlgorithm` enum (zlib = 1,
/// LZMA = 2, LZ4 = 4, Zstd = 5); `level` is 1..=9.
pub fn compression_settings(algorithm_code: u8, level: u8) -> u32 {
    algorithm_code as u32 * 100 + level as u32
}

/// Split a compression-settings integer back into `(algorithm_code, level)`.
pub fn split_settings(settings: u32) -> (u8, u8) {
    ((settings / 100) as u8, (settings % 100) as u8)
}

/// Decompress `src` into exactly `uncompressed_len` bytes.
///
/// When `src.len() == uncompressed_len` the payload is taken to be stored
/// uncompressed (no block header) and returned verbatim. Otherwise `src` is
/// parsed as a sequence of ROOT compression blocks until `uncompressed_len`
/// bytes have been produced.
pub fn decompress(src: &[u8], uncompressed_len: usize) -> Result<Vec<u8>, CompressError> {
    if src.len() == uncompressed_len {
        return Ok(src.to_vec());
    }

    let mut out = Vec::with_capacity(uncompressed_len);
    let mut cur = src;
    while out.len() < uncompressed_len {
        let hdr = BlockHeader::parse(cur)?;
        let payload_end = HDR_SIZE + hdr.compressed_size as usize;
        if cur.len() < payload_end {
            return Err(CompressError::Truncated {
                needed: payload_end,
                available: cur.len(),
            });
        }
        let payload = &cur[HDR_SIZE..payload_end];
        out.extend_from_slice(&decompress_block(&hdr, payload)?);
        cur = &cur[payload_end..];
    }

    if out.len() != uncompressed_len {
        return Err(CompressError::SizeMismatch {
            expected: uncompressed_len,
            got: out.len(),
        });
    }
    Ok(out)
}

fn decompress_block(hdr: &BlockHeader, payload: &[u8]) -> Result<Vec<u8>, CompressError> {
    let n = hdr.uncompressed_size as usize;
    let out = match hdr.algorithm() {
        Algorithm::Zstd => codec::zstd_decode(payload, n)?,
        Algorithm::Zlib => codec::zlib_decode(payload)?,
        // LZ4 (with its xxhash64 prefix) and LZMA decode arrive in a later step.
        algo => return Err(CompressError::CodecUnavailable(algo)),
    };
    if out.len() != n {
        return Err(CompressError::SizeMismatch {
            expected: n,
            got: out.len(),
        });
    }
    Ok(out)
}

/// Compress `src` according to `settings` (`algorithm * 100 + level`).
///
/// `settings == 0` means "store uncompressed": the input is returned unchanged
/// (the caller stores it without a block header). Otherwise the data is encoded
/// into ROOT compression blocks. Only Zstd encoding (algorithm 5) is supported.
pub fn compress(src: &[u8], settings: u32) -> Result<Vec<u8>, CompressError> {
    if settings == 0 {
        return Ok(src.to_vec());
    }
    let (algorithm, _level) = split_settings(settings);
    if algorithm != 5 {
        return Err(CompressError::Codec(format!(
            "encoding algorithm {algorithm} is not supported (only Zstd)"
        )));
    }

    let mut out = Vec::new();
    for chunk in src.chunks(MAX_CHUNK_SIZE.max(1)) {
        let frame = codec::zstd_encode(chunk);
        if frame.len() > MAX_CHUNK_SIZE {
            return Err(CompressError::Codec(
                "compressed block exceeds 24-bit size".into(),
            ));
        }
        BlockHeader {
            tag: *b"ZS",
            method: 1,
            compressed_size: frame.len() as u32,
            uncompressed_size: chunk.len() as u32,
        }
        .write(&mut out);
        out.extend_from_slice(&frame);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        // Zstd level 5 -> 505.
        assert_eq!(compression_settings(5, 5), 505);
        assert_eq!(split_settings(505), (5, 5));
        assert_eq!(split_settings(101), (1, 1)); // zlib level 1
    }

    #[test]
    fn decompress_uncompressed_passthrough() {
        let data = b"hello root".to_vec();
        let out = decompress(&data, data.len()).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn compress_uncompressed_passthrough() {
        let data = b"hello root".to_vec();
        assert_eq!(compress(&data, 0).unwrap(), data);
    }

    #[test]
    fn compress_zstd_round_trips() {
        // Zstd level 5 -> settings 505. Compress, then decompress back.
        let data = b"the quick brown fox jumps over the lazy dog. ".repeat(40);
        let compressed = compress(&data, 505).unwrap();
        // The first block must carry the Zstd tag.
        assert_eq!(&compressed[0..2], b"ZS");
        let out = decompress(&compressed, data.len()).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn decompress_reports_unavailable_codec() {
        // LZ4 decode is not wired up yet, so a block tagged "L4" must report the
        // codec as unavailable rather than silently mis-decoding.
        let mut buf = Vec::new();
        BlockHeader {
            tag: *b"L4",
            method: 1,
            compressed_size: 4,
            uncompressed_size: 8,
        }
        .write(&mut buf);
        buf.extend_from_slice(&[0, 1, 2, 3]);
        assert!(matches!(
            decompress(&buf, 8),
            Err(CompressError::CodecUnavailable(Algorithm::Lz4))
        ));
    }

    #[test]
    fn decompress_zlib_block_round_trips() {
        // Build a ROOT "ZL" block by hand (zlib stream behind the 9-byte header)
        // and confirm we decode it back to the original bytes.
        let original = b"the quick brown fox jumps over the lazy dog. ".repeat(20);
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&original, 6);

        let mut block = Vec::new();
        BlockHeader {
            tag: *b"ZL",
            method: 8, // Z_DEFLATED
            compressed_size: compressed.len() as u32,
            uncompressed_size: original.len() as u32,
        }
        .write(&mut block);
        block.extend_from_slice(&compressed);

        let out = decompress(&block, original.len()).unwrap();
        assert_eq!(out, original);
    }
}
