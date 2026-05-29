//! `TKey` records and the `TDatime` timestamp.
//!
//! Every object in a ROOT file is preceded by a `TKey` header that locates it
//! and names its class. Keys switch to 64-bit seek pointers once the key
//! version exceeds 1000 (ROOT's large-file convention). Layout mirrors uproot's
//! `_key_format_{small,big}`.

use std::ops::Range;

use crate::buffer::RBuffer;
use crate::error::Result;

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

    /// Length of the (possibly compressed) object payload on disk.
    pub fn payload_len(&self) -> usize {
        self.total_bytes() as usize - self.key_len as usize
    }

    /// Whether the object payload is stored uncompressed (on-disk size equals
    /// the uncompressed object length).
    pub fn is_uncompressed(&self) -> bool {
        self.payload_len() == self.obj_len as usize
    }

    /// Byte range of the (possibly compressed) object payload within the file.
    pub fn payload_range(&self) -> Range<usize> {
        let start = self.seek_key as usize + self.key_len as usize;
        start..start + self.payload_len()
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
}
