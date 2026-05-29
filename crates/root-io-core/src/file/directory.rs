//! `TDirectory` records and key-list traversal.
//!
//! A directory record stores creation/modification times, a back-pointer to its
//! own location, and `fSeekKeys`/`fNbytesKeys` locating its key list. The key
//! list itself is a `TKey`-wrapped record whose payload is an `i32` count
//! followed by that many `TKey` headers. Layout mirrors uproot's
//! `_directory_format_{small,big}`.

use super::header::FileHeader;
use super::key::TKey;
use crate::buffer::RBuffer;
use crate::error::Result;

/// Directory version above which seek pointers are 64-bit.
const DIR_BIG_VERSION: i16 = 1000;

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
    /// Read a directory record at absolute `offset` within `data`, loading its
    /// key list.
    pub fn read(data: &[u8], offset: usize) -> Result<Directory> {
        let mut r = RBuffer::new(data);
        r.seek(offset)?;

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

        let keys = read_keys(data, seek_keys as usize)?;

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
    pub fn read_root(data: &[u8], header: &FileHeader) -> Result<Directory> {
        Self::read(data, header.begin as usize + header.nbytes_name as usize)
    }
}

/// Read a directory's key list: a wrapping `TKey`, an `i32` count, then that
/// many `TKey` headers.
fn read_keys(data: &[u8], seek_keys: usize) -> Result<Vec<TKey>> {
    if seek_keys == 0 {
        return Ok(Vec::new());
    }
    let mut r = RBuffer::new(data);
    r.seek(seek_keys)?;

    // The record at `seek_keys` is itself a TKey; its payload is the key list.
    let _wrapper = TKey::read(&mut r)?;
    let nkeys = r.be_i32()?.max(0) as usize;

    let mut keys = Vec::with_capacity(nkeys);
    for _ in 0..nkeys {
        keys.push(TKey::read(&mut r)?);
    }
    Ok(keys)
}
