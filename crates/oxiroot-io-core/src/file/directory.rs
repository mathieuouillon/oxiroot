//! `TDirectory` records and key-list traversal.
//!
//! A directory record stores creation/modification times, a back-pointer to its
//! own location, and `fSeekKeys`/`fNbytesKeys` locating its key list. The key
//! list itself is a `TKey`-wrapped record whose payload is an `i32` count
//! followed by that many `TKey` headers. Layout mirrors uproot's
//! `_directory_format_{small,big}`.

use super::header::FileHeader;
use super::key::TKey;
use super::source::ByteSource;
use crate::buffer::RBuffer;
use crate::error::Result;

/// Directory version above which seek pointers are 64-bit.
const DIR_BIG_VERSION: i16 = 1000;

/// Upper bound on a `TDirectory` record's fixed header (big form is 42 bytes:
/// `i16 + 2*u32 + 2*i32 + 3*u64`). Fetched as one window before the seek
/// pointers are known.
const DIR_RECORD_MAX: usize = 64;

/// A parsed `TDirectory` record together with its key list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    /// Directory version (`> 1000` ⇒ 64-bit seek pointers).
    pub version: i16,
    /// Creation date/time (`fDatimeC`, raw packed `TDatime`).
    pub datime_c: u32,
    /// Last-modification date/time (`fDatimeM`, raw packed `TDatime`).
    pub datime_m: u32,
    /// Size in bytes of the key-list record (`fNbytesKeys`).
    pub nbytes_keys: i32,
    /// Size in bytes of the directory name record (`fNbytesName`).
    pub nbytes_name: i32,
    /// Offset of this directory (`fSeekDir`).
    pub seek_dir: u64,
    /// Offset of the parent directory (`fSeekParent`, 0 for the root).
    pub seek_parent: u64,
    /// Offset of the key-list record (`fSeekKeys`).
    pub seek_keys: u64,
    /// The keys contained directly in this directory.
    pub keys: Vec<TKey>,
}

impl Directory {
    /// Read a directory record at absolute `offset` from `source`, loading its
    /// key list. Only the directory record and its key-list record are fetched —
    /// two small ranges — so this stays lazy over a remote source.
    pub fn read(source: &dyn ByteSource, offset: u64) -> Result<Directory> {
        // The directory record's fixed header is at most `DIR_RECORD_MAX` bytes;
        // fetch that window (clamped to the file) and parse it standalone.
        let avail = source.len().saturating_sub(offset);
        let win = source.read_at(offset, DIR_RECORD_MAX.min(avail as usize))?;
        let mut r = RBuffer::new(&win);

        let version = r.be_i16()?;
        let datime_c = r.be_u32()?;
        let datime_m = r.be_u32()?;
        let nbytes_keys = r.be_i32()?;
        let nbytes_name = r.be_i32()?;
        let (seek_dir, seek_parent, seek_keys) = if version > DIR_BIG_VERSION {
            (r.be_u64()?, r.be_u64()?, r.be_u64()?)
        } else {
            (r.be_u32()? as u64, r.be_u32()? as u64, r.be_u32()? as u64)
        };

        let keys = read_keys(source, seek_keys, nbytes_keys)?;

        Ok(Directory {
            version,
            datime_c,
            datime_m,
            nbytes_keys,
            nbytes_name,
            seek_dir,
            seek_parent,
            seek_keys,
            keys,
        })
    }

    /// Read the root directory of a file (located at `begin + nbytes_name`).
    pub fn read_root(source: &dyn ByteSource, header: &FileHeader) -> Result<Directory> {
        Self::read(source, header.begin + header.nbytes_name as u64)
    }
}

/// Read a directory's key list: a wrapping `TKey`, an `i32` count, then that
/// many `TKey` headers. `nbytes_keys` (`fNbytesKeys`) is the exact on-disk size
/// of the record, so exactly that window is fetched.
fn read_keys(source: &dyn ByteSource, seek_keys: u64, nbytes_keys: i32) -> Result<Vec<TKey>> {
    if seek_keys == 0 {
        return Ok(Vec::new());
    }
    let avail = source.len().saturating_sub(seek_keys);
    // Trust `fNbytesKeys` when present; otherwise fall back to the rest of the
    // file so a zero/negative count still reads (key lists are small).
    let want = if nbytes_keys > 0 {
        (nbytes_keys as u64).min(avail)
    } else {
        avail
    };
    let win = source.read_at(seek_keys, want as usize)?;
    let mut r = RBuffer::new(&win);

    // The record at `seek_keys` is itself a TKey; its payload is the key list.
    let _wrapper = TKey::read(&mut r)?;
    let nkeys = r.be_i32()?.max(0) as usize;

    let mut keys = Vec::with_capacity(nkeys.min(r.remaining()));
    for _ in 0..nkeys {
        keys.push(TKey::read(&mut r)?);
    }
    Ok(keys)
}
