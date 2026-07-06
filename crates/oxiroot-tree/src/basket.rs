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
use oxiroot_io_core::RFile;

/// Key version at or above which a `TKey` uses 64-bit seek pointers.
const KEY_BIG_VERSION: u16 = 1000;

/// Bytes fetched at a basket's start to parse its key header before the record
/// length (`fNbytes`) is known. A basket key is `"TBasket"` + the branch
/// name/title + a 19-byte extension — comfortably under this even for long
/// names. Kept modest so a remote read does not over-fetch: the (possibly much
/// larger) basket data is fetched separately at its exact size once `fNbytes` is
/// known. A small basket whose whole record fits here is read in a single fetch.
const BASKET_HEADER_PROBE: u64 = 1024;

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
    /// Read and decompress the basket at file offset `seek`, fetching only the
    /// basket's own bytes from `file` — so over a remote source a branch reads
    /// just its baskets, not the whole file. `nbytes` is the record's exact
    /// on-disk size (`fBasketBytes`) when the branch recorded it — then the whole
    /// basket is fetched in one exact request; otherwise the key header is probed
    /// to discover the size.
    pub fn read(file: &RFile, seek: u64, nbytes: Option<usize>) -> Result<Basket> {
        // With the exact size known, fetch the whole record at once; otherwise
        // probe a bounded window to parse the key header (which reveals `fNbytes`).
        let avail = file.size().saturating_sub(seek);
        let want = match nbytes {
            Some(n) => (n as u64).min(avail),
            None => BASKET_HEADER_PROBE.min(avail),
        };
        let head = file.read_at(seek, want as usize)?;
        let mut r = RBuffer::new(&head);

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

        // The (possibly compressed) data starts at fSeekKey + fKeyLen and runs to
        // the record end (fNbytes). Reuse the bytes already in the probe window
        // and fetch only the tail past it — so no byte is fetched twice and a
        // basket costs exactly its on-disk size over a remote source.
        let record_end = key_len + on_disk;
        let raw: Vec<u8> = if record_end <= head.len() {
            head[key_len..record_end].to_vec()
        } else {
            let have = head.len();
            let tail = file.read_at(seek + have as u64, record_end - have)?;
            let mut buf = Vec::with_capacity(on_disk);
            buf.extend_from_slice(&head[key_len..have]);
            buf.extend_from_slice(&tail);
            buf
        };

        let data = if on_disk == obj_len as usize {
            raw
        } else {
            oxiroot_compress::decompress(&raw, obj_len as usize)
                .map_err(|e| Error::Format(format!("decompressing basket: {e}")))?
        };

        // `fLast` is measured from the key start; the boundary within the data
        // buffer is `fLast − fKeyLen`, clamped to the buffer.
        let border = last.saturating_sub(key_len).min(data.len());

        // A variable-length branch appends its `fEntryOffset` array after the
        // entry data: `int32 count` then `count` basket-relative offsets (made
        // relative to the data buffer by subtracting the key length). ROOT stores
        // `n_entries + 1` values, but the final one is a sentinel — `0` in ROOT
        // C++, the border in uproot — so the last entry always ends at `border`.
        // Keep the `n_entries` start offsets and append `border` as the end.
        let entry_offsets = if border < data.len() {
            let mut o = RBuffer::new(&data[border..]);
            let count = o.be_i32()?.max(0) as usize;
            let mut offs = Vec::with_capacity(count.min(o.remaining()));
            for _ in 0..count {
                let raw = o.be_i32()? as i64 - key_len as i64;
                offs.push(raw.clamp(0, border as i64) as usize);
            }
            offs.truncate(n_entries as usize);
            offs.push(border);
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
