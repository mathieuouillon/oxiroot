//! The free-segment list (`TFree`).
//!
//! ROOT tracks reusable gaps as a list of `[first, last]` byte ranges stored in
//! a `TKey`-wrapped record at `fSeekFree`. Each `TFree` entry uses 64-bit
//! offsets when its version exceeds 1000. Parsing is informational for reading;
//! the allocator that consumes/produces this list arrives with `update`-mode
//! writing (M6).

use super::header::FileHeader;
use super::key::TKey;
use crate::buffer::RBuffer;
use crate::error::Result;

/// `TFree` version above which offsets are 64-bit.
const FREE_BIG_VERSION: i16 = 1000;

/// A single free byte range `[first, last]` (inclusive of `first`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeSegment {
    /// First free byte offset.
    pub first: u64,
    /// One past the last free byte (ROOT stores `fLast`).
    pub last: u64,
}

/// Read the file's free-segment list. Returns an empty list when there is none.
pub fn read_free(data: &[u8], header: &FileHeader) -> Result<Vec<FreeSegment>> {
    if header.seek_free == 0 || header.nfree == 0 {
        return Ok(Vec::new());
    }
    let mut r = RBuffer::new(data);
    r.seek(header.seek_free as usize)?;

    // The free list is wrapped in a TKey; its payload is `nfree` TFree records.
    let _wrapper = TKey::read(&mut r)?;

    let mut segments = Vec::with_capacity(header.nfree as usize);
    for _ in 0..header.nfree {
        let version = r.be_i16()?;
        let (first, last) = if version > FREE_BIG_VERSION {
            (r.be_u64()?, r.be_u64()?)
        } else {
            (r.be_u32()? as u64, r.be_u32()? as u64)
        };
        segments.push(FreeSegment { first, last });
    }
    Ok(segments)
}
