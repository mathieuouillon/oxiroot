//! The TFile header (first ~100 bytes of every ROOT file) and its TUUID.
//!
//! Field widths depend on the file version: once any file pointer would exceed
//! ~2 GiB, ROOT rewrites the file in "big" form — `fVersion` is bumped past
//! [`BIG_FILE_VERSION`], `fUnits` becomes 8, and `fEND`/`fSeekFree`/`fSeekInfo`
//! widen to 64 bits. Layout mirrors uproot's `_file_header_fields_{small,big}`.

use crate::buffer::RBuffer;
use crate::error::{Error, Result};

/// Magic bytes at the start of every ROOT file.
pub const MAGIC: &[u8; 4] = b"root";

/// `fVersion` at or above this value indicates the 64-bit ("big") on-disk form.
pub const BIG_FILE_VERSION: u32 = 1_000_000;

/// A ROOT `TUUID`: a 16-bit version followed by 16 UUID bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TUuid {
    /// `TUUID` streamer version.
    pub version: u16,
    /// The 16 raw UUID bytes.
    pub bytes: [u8; 16],
}

/// Parsed TFile header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    /// On-disk format version (`fVersion`); `>= BIG_FILE_VERSION` ⇒ 64-bit form.
    pub version: u32,
    /// Offset of the first data record (`fBEGIN`, normally 100).
    pub begin: u64,
    /// Current end-of-file offset (`fEND`).
    pub end: u64,
    /// Offset of the free-segments record (`fSeekFree`).
    pub seek_free: u64,
    /// Size in bytes of the free-segments record (`fNbytesFree`).
    pub nbytes_free: u32,
    /// Number of free segments (`nfree`).
    pub nfree: u32,
    /// Size of the directory name record at creation (`fNbytesName`).
    pub nbytes_name: u32,
    /// Bytes per file pointer (`fUnits`, 4 or 8).
    pub units: u8,
    /// Compression settings (`fCompress`, `algorithm * 100 + level`).
    pub compress: u32,
    /// Offset of the `TStreamerInfo` record (`fSeekInfo`).
    pub seek_info: u64,
    /// Size in bytes of the `TStreamerInfo` record (`fNbytesInfo`).
    pub nbytes_info: u32,
    /// File UUID.
    pub uuid: TUuid,
}

impl FileHeader {
    /// Whether the file uses the 64-bit ("big") on-disk form.
    pub fn is_big(&self) -> bool {
        self.version >= BIG_FILE_VERSION
    }

    /// Parse a header from the start of `r`.
    pub fn read(r: &mut RBuffer) -> Result<FileHeader> {
        let magic_slice = r.bytes(4)?;
        let mut magic = [0u8; 4];
        magic.copy_from_slice(magic_slice);
        if &magic != MAGIC {
            return Err(Error::BadMagic(magic));
        }

        let version = r.be_u32()?;
        let big = version >= BIG_FILE_VERSION;
        let begin = r.be_u32()? as u64;
        let (end, seek_free) = if big {
            (r.be_u64()?, r.be_u64()?)
        } else {
            (r.be_u32()? as u64, r.be_u32()? as u64)
        };
        let nbytes_free = r.be_u32()?;
        let nfree = r.be_u32()?;
        let nbytes_name = r.be_u32()?;
        let units = r.u8()?;
        let compress = r.be_u32()?;
        let seek_info = if big { r.be_u64()? } else { r.be_u32()? as u64 };
        let nbytes_info = r.be_u32()?;
        let uuid_version = r.be_u16()?;
        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(r.bytes(16)?);

        Ok(FileHeader {
            version,
            begin,
            end,
            seek_free,
            nbytes_free,
            nfree,
            nbytes_name,
            units,
            compress,
            seek_info,
            nbytes_info,
            uuid: TUuid {
                version: uuid_version,
                bytes: uuid_bytes,
            },
        })
    }
}
