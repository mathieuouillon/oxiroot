//! Opening an RNTuple from a ROOT file: locate the anchor key, parse and
//! verify the anchor, and load the header and footer envelopes.

use root_io_core::error::{Error, Result};
use root_io_core::RFile;

use crate::anchor::{RNTupleAnchor, ANCHOR_CLASS};
use crate::envelope::{read_envelope, ENVELOPE_FOOTER, ENVELOPE_HEADER};
use crate::header::Header;

/// An opened RNTuple: the verified anchor, the parsed schema (header), and the
/// decompressed footer envelope. Page-list and data decoding build on this.
pub struct RNTuple {
    anchor: RNTupleAnchor,
    header: Header,
    header_bytes: Vec<u8>,
    footer_bytes: Vec<u8>,
}

impl RNTuple {
    /// Open the RNTuple named `name` from an open ROOT file.
    pub fn open(file: &RFile, name: &str) -> Result<RNTuple> {
        let key = file
            .key(name)
            .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
        if key.class_name != ANCHOR_CLASS {
            return Err(Error::Format(format!(
                "key {name:?} is a {}, not {ANCHOR_CLASS}",
                key.class_name
            )));
        }

        let anchor_payload = &file.data()[key.payload_range()];
        let anchor_object = root_compress::decompress(anchor_payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing anchor: {e}")))?;
        let anchor = RNTupleAnchor::read(&anchor_object)?;

        let header = read_blob(
            file.data(),
            anchor.seek_header,
            anchor.nbytes_header,
            anchor.len_header,
            "header",
        )?;
        let footer = read_blob(
            file.data(),
            anchor.seek_footer,
            anchor.nbytes_footer,
            anchor.len_footer,
            "footer",
        )?;

        // Verify the envelope checksums and types up front, and parse the schema.
        let h = read_envelope(&header)?;
        if h.type_id != ENVELOPE_HEADER {
            return Err(Error::Format(format!(
                "header envelope has type {:#x}, expected {ENVELOPE_HEADER:#x}",
                h.type_id
            )));
        }
        let parsed_header = Header::parse(h.payload)?;

        let f = read_envelope(&footer)?;
        if f.type_id != ENVELOPE_FOOTER {
            return Err(Error::Format(format!(
                "footer envelope has type {:#x}, expected {ENVELOPE_FOOTER:#x}",
                f.type_id
            )));
        }

        Ok(RNTuple {
            anchor,
            header: parsed_header,
            header_bytes: header,
            footer_bytes: footer,
        })
    }

    /// The verified anchor.
    pub fn anchor(&self) -> &RNTupleAnchor {
        &self.anchor
    }

    /// The parsed schema (fields and columns).
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The decompressed header envelope bytes.
    pub fn header_envelope(&self) -> &[u8] {
        &self.header_bytes
    }

    /// The decompressed footer envelope bytes.
    pub fn footer_envelope(&self) -> &[u8] {
        &self.footer_bytes
    }
}

/// Read and decompress an RBlob (header/footer/page list) at `seek`.
fn read_blob(data: &[u8], seek: u64, nbytes: u64, len: u64, what: &str) -> Result<Vec<u8>> {
    let start = seek as usize;
    let end = start
        .checked_add(nbytes as usize)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| Error::Format(format!("{what} blob at {seek} runs past end of file")))?;
    root_compress::decompress(&data[start..end], len as usize)
        .map_err(|e| Error::Format(format!("decompressing {what}: {e}")))
}
