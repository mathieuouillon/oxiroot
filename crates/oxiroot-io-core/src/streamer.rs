//! Generic ROOT object-streaming helpers shared by class readers.
//!
//! ROOT serializes most objects with a `{byte-count, version}` header (see
//! [`RBuffer::read_version`]); the byte count lets a reader consume the members
//! it understands and then seek to the object's end, ignoring members added by
//! later schema versions. `TObject` is the notable exception — it is written
//! with a 2-byte version and no byte count.

use crate::buffer::{RBuffer, WBuffer};
use crate::error::Result;

/// `TObject::kIsReferenced` — when set in `fBits`, a 2-byte process-id follows.
const K_IS_REFERENCED: u32 = 0x10;

/// A streamed `TObject` base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TObjectHeader {
    /// `TObject` streamer version.
    pub version: u16,
    /// Unique id (usually 0).
    pub unique_id: u32,
    /// Status bits.
    pub bits: u32,
}

/// Read a `TObject` base: a 2-byte version, `fUniqueID`, `fBits`, and a 2-byte
/// process-id reference iff `kIsReferenced` is set. Leaves the cursor just past
/// the `TObject` data.
pub fn read_tobject(r: &mut RBuffer) -> Result<TObjectHeader> {
    let vh = r.read_version()?; // version only, no byte count
    let unique_id = r.be_u32()?;
    let bits = r.be_u32()?;
    if bits & K_IS_REFERENCED != 0 {
        let _pidf = r.be_u16()?;
    }
    Ok(TObjectHeader {
        version: vh.version,
        unique_id,
        bits,
    })
}

/// A streamed `TNamed` (name + title).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TNamed {
    /// Object name (`fName`).
    pub name: String,
    /// Object title (`fTitle`).
    pub title: String,
}

/// Read a `TNamed` base (`TObject` + `fName` + `fTitle`), then seek to the end
/// of the `TNamed` record as given by its byte count.
pub fn read_tnamed(r: &mut RBuffer) -> Result<TNamed> {
    let vh = r.read_version()?;
    let _obj = read_tobject(r)?;
    let name = r.string()?;
    let title = r.string()?;
    if let Some(end) = vh.end {
        r.seek(end)?;
    }
    Ok(TNamed { name, title })
}

/// Read a versioned object's header and seek straight to its end, skipping the
/// payload entirely. Returns the version. Errors if the object had no byte
/// count (and therefore no known end to skip to).
pub fn skip_versioned(r: &mut RBuffer) -> Result<u16> {
    let vh = r.read_version()?;
    match vh.end {
        Some(end) => {
            r.seek(end)?;
            Ok(vh.version)
        }
        None => Err(crate::error::Error::Unsupported(
            "cannot skip a versioned object that carries no byte count".into(),
        )),
    }
}

/// Write a `TObject` base: a 2-byte version, `fUniqueID = 0`, and `fBits`.
/// (No byte count, matching ROOT's `TObject::Streamer`.)
pub fn write_tobject(w: &mut WBuffer, bits: u32) {
    w.be_u16(1); // TObject version
    w.be_u32(0); // fUniqueID
    w.be_u32(bits); // fBits
}

/// Write an embedded object the way ROOT's `WriteObjectAny` does: a byte count,
/// `kNewClassTag`, the NUL-terminated class name, then `body` (the object's own
/// streamed bytes, as produced by [`WriteRoot`](crate::WriteRoot)).
pub fn write_object_any(w: &mut WBuffer, class: &str, body: &[u8]) {
    let bc = w.reserve(4);
    w.be_u32(0xFFFF_FFFF); // kNewClassTag
    w.bytes(class.as_bytes());
    w.u8(0);
    w.bytes(body);
    let inner = (w.len() - w.patch_offset(bc) - 4) as u32;
    w.patch_be_u32(bc, inner | crate::buffer::K_BYTE_COUNT_MASK);
}

/// Write a `TNamed` base (a byte-counted `TObject` + `fName` + `fTitle`).
pub fn write_tnamed(w: &mut WBuffer, bits: u32, name: &str, title: &str) {
    let tok = w.begin_object(1); // TNamed version 1
    write_tobject(w, bits);
    w.string(name);
    w.string(title);
    w.end_object(tok);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::WBuffer;

    #[test]
    fn reads_tnamed_with_tobject() {
        // Build: TNamed{ TObject(v1, uid=0, bits=0), fName="hi", fTitle="" }.
        let mut inner = WBuffer::new();
        // TObject: version (no byte count) + uniqueID + bits.
        inner.be_u16(1);
        inner.be_u32(0);
        inner.be_u32(0x0300_0000);
        inner.string("hi");
        inner.string("");
        let inner = inner.into_vec();

        let mut w = WBuffer::new();
        let tok = w.begin_object(1); // TNamed version 1 + byte count
        w.bytes(&inner);
        w.end_object(tok);
        let bytes = w.into_vec();

        let mut r = RBuffer::new(&bytes);
        let named = read_tnamed(&mut r).unwrap();
        assert_eq!(named.name, "hi");
        assert_eq!(named.title, "");
        assert_eq!(r.pos(), bytes.len());
    }
}
