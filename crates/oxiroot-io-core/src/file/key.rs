//! `TKey` records and the `TDatime` timestamp.
//!
//! Every object in a ROOT file is preceded by a `TKey` header that locates it
//! and names its class. Keys switch to 64-bit seek pointers once the key
//! version exceeds 1000 (ROOT's large-file convention). Layout mirrors uproot's
//! `_key_format_{small,big}`.

use std::ops::Range;

use crate::buffer::RBuffer;
use crate::error::{Error, Result};

/// Key version at or below which seek pointers are 32-bit.
const KEY_BIG_VERSION: u16 = 1000;

/// A ROOT `TDatime`: a 32-bit packed date/time (bit-fields, local time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TDatime(pub u32);

impl TDatime {
    /// Calendar year.
    pub fn year(self) -> u32 {
        (self.0 >> 26) + 1995
    }
    /// Month, 1..=12.
    pub fn month(self) -> u32 {
        (self.0 >> 22) & 0xF
    }
    /// Day of month, 1..=31.
    pub fn day(self) -> u32 {
        (self.0 >> 17) & 0x1F
    }
    /// Hour, 0..=23.
    pub fn hour(self) -> u32 {
        (self.0 >> 12) & 0x1F
    }
    /// Minute, 0..=59.
    pub fn minute(self) -> u32 {
        (self.0 >> 6) & 0x3F
    }
    /// Second, 0..=59.
    pub fn second(self) -> u32 {
        self.0 & 0x3F
    }
}

/// A parsed `TKey` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TKey {
    /// Total size of the key record (header + payload). Negative ⇒ deleted/free.
    pub nbytes: i32,
    /// Key version (`> 1000` ⇒ 64-bit seek pointers).
    pub version: u16,
    /// Uncompressed object length (`fObjLen`).
    pub obj_len: u32,
    /// Creation date/time.
    pub datime: TDatime,
    /// Length of this key header in bytes (`fKeyLen`).
    pub key_len: u16,
    /// Cycle number (versioning of same-named keys).
    pub cycle: u16,
    /// Absolute file offset of this key (`fSeekKey`).
    pub seek_key: u64,
    /// Absolute file offset of the owning directory (`fSeekPdir`).
    pub seek_pdir: u64,
    /// Object class name.
    pub class_name: String,
    /// Object name.
    pub name: String,
    /// Object title.
    pub title: String,
}

impl TKey {
    /// Read a key header from `r`, leaving the cursor just past the header
    /// (exactly `key_len` bytes from where it started).
    pub fn read(r: &mut RBuffer) -> Result<TKey> {
        let start = r.pos();
        let nbytes = r.be_i32()?;
        let version = r.be_u16()?;
        let obj_len = r.be_u32()?;
        let datime = TDatime(r.be_u32()?);
        let key_len = r.be_u16()?;
        let cycle = r.be_u16()?;
        let (seek_key, seek_pdir) = if version > KEY_BIG_VERSION {
            (r.be_u64()?, r.be_u64()?)
        } else {
            (r.be_u32()? as u64, r.be_u32()? as u64)
        };
        let class_name = r.string()?;
        let name = r.string()?;
        let title = r.string()?;
        // The header occupies exactly `key_len` bytes; realign for the caller.
        r.seek(start + key_len as usize)?;

        Ok(TKey {
            nbytes,
            version,
            obj_len,
            datime,
            key_len,
            cycle,
            seek_key,
            seek_pdir,
            class_name,
            name,
            title,
        })
    }

    /// Whether this key marks deleted space (negative byte count).
    pub fn is_deleted(&self) -> bool {
        self.nbytes < 0
    }

    /// Total bytes occupied by this key record (header + payload).
    pub fn total_bytes(&self) -> u32 {
        self.nbytes.unsigned_abs()
    }

    /// Length of the (possibly compressed) object payload on disk. Saturates to
    /// 0 for a malformed key whose `key_len` exceeds its total byte count
    /// (rather than underflowing); [`payload`](TKey::payload) rejects such keys.
    pub fn payload_len(&self) -> usize {
        self.total_bytes().saturating_sub(self.key_len as u32) as usize
    }

    /// Whether the object payload is stored uncompressed (on-disk size equals
    /// the uncompressed object length).
    pub fn is_uncompressed(&self) -> bool {
        self.payload_len() == self.obj_len as usize
    }

    /// Byte range of the (possibly compressed) object payload within the file.
    /// Unvalidated — prefer [`payload`](TKey::payload), which bounds-checks
    /// against the actual buffer. Kept for callers operating on trusted data.
    pub fn payload_range(&self) -> Range<usize> {
        let start = self.seek_key as usize + self.key_len as usize;
        start..start + self.payload_len()
    }

    /// Bounds-checked absolute offset of the record body (`fSeekKey + fKeyLen`)
    /// within a buffer of length `data_len`. Returns an error — never overflows
    /// `usize` — for a malformed key whose offset wraps or points past the
    /// buffer. Use this (not [`payload_range`](TKey::payload_range)`.start`) to
    /// locate a key's body on any untrusted file.
    pub fn payload_start(&self, data_len: usize) -> Result<usize> {
        (self.seek_key as usize)
            .checked_add(self.key_len as usize)
            .filter(|&s| s <= data_len)
            .ok_or_else(|| {
                Error::Format(format!(
                    "key {:?}: payload offset past end of file",
                    self.name
                ))
            })
    }

    /// The (possibly compressed) object payload bytes within `data`, fully
    /// bounds-checked. Returns an error — never panics — for a malformed key
    /// whose `fSeekKey`/`fKeyLen`/`fNbytes` point outside the buffer or whose
    /// `fKeyLen` exceeds its byte count. Use this on any untrusted file.
    pub fn payload<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        let start = self.payload_start(data.len())?;
        let len = self
            .total_bytes()
            .checked_sub(self.key_len as u32)
            .ok_or_else(|| Error::Format(format!("key {:?}: fKeyLen exceeds fNbytes", self.name)))?
            as usize;
        let end = start
            .checked_add(len)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| {
                Error::Format(format!(
                    "key {:?}: payload runs past end of file",
                    self.name
                ))
            })?;
        Ok(&data[start..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datime_decodes_fields() {
        // 2021-03-17 12:34:56
        let packed = ((2021 - 1995) << 26) | (3 << 22) | (17 << 17) | (12 << 12) | (34 << 6) | 56;
        let dt = TDatime(packed);
        assert_eq!(dt.year(), 2021);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 17);
        assert_eq!(dt.hour(), 12);
        assert_eq!(dt.minute(), 34);
        assert_eq!(dt.second(), 56);
    }

    fn key_with(seek_key: u64, key_len: u16, nbytes: i32) -> TKey {
        TKey {
            nbytes,
            version: 1001, // big format: fSeekKey is a full 64-bit value
            obj_len: 0,
            datime: TDatime(0),
            key_len,
            cycle: 1,
            seek_key,
            seek_pdir: 0,
            class_name: "TDirectory".into(),
            name: "d".into(),
            title: String::new(),
        }
    }

    #[test]
    fn payload_start_rejects_overflowing_offset() {
        // A hostile big-format TDirectory key whose fSeekKey is near u64::MAX:
        // `seek_key + key_len` overflows usize. payload_start must return Err,
        // not panic (debug) or wrap to a small in-bounds offset (release).
        let key = key_with(u64::MAX - 8, 60, 100);
        assert!(key.payload_start(4096).is_err());
        assert!(key.payload(&[0u8; 4096]).is_err());

        // Past-the-end but non-overflowing is likewise rejected.
        let key = key_with(10_000, 60, 100);
        assert!(key.payload_start(4096).is_err());
    }

    #[test]
    fn payload_start_accepts_in_bounds() {
        let key = key_with(100, 60, 200);
        assert_eq!(key.payload_start(1024).unwrap(), 160);
        // Ending exactly at EOF is valid (slice end is exclusive).
        assert_eq!(key.payload_start(160).unwrap(), 160);
    }
}
