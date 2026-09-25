//! The free-segment list (`TFree`).
//!
//! ROOT tracks reusable gaps as a list of `[first, last]` byte ranges stored in
//! a `Key`-wrapped record at `fSeekFree`. Each `TFree` entry uses 64-bit
//! offsets when its version exceeds 1000. Parsing is informational for reading;
//! the allocator that consumes/produces this list arrives with `update`-mode
//! writing (M6).

use super::header::FileHeader;
use super::key::Key;
use super::source::ByteSource;
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
/// Fetches only the `[fSeekFree, fNbytesFree]` record, so it stays lazy over a
/// remote source.
pub fn read_free(source: &dyn ByteSource, header: &FileHeader) -> Result<Vec<FreeSegment>> {
    if header.seek_free == 0 || header.nfree == 0 {
        return Ok(Vec::new());
    }
    let avail = source.len().saturating_sub(header.seek_free);
    let want = if header.nbytes_free > 0 {
        u64::from(header.nbytes_free).min(avail)
    } else {
        avail
    };
    let win = source.read_at(header.seek_free, want as usize)?;
    let mut r = RBuffer::new(&win);

    // The free list is wrapped in a Key; its payload is `nfree` TFree records.
    let _wrapper = Key::read(&mut r)?;

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
