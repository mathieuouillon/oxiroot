//! RNTuple envelopes and frames (little-endian).
//!
//! An envelope is a 64-bit word packing a 16-bit type id (low bits) and a
//! 48-bit uncompressed length (high bits), followed by the payload and a
//! trailing XXH3-64 checksum over everything before it.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};

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
/// list (a count of homogeneous items). The reader uses `end` to skip.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Number of items, for list frames (0 for a record frame).
    pub n_items: u32,
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
    let inner_start = r.pos();
    // The frame must end within the envelope and at or past its own header, so
    // that the `r.seek(frame.end)` skip and the item loop stay in bounds even
    // for a corrupt or truncated size.
    let end = start
        .checked_add(size)
        .filter(|&e| e >= inner_start && e <= r.len())
        .ok_or_else(|| {
            Error::Format(format!("frame size {size} runs past the envelope payload"))
        })?;
    Ok(Frame { n_items, end })
}

/// A standard (type-0) on-disk locator: a compressed byte count and a file
/// offset from the start of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locator {
    /// Compressed size of the located bytes.
    pub size: u32,
    /// File offset of the located bytes.
    pub offset: u64,
}

/// Read a type-0 locator. A negative size flags a non-standard locator, which
/// is not supported here.
pub fn read_locator(r: &mut RBuffer) -> Result<Locator> {
    let size = r.le_i32()?;
    if size < 0 {
        return Err(Error::Format(
            "non-standard RNTuple locator is unsupported".into(),
        ));
    }
    let offset = r.le_u64()?;
    Ok(Locator {
        size: size as u32,
        offset,
    })
}

/// Read an RNTuple string: a 32-bit little-endian length, then the UTF-8 bytes.
pub(crate) fn read_string(r: &mut RBuffer) -> Result<String> {
    let n = r.le_u32()? as usize;
    let bytes = r.bytes(n)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
}

/// Consume the feature-flag fields (continuing while the signed value is
/// negative, per the spec's extension convention).
pub(crate) fn read_feature_flags(r: &mut RBuffer) -> Result<()> {
    while r.le_i64()? < 0 {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::read_envelope;
    use crate::NtupleReader;
    use oxiroot_io_core::FileReader;

    #[test]
    fn a_root_written_rntuple_has_typed_envelopes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/rntuple_scalars_uncompressed.root");
        let f = FileReader::open(path).unwrap();
        let ntpl = NtupleReader::open(&f, "ntpl").unwrap();
        let header = read_envelope(ntpl.header_envelope()).unwrap();
        assert_eq!(header.type_id, 0x01);
        // The payload is the envelope minus its 8-byte preamble and checksum.
        assert_eq!(header.payload.len(), ntpl.header_envelope().len() - 16);
        assert_eq!(read_envelope(ntpl.footer_envelope()).unwrap().type_id, 0x02);
    }
}
