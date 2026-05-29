//! ROOT's 9-byte compression block header.
//!
//! Every compressed ROOT payload — TKey object data and RNTuple pages alike —
//! is a sequence of independently-compressed blocks, each prefixed by this
//! 9-byte header. A single block holds at most [`MAX_CHUNK_SIZE`] uncompressed
//! bytes; larger payloads are split across consecutive blocks.
//!
//! Layout (offsets within the 9-byte header):
//! - `[0..2]` two ASCII characters identifying the algorithm (e.g. `b"ZS"`)
//! - `[2]` method / version byte (algorithm-specific)
//! - `[3..6]` compressed payload size, 24-bit little-endian
//! - `[6..9]` uncompressed payload size, 24-bit little-endian

use crate::CompressError;

/// Size of the compression block header in bytes.
pub const HDR_SIZE: usize = 9;

/// Maximum uncompressed payload carried by a single block (`0xFFFFFF`, ~16 MiB).
pub const MAX_CHUNK_SIZE: usize = 0xFF_FFFF;

/// A ROOT compression algorithm, identified by the 2-character block tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// zlib / DEFLATE (`b"ZL"`).
    Zlib,
    /// LZMA (`b"XZ"`).
    Lzma,
    /// Legacy ROOT algorithm (`b"CS"`).
    OldRoot,
    /// LZ4 (`b"L4"`).
    Lz4,
    /// Zstandard (`b"ZS"`).
    Zstd,
    /// Unrecognized tag.
    Unknown([u8; 2]),
}

impl Algorithm {
    /// Identify the algorithm from its 2-character block tag.
    pub fn from_tag(tag: [u8; 2]) -> Self {
        match &tag {
            b"ZL" => Algorithm::Zlib,
            b"XZ" => Algorithm::Lzma,
            b"CS" => Algorithm::OldRoot,
            b"L4" => Algorithm::Lz4,
            b"ZS" => Algorithm::Zstd,
            _ => Algorithm::Unknown(tag),
        }
    }

    /// The 2-character tag for this algorithm.
    pub fn tag(self) -> [u8; 2] {
        match self {
            Algorithm::Zlib => *b"ZL",
            Algorithm::Lzma => *b"XZ",
            Algorithm::OldRoot => *b"CS",
            Algorithm::Lz4 => *b"L4",
            Algorithm::Zstd => *b"ZS",
            Algorithm::Unknown(t) => t,
        }
    }
}

/// A parsed ROOT compression block header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHeader {
    /// 2-character algorithm tag.
    pub tag: [u8; 2],
    /// Algorithm-specific method / version byte.
    pub method: u8,
    /// Size of the compressed payload immediately following this header.
    pub compressed_size: u32,
    /// Size of the payload once decompressed.
    pub uncompressed_size: u32,
}

impl BlockHeader {
    /// The algorithm this block was compressed with.
    pub fn algorithm(&self) -> Algorithm {
        Algorithm::from_tag(self.tag)
    }

    /// Parse a header from the first [`HDR_SIZE`] bytes of `buf`.
    pub fn parse(buf: &[u8]) -> Result<Self, CompressError> {
        if buf.len() < HDR_SIZE {
            return Err(CompressError::Truncated {
                needed: HDR_SIZE,
                available: buf.len(),
            });
        }
        Ok(BlockHeader {
            tag: [buf[0], buf[1]],
            method: buf[2],
            compressed_size: u24_le(&buf[3..6]),
            uncompressed_size: u24_le(&buf[6..9]),
        })
    }

    /// Append the 9-byte header to `out`.
    pub fn write(&self, out: &mut Vec<u8>) {
        out.push(self.tag[0]);
        out.push(self.tag[1]);
        out.push(self.method);
        put_u24_le(out, self.compressed_size);
        put_u24_le(out, self.uncompressed_size);
    }
}

fn u24_le(b: &[u8]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16)
}

fn put_u24_le(out: &mut Vec<u8>, v: u32) {
    out.push((v & 0xFF) as u8);
    out.push(((v >> 8) & 0xFF) as u8);
    out.push(((v >> 16) & 0xFF) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_tag_round_trips() {
        for algo in [
            Algorithm::Zlib,
            Algorithm::Lzma,
            Algorithm::OldRoot,
            Algorithm::Lz4,
            Algorithm::Zstd,
        ] {
            assert_eq!(Algorithm::from_tag(algo.tag()), algo);
        }
        assert_eq!(Algorithm::from_tag(*b"??"), Algorithm::Unknown(*b"??"));
    }

    #[test]
    fn header_round_trips() {
        let hdr = BlockHeader {
            tag: *b"ZS",
            method: 1,
            compressed_size: 0x0001_2345,
            uncompressed_size: 0x0000_ABCD,
        };
        let mut buf = Vec::new();
        hdr.write(&mut buf);
        assert_eq!(buf.len(), HDR_SIZE);
        // 24-bit little-endian sizes.
        assert_eq!(&buf[3..6], &[0x45, 0x23, 0x01]);
        assert_eq!(&buf[6..9], &[0xCD, 0xAB, 0x00]);
        assert_eq!(BlockHeader::parse(&buf).unwrap(), hdr);
    }

    #[test]
    fn parse_rejects_short_buffer() {
        assert!(matches!(
            BlockHeader::parse(&[0u8; 4]),
            Err(CompressError::Truncated {
                needed: HDR_SIZE,
                available: 4
            })
        ));
    }

    #[test]
    fn zstd_tag_is_zs() {
        assert_eq!(Algorithm::Zstd.tag(), *b"ZS");
    }
}
