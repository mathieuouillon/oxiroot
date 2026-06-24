//! [`RFile`] — the high-level entry point for reading a ROOT file.
//!
//! Mirrors the spirit of `ROOT::Experimental::RFile`: a small Open/Get/List
//! surface over the TFile container. M1 provides reading and key enumeration;
//! object materialization (`get`) and writing arrive in later milestones.

use std::path::Path;

use super::directory::Directory;
use super::free::{read_free, FreeSegment};
use super::header::FileHeader;
use super::key::TKey;
use crate::buffer::RBuffer;
use crate::error::{Error, Result};
use crate::streamer_info::{parse_streamer_info, StreamerRegistry};

/// Backing storage for a file's raw bytes: an owned in-memory buffer or, with
/// the `mmap` feature, a read-only memory map of the file on disk. Both deref to
/// `&[u8]`, so all parsing is identical regardless of source.
enum FileData {
    Owned(Vec<u8>),
    #[cfg(feature = "mmap")]
    Mapped(memmap2::Mmap),
}

impl std::ops::Deref for FileData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            FileData::Owned(v) => v,
            #[cfg(feature = "mmap")]
            FileData::Mapped(m) => m,
        }
    }
}

/// An open ROOT file. Its bytes are either read fully into memory or, with the
/// `mmap` feature and `RFile::open_mmap`, memory-mapped.
pub struct RFile {
    data: FileData,
    header: FileHeader,
    root_dir: Directory,
}

impl std::fmt::Debug for RFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RFile")
            .field("bytes", &self.data.len())
            .field("version", &self.header.version)
            .field("keys", &self.root_dir.keys.len())
            .finish()
    }
}

impl RFile {
    /// Open and parse a ROOT file from disk (read fully into memory).
    pub fn open(path: impl AsRef<Path>) -> Result<RFile> {
        Self::from_bytes(std::fs::read(path)?)
    }

    /// Open and parse a ROOT file by memory-mapping it, avoiding a full read into
    /// memory — useful for large files where only some objects are touched.
    ///
    /// Requires the `mmap` feature. The map is read-only; as with any `mmap`,
    /// the caller must ensure the file is not modified or truncated by another
    /// process while the [`RFile`] is alive (which would be undefined behavior).
    #[cfg(feature = "mmap")]
    pub fn open_mmap(path: impl AsRef<Path>) -> Result<RFile> {
        let file = std::fs::File::open(path)?;
        // SAFETY: see the read-only / no-concurrent-modification contract above.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Self::from_backing(FileData::Mapped(mmap))
    }

    /// Parse a ROOT file already held in memory.
    pub fn from_bytes(data: Vec<u8>) -> Result<RFile> {
        Self::from_backing(FileData::Owned(data))
    }

    fn from_backing(data: FileData) -> Result<RFile> {
        let header = {
            let mut r = RBuffer::new(&data);
            FileHeader::read(&mut r)?
        };
        let root_dir = Directory::read_root(&data, &header)?;
        Ok(RFile {
            data,
            header,
            root_dir,
        })
    }

    /// The parsed file header.
    pub fn header(&self) -> &FileHeader {
        &self.header
    }

    /// The root (top-level) directory.
    pub fn root_directory(&self) -> &Directory {
        &self.root_dir
    }

    /// The keys in the root directory.
    pub fn keys(&self) -> &[TKey] {
        &self.root_dir.keys
    }

    /// Look up a key by name, returning the highest cycle if several share it.
    pub fn key(&self, name: &str) -> Option<&TKey> {
        self.root_dir
            .keys
            .iter()
            .filter(|k| k.name == name && !k.is_deleted())
            .max_by_key(|k| k.cycle)
    }

    /// Navigate into a subdirectory by name, returning its parsed [`Directory`]
    /// (with the keys it directly contains). Errors if the root directory has no
    /// such `TDirectory` key.
    pub fn subdir(&self, name: &str) -> Result<Directory> {
        let key = self
            .root_dir
            .keys
            .iter()
            .find(|k| k.name == name && k.class_name == "TDirectory")
            .ok_or_else(|| Error::Format(format!("no subdirectory named {name:?}")))?;
        Directory::read(&self.data, key.payload_start(self.data.len())?)
    }

    /// Return the class name and decompressed object bytes for key `name` inside
    /// subdirectory `subdir`.
    pub fn object_in(&self, subdir: &str, name: &str) -> Result<(String, Vec<u8>)> {
        let dir = self.subdir(subdir)?;
        let key = dir
            .keys
            .iter()
            .filter(|k| k.name == name && !k.is_deleted())
            .max_by_key(|k| k.cycle)
            .ok_or_else(|| Error::Format(format!("no key {name:?} in subdirectory {subdir:?}")))?;
        let payload = key.payload(&self.data)?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
        Ok((key.class_name.clone(), object))
    }

    /// The file's free-segment list (informational).
    pub fn free_segments(&self) -> Result<Vec<FreeSegment>> {
        read_free(&self.data, &self.header)
    }

    /// Parse the file's `TStreamerInfo` records (at `fSeekInfo`) into a registry
    /// describing every class stored in the file.
    pub fn streamer_registry(&self) -> Result<StreamerRegistry> {
        if self.header.seek_info == 0 || self.header.nbytes_info == 0 {
            return Ok(StreamerRegistry::default());
        }
        let mut r = RBuffer::new(&self.data);
        r.seek(self.header.seek_info as usize)?;
        let key = TKey::read(&mut r)?;
        let payload = key.payload(&self.data)?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing streamer info: {e}")))?;
        parse_streamer_info(&object, key.key_len as usize)
    }

    /// The decompressed streamer-info object (the `TList<TStreamerInfo>` bytes at
    /// `fSeekInfo`), or `None` if the file has none. Used to carry a file's
    /// streamer info across a rewrite.
    pub fn streamer_info_object(&self) -> Result<Option<Vec<u8>>> {
        if self.header.seek_info == 0 || self.header.nbytes_info == 0 {
            return Ok(None);
        }
        let mut r = RBuffer::new(&self.data);
        r.seek(self.header.seek_info as usize)?;
        let key = TKey::read(&mut r)?;
        let payload = key.payload(&self.data)?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing streamer info: {e}")))?;
        Ok(Some(object))
    }

    /// The raw file bytes (used by object readers in later milestones).
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
