//! [`ByteSource`] — a random-access byte provider behind [`RFile`].
//!
//! A ROOT file is read by seeking to absolute offsets (a key's `fSeekKey`, an
//! RNTuple page locator, a TBasket seek) and pulling out a contiguous range.
//! Abstracting that single operation — "give me `len` bytes at `offset`" — lets
//! the same readers run against an in-memory buffer, a memory map, a local file
//! read positionally, or (with the `http` feature) a remote file fetched with
//! HTTP byte-range requests, downloading only the ranges actually touched.
//!
//! Ranges are returned as [`bytes::Bytes`]: for the in-memory and mmap backings
//! this is a zero-copy view sharing the underlying allocation, so the resident
//! read path stays copy-free.

use bytes::Bytes;

use crate::error::{Error, Result};

/// A random-access source of a ROOT file's bytes.
///
/// Implementations must be cheap to share across threads (`Send + Sync`) — the
/// TTree reader fetches baskets in parallel under the `rayon` feature.
pub trait ByteSource: Send + Sync + std::fmt::Debug {
    /// Total length of the file in bytes.
    fn len(&self) -> u64;

    /// Whether the file is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read exactly `len` bytes starting at absolute `offset`, or error if the
    /// requested range falls outside the file.
    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes>;
}

/// Bounds-check `[offset, offset + len)` against a file of `total` bytes,
/// returning the `usize` `(start, end)` on success. Never overflows `usize`.
fn checked_range(offset: u64, len: usize, total: u64) -> Result<(usize, usize)> {
    let end = offset
        .checked_add(len as u64)
        .filter(|&e| e <= total)
        .ok_or_else(|| Error::UnexpectedEof {
            needed: len,
            available: total.saturating_sub(offset) as usize,
        })?;
    // `end <= total` and `total` came from a real file/response length, so both
    // fit `usize` on any target that could hold the file in the first place.
    let start = usize::try_from(offset).map_err(|_| Error::UnexpectedEof {
        needed: len,
        available: 0,
    })?;
    let end = usize::try_from(end).map_err(|_| Error::UnexpectedEof {
        needed: len,
        available: 0,
    })?;
    Ok((start, end))
}

/// The whole file resident in memory as [`Bytes`]; `read_at` is a zero-copy
/// slice. Backs [`RFile::from_bytes`](super::rfile::RFile::from_bytes) and the
/// default [`RFile::open`](super::rfile::RFile::open).
#[derive(Debug)]
pub struct BytesSource(Bytes);

impl BytesSource {
    /// Wrap an owned in-memory buffer.
    pub fn new(data: impl Into<Bytes>) -> Self {
        BytesSource(data.into())
    }
}

impl ByteSource for BytesSource {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        let (start, end) = checked_range(offset, len, self.0.len() as u64)?;
        Ok(self.0.slice(start..end))
    }
}

/// A memory-mapped file; `read_at` copies the requested range out of the map.
/// Backs [`RFile::open_mmap`](super::rfile::RFile::open_mmap).
#[cfg(feature = "mmap")]
#[derive(Debug)]
pub struct MmapSource(memmap2::Mmap);

#[cfg(feature = "mmap")]
impl MmapSource {
    /// Wrap a read-only memory map of the whole file.
    pub fn new(map: memmap2::Mmap) -> Self {
        MmapSource(map)
    }
}

#[cfg(feature = "mmap")]
impl ByteSource for MmapSource {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        let (start, end) = checked_range(offset, len, self.0.len() as u64)?;
        Ok(Bytes::copy_from_slice(&self.0[start..end]))
    }
}

/// A local file read with positioned reads (`pread`/`seek_read`) — every
/// `read_at` touches only its range, so a large file is never slurped whole.
/// Backs [`RFile::open_ranged`](super::rfile::RFile::open_ranged).
#[derive(Debug)]
pub struct FileSource {
    file: std::fs::File,
    len: u64,
}

impl FileSource {
    /// Open `path` for positioned reads, recording its length up front.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        Ok(FileSource { file, len })
    }
}

impl ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        checked_range(offset, len, self.len)?;
        let mut buf = vec![0u8; len];
        read_exact_at(&self.file, &mut buf, offset)?;
        Ok(Bytes::from(buf))
    }
}

/// Fill `buf` from `file` starting at absolute `offset`, looping until full
/// (a positioned read may return fewer bytes than requested). Uses the
/// platform positioned-read primitive so no shared cursor is mutated — several
/// threads may read the same file concurrently.
fn read_exact_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = positioned_read(file, &mut buf[filled..], offset + filled as u64)?;
        if n == 0 {
            return Err(Error::UnexpectedEof {
                needed: buf.len(),
                available: filled,
            });
        }
        filled += n;
    }
    Ok(())
}

#[cfg(unix)]
fn positioned_read(file: &std::fs::File, buf: &mut [u8], offset: u64) -> Result<usize> {
    use std::os::unix::fs::FileExt;
    Ok(file.read_at(buf, offset)?)
}

#[cfg(windows)]
fn positioned_read(file: &std::fs::File, buf: &mut [u8], offset: u64) -> Result<usize> {
    use std::os::windows::fs::FileExt;
    Ok(file.seek_read(buf, offset)?)
}

#[cfg(not(any(unix, windows)))]
fn positioned_read(file: &std::fs::File, buf: &mut [u8], offset: u64) -> Result<usize> {
    // Fallback for exotic targets: clone the handle and seek+read. Correct but
    // not concurrency-friendly; unix/windows use true positioned reads above.
    use std::io::{Read, Seek, SeekFrom};
    let mut f = file.try_clone()?;
    f.seek(SeekFrom::Start(offset))?;
    Ok(f.read(buf)?)
}
