//! The RNTuple anchor — the small `ROOT::RNTuple` object stored in a TFile key
//! that locates the header and footer envelopes.
//!
//! In the key the anchor is wrapped in ROOT's `{byte-count, version}` object
//! header, then 64 bytes of **big-endian** fields, then an 8-byte XXH3-64
//! checksum over those 64 field bytes.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};

/// The ROOT class name under which the anchor is stored.
pub const ANCHOR_CLASS: &str = "ROOT::RNTuple";

/// Number of big-endian anchor field bytes covered by the checksum.
const ANCHOR_FIELDS_LEN: usize = 64;

/// The `ROOT::RNTuple` streamer class version the format spec fixes (and that
/// our writer, ROOT, and uproot all emit).
const ANCHOR_CLASS_VERSION: u16 = 2;

/// A parsed RNTuple anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RNTupleAnchor {
    /// Format version epoch.
    pub version_epoch: u16,
    /// Format version major.
    pub version_major: u16,
    /// Format version minor.
    pub version_minor: u16,
    /// Format version patch.
    pub version_patch: u16,
    /// File offset of the header envelope.
    pub seek_header: u64,
    /// Compressed size of the header envelope.
    pub nbytes_header: u64,
    /// Uncompressed size of the header envelope.
    pub len_header: u64,
    /// File offset of the footer envelope.
    pub seek_footer: u64,
    /// Compressed size of the footer envelope.
    pub nbytes_footer: u64,
    /// Uncompressed size of the footer envelope.
    pub len_footer: u64,
    /// Maximum RBlob (key) payload size.
    pub max_key_size: u64,
}

impl RNTupleAnchor {
    /// Parse and checksum-verify the anchor from the decompressed TKey object
    /// bytes (which begin with ROOT's object header).
    pub fn read(object: &[u8]) -> Result<RNTupleAnchor> {
        let mut r = RBuffer::new(object);
        let header = r.read_version()?; // ROOT {byte-count, class version}

        // The anchor is a byte-count-framed `ROOT::RNTuple` object whose class
        // version the spec fixes at 2. Reject anything else up front rather than
        // trusting a malformed/foreign header and parsing 64 bytes from a wrong
        // offset (the checksum would catch it, but a clear error is better).
        if header.byte_count.is_none() {
            return Err(Error::Format(
                "RNTuple anchor: missing object byte count".into(),
            ));
        }
        if header.version != ANCHOR_CLASS_VERSION {
            return Err(Error::Format(format!(
                "RNTuple anchor: unsupported class version {} (expected {ANCHOR_CLASS_VERSION})",
                header.version
            )));
        }

        let fields_start = r.pos();
        let version_epoch = r.be_u16()?;
        let version_major = r.be_u16()?;
        let version_minor = r.be_u16()?;
        let version_patch = r.be_u16()?;
        let seek_header = r.be_u64()?;
        let nbytes_header = r.be_u64()?;
        let len_header = r.be_u64()?;
        let seek_footer = r.be_u64()?;
        let nbytes_footer = r.be_u64()?;
        let len_footer = r.be_u64()?;
        let max_key_size = r.be_u64()?;
        let fields_end = r.pos();
        debug_assert_eq!(fields_end - fields_start, ANCHOR_FIELDS_LEN);

        let stored = r.be_u64()?;
        let computed = xxhash_rust::xxh3::xxh3_64(&object[fields_start..fields_end]);
        if computed != stored {
            return Err(Error::Format(format!(
                "RNTuple anchor checksum mismatch: computed {computed:#018x}, stored {stored:#018x}"
            )));
        }

        Ok(RNTupleAnchor {
            version_epoch,
            version_major,
            version_minor,
            version_patch,
            seek_header,
            nbytes_header,
            len_header,
            seek_footer,
            nbytes_footer,
            len_footer,
            max_key_size,
        })
    }
}
