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

/// An open ROOT file, read fully into memory.
pub struct RFile {
    data: Vec<u8>,
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
    /// Open and parse a ROOT file from disk.
    pub fn open(path: impl AsRef<Path>) -> Result<RFile> {
        Self::from_bytes(std::fs::read(path)?)
    }

    /// Parse a ROOT file already held in memory.
    pub fn from_bytes(data: Vec<u8>) -> Result<RFile> {
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
        let payload = &self.data[key.payload_range()];
        let object = root_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing streamer info: {e}")))?;
        parse_streamer_info(&object, key.key_len as usize)
    }

    /// The raw file bytes (used by object readers in later milestones).
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
