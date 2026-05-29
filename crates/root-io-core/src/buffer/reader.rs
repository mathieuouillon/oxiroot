//! Big-/little-endian read cursor for ROOT on-disk structures.
//!
//! ROOT's classic/TKey/streamer payloads are big-endian (network order); the
//! RNTuple anchor is big-endian while RNTuple envelope/page payloads are
//! little-endian. Endianness is therefore explicit in every accessor name
//! rather than carried as buffer state.

use crate::error::{Error, Result};

/// Bit in the 32-bit byte-count word that marks a byte count as present
/// (ROOT's `kByteCountMask`).
pub const K_BYTE_COUNT_MASK: u32 = 0x4000_0000;

/// Header written ahead of a streamed object by ROOT's `TBuffer::WriteVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionHeader {
    /// Class version (`fVersion`).
    pub version: u16,
    /// Byte count of the payload following the count word, when one was present.
    pub byte_count: Option<u32>,
    /// Buffer offset of the count word (the start of the header).
    pub start: usize,
    /// Buffer offset one past the end of the object, when a byte count was present.
    pub end: Option<usize>,
}

/// Read cursor over a borrowed ROOT buffer.
#[derive(Debug, Clone)]
pub struct RBuffer<'a> {
    data: &'a [u8],
    pos: usize,
}

macro_rules! num_reader {
    ($(#[$doc:meta])* $name:ident, $ty:ty, $from:ident, $n:literal) => {
        $(#[$doc])*
        pub fn $name(&mut self) -> Result<$ty> {
            Ok(<$ty>::$from(self.array::<$n>()?))
        }
    };
}

impl<'a> RBuffer<'a> {
    /// Create a cursor positioned at the start of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current cursor offset.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Number of bytes left to read.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Whether the cursor is at or past the end of the buffer.
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// The full underlying buffer (ignoring the cursor).
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Move the cursor to an absolute offset.
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::UnexpectedEof {
                needed: pos,
                available: self.data.len(),
            });
        }
        self.pos = pos;
        Ok(())
    }

    /// Advance the cursor by `n` bytes.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// Borrow the next `n` bytes and advance the cursor.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.take(n)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or(Error::UnexpectedEof {
                needed: n,
                available: self.remaining(),
            })?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let slice = self.take(N)?;
        let mut arr = [0u8; N];
        arr.copy_from_slice(slice);
        Ok(arr)
    }

    /// Read a single byte.
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    /// Read a single signed byte.
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.array::<1>()?[0] as i8)
    }

    num_reader!(/// Read a big-endian `u16`.
        be_u16, u16, from_be_bytes, 2);
    num_reader!(/// Read a big-endian `u32`.
        be_u32, u32, from_be_bytes, 4);
    num_reader!(/// Read a big-endian `u64`.
        be_u64, u64, from_be_bytes, 8);
    num_reader!(/// Read a big-endian `i16`.
        be_i16, i16, from_be_bytes, 2);
    num_reader!(/// Read a big-endian `i32`.
        be_i32, i32, from_be_bytes, 4);
    num_reader!(/// Read a big-endian `i64`.
        be_i64, i64, from_be_bytes, 8);
    num_reader!(/// Read a big-endian `f32`.
        be_f32, f32, from_be_bytes, 4);
    num_reader!(/// Read a big-endian `f64`.
        be_f64, f64, from_be_bytes, 8);

    num_reader!(/// Read a little-endian `u16`.
        le_u16, u16, from_le_bytes, 2);
    num_reader!(/// Read a little-endian `u32`.
        le_u32, u32, from_le_bytes, 4);
    num_reader!(/// Read a little-endian `u64`.
        le_u64, u64, from_le_bytes, 8);
    num_reader!(/// Read a little-endian `i16`.
        le_i16, i16, from_le_bytes, 2);
    num_reader!(/// Read a little-endian `i32`.
        le_i32, i32, from_le_bytes, 4);
    num_reader!(/// Read a little-endian `i64`.
        le_i64, i64, from_le_bytes, 8);
    num_reader!(/// Read a little-endian `f32`.
        le_f32, f32, from_le_bytes, 4);
    num_reader!(/// Read a little-endian `f64`.
        le_f64, f64, from_le_bytes, 8);

    /// Read a ROOT-encoded string: a 1-byte length, or `0xFF` followed by a
    /// big-endian `u32` length, then that many UTF-8 bytes.
    pub fn string(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        let n = if n == 255 { self.be_u32()? as usize } else { n };
        let bytes = self.take(n)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
    }

    /// Read a streamed-object version header (`{fByteCount, fVersion}`).
    ///
    /// If the leading 32-bit word has [`K_BYTE_COUNT_MASK`] set, a byte count
    /// and 16-bit version follow ROOT's convention; otherwise the leading two
    /// bytes are the version and no byte count is present (the cursor is left
    /// just past the version).
    pub fn read_version(&mut self) -> Result<VersionHeader> {
        let start = self.pos;
        let word = self.be_u32()?;
        if word & K_BYTE_COUNT_MASK != 0 {
            let byte_count = word & !K_BYTE_COUNT_MASK;
            let version = self.be_u16()?;
            Ok(VersionHeader {
                version,
                byte_count: Some(byte_count),
                start,
                end: Some(start + 4 + byte_count as usize),
            })
        } else {
            // No byte count: rewind, the first two bytes were the version.
            self.pos = start;
            let version = self.be_u16()?;
            Ok(VersionHeader {
                version,
                byte_count: None,
                start,
                end: None,
            })
        }
    }

    /// Verify the cursor sits exactly at the end implied by a [`VersionHeader`].
    ///
    /// A no-op when the header carried no byte count.
    pub fn check_byte_count(&self, vh: &VersionHeader) -> Result<()> {
        if let Some(end) = vh.end {
            if self.pos != end {
                return Err(Error::ByteCountMismatch {
                    expected: end,
                    got: self.pos,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_big_endian_integers() {
        let mut r = RBuffer::new(&[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(r.be_u16().unwrap(), 0x1234);
        assert_eq!(r.be_u16().unwrap(), 0x5678);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn reads_little_endian_integers() {
        let mut r = RBuffer::new(&[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(r.le_u32().unwrap(), 0x1234_5678);
    }

    #[test]
    fn eof_is_reported() {
        let mut r = RBuffer::new(&[0x00]);
        assert!(matches!(
            r.be_u32(),
            Err(Error::UnexpectedEof {
                needed: 4,
                available: 1
            })
        ));
    }

    #[test]
    fn reads_short_string() {
        let mut buf = vec![5u8];
        buf.extend_from_slice(b"hello");
        let mut r = RBuffer::new(&buf);
        assert_eq!(r.string().unwrap(), "hello");
    }
}
