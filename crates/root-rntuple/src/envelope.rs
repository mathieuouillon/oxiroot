//! RNTuple envelopes and frames (little-endian).
//!
//! An envelope is a 64-bit word packing a 16-bit type id (low bits) and a
//! 48-bit uncompressed length (high bits), followed by the payload and a
//! trailing XXH3-64 checksum over everything before it.

use root_io_core::buffer::RBuffer;
use root_io_core::error::{Error, Result};

/// Schema (fields, columns) envelope.
pub const ENVELOPE_HEADER: u16 = 0x01;
/// Cluster groups + schema extension + page-list locators.
pub const ENVELOPE_FOOTER: u16 = 0x02;
/// Cluster summaries + page locations.
pub const ENVELOPE_PAGELIST: u16 = 0x03;

/// A parsed envelope: its type and payload (the bytes between the type/length
/// word and the trailing checksum).
#[derive(Debug, Clone, Copy)]
pub struct Envelope<'a> {
    /// Envelope type id ([`ENVELOPE_HEADER`], …).
    pub type_id: u16,
    /// Payload bytes (excluding the 8-byte word and the 8-byte checksum).
    pub payload: &'a [u8],
}

/// Parse a fully-decompressed envelope, verifying its XXH3-64 checksum.
pub fn read_envelope(bytes: &[u8]) -> Result<Envelope<'_>> {
    if bytes.len() < 16 {
        return Err(Error::Format(format!(
            "envelope too short: {} bytes",
            bytes.len()
        )));
    }
    let word = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let type_id = (word & 0xFFFF) as u16;
    let length = (word >> 16) as usize;

    if length != bytes.len() {
        return Err(Error::Format(format!(
            "envelope length {length} does not match decompressed size {}",
            bytes.len()
        )));
    }

    let checksum_pos = length - 8;
    let stored = u64::from_le_bytes(bytes[checksum_pos..length].try_into().unwrap());
    let computed = xxhash_rust::xxh3::xxh3_64(&bytes[..checksum_pos]);
    if computed != stored {
        return Err(Error::Format(format!(
            "envelope checksum mismatch: computed {computed:#018x}, stored {stored:#018x}"
        )));
    }

    Ok(Envelope {
        type_id,
        payload: &bytes[8..checksum_pos],
    })
}

/// A frame within an envelope payload: a record (single set of fields) or a
/// list (a count of homogeneous items). The reader uses `size` to skip.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Whether this is a list frame (vs a record frame).
    pub is_list: bool,
    /// Number of items, for list frames.
    pub n_items: u32,
    /// Absolute buffer offset of the frame's inner payload (after the header).
    pub inner_start: usize,
    /// Absolute buffer offset one past the end of the frame.
    pub end: usize,
}

/// Read a frame header at the cursor, leaving the cursor at the inner payload.
/// `size` is a signed 64-bit count of total frame bytes; a negative value marks
/// a list frame and is followed by a 32-bit item count.
pub fn read_frame(r: &mut RBuffer) -> Result<Frame> {
    let start = r.pos();
    let raw = r.le_i64()?;
    let is_list = raw < 0;
    let size = raw.unsigned_abs() as usize;
    if size < 8 {
        return Err(Error::Format(format!("frame size {size} too small")));
    }
    let n_items = if is_list { r.le_u32()? } else { 0 };
    Ok(Frame {
        is_list,
        n_items,
        inner_start: r.pos(),
        end: start + size,
    })
}
