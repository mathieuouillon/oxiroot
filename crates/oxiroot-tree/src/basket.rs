//! Reading a `TBasket` — the unit of branch data on disk.
//!
//! A basket is a `TKey` whose `fKeyLen` *includes* a 19-byte TBasket extension
//! after the title strings: `fVersion(u16) fBufferSize(i32) fNevBufSize(i32)
//! fNevBuf(i32) fLast(i32) flag(u8)`. The data starts at `fSeekKey + fKeyLen`
//! and is compressed iff its on-disk size differs from the key's `fObjLen`. The
//! uncompressed buffer holds the entry data in `[0, border)` (`border = fLast −
//! fKeyLen`); for variable-length branches `[border, fObjLen)` is the
//! `fEntryOffset` array.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};

/// Key version at or above which a `TKey` uses 64-bit seek pointers.
const KEY_BIG_VERSION: u16 = 1000;

/// A decoded basket: its entry count and uncompressed buffer.
pub(crate) struct Basket {
    /// Number of entries in this basket (`fNevBuf`).
    pub n_entries: u32,
    /// Byte offset within `data` where entry data ends and the `fEntryOffset`
    /// array begins (`fLast − fKeyLen`); equals `data.len()` for fixed branches.
    pub border: usize,
    /// The uncompressed basket buffer (`fObjLen` bytes).
    pub data: Vec<u8>,
    /// For variable-length branches, the per-entry byte offsets into the entry
    /// region (`fEntryOffset`, `n_entries + 1` of them, made relative to the
    /// data buffer). `None` for fixed/scalar branches.
    pub entry_offsets: Option<Vec<usize>>,
}

impl Basket {
    /// Read and decompress the basket at file offset `seek` within `file_data`.
    pub fn read(file_data: &[u8], seek: u64) -> Result<Basket> {
        let mut r = RBuffer::new(file_data);
        let key_start = seek as usize;
        r.seek(key_start)?;

        // Standard TKey header.
        let nbytes = r.be_i32()?;
        let key_version = r.be_u16()?;
        let obj_len = r.be_u32()?;
        let _datime = r.be_u32()?;
        let key_len = r.be_u16()?;
        let _cycle = r.be_u16()?;
        if key_version > KEY_BIG_VERSION {
            let _seek_key = r.be_u64()?;
            let _seek_pdir = r.be_u64()?;
        } else {
            let _seek_key = r.be_u32()?;
            let _seek_pdir = r.be_u32()?;
        }
        let _class = r.string()?; // "TBasket"
        let _name = r.string()?;
        let _title = r.string()?;

        // TBasket extension (the tail of the key header, within fKeyLen).
        let _basket_version = r.be_u16()?;
        let _buffer_size = r.be_i32()?;
        let _nev_buf_size = r.be_i32()?;
        let n_entries = r.be_i32()?.max(0) as u32; // fNevBuf
        let last = r.be_i32()?.max(0) as usize; // fLast
        let _flag = r.u8()?;

        let key_len = key_len as usize;
        let nbytes = nbytes.unsigned_abs() as usize;
        let on_disk = nbytes
            .checked_sub(key_len)
            .ok_or_else(|| Error::Format("basket fKeyLen exceeds fNbytes".into()))?;
        let data_start = key_start
            .checked_add(key_len)
            .filter(|&s| s.checked_add(on_disk).is_some_and(|e| e <= file_data.len()))
            .ok_or_else(|| Error::Format("basket data runs past end of file".into()))?;
        let raw = &file_data[data_start..data_start + on_disk];

        let data = if on_disk == obj_len as usize {
            raw.to_vec()
        } else {
            oxiroot_compress::decompress(raw, obj_len as usize)
                .map_err(|e| Error::Format(format!("decompressing basket: {e}")))?
        };

        // `fLast` is measured from the key start; the boundary within the data
        // buffer is `fLast − fKeyLen`, clamped to the buffer.
        let border = last.saturating_sub(key_len).min(data.len());

        // A variable-length branch appends its `fEntryOffset` array after the
        // entry data: `int32 count` then `count` basket-relative offsets. Make
        // them relative to the data buffer (subtract the key length).
        let entry_offsets = if border < data.len() {
            let mut o = RBuffer::new(&data[border..]);
            let count = o.be_i32()?.max(0) as usize;
            let mut offs = Vec::with_capacity(count.min(o.remaining()));
            for _ in 0..count {
                let raw = o.be_i32()? as i64 - key_len as i64;
                offs.push(raw.clamp(0, border as i64) as usize);
            }
            Some(offs)
        } else {
            None
        };

        Ok(Basket {
            n_entries,
            border,
            data,
            entry_offsets,
        })
    }

    /// The entry-data region (before any `fEntryOffset` array).
    pub fn entry_data(&self) -> &[u8] {
        &self.data[..self.border]
    }
}
