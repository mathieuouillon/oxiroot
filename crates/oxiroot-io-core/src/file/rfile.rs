//! [`RFile`] — the high-level entry point for reading a ROOT file.
//!
//! Mirrors the spirit of `ROOT::Experimental::RFile`: a small Open/Get/List
//! surface over the TFile container. M1 provides reading and key enumeration;
//! object materialization (`get`) and writing arrive in later milestones.

use std::path::Path;

use bytes::Bytes;

use super::directory::Directory;
use super::free::{read_free, FreeSegment};
use super::header::FileHeader;
use super::key::TKey;
use super::source::{ByteSource, BytesSource, FileSource};
use crate::buffer::RBuffer;
use crate::error::{Error, Result};
use crate::read_object::read_object;
use crate::streamer_info::{parse_streamer_info, StreamerRegistry};
use crate::value::Value;

/// Bytes fetched from the start of the file to parse its header. The TFile
/// header is ~100 bytes (a little more in the 64-bit form); 512 covers it with
/// room to spare and is a single small read for a remote source.
const HEADER_PROBE: u64 = 512;

/// An open ROOT file, read through a [`ByteSource`]. The default
/// [`open`](Self::open) reads the whole file into memory; [`open_ranged`] and
/// (with the `http` feature) [`open_url`](Self::open_url) read only the byte
/// ranges each object touches, never downloading the file whole.
///
/// The header, root directory, and key list are parsed at open (a few small
/// ranged reads); object, page, and basket bytes are fetched on demand.
pub struct RFile {
    source: Box<dyn ByteSource>,
    size: u64,
    header: FileHeader,
    root_dir: Directory,
}

impl std::fmt::Debug for RFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RFile")
            .field("bytes", &self.size)
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

    /// Open a local ROOT file using positioned reads, fetching only the byte
    /// ranges each object touches instead of reading the whole file up front —
    /// the local analog of [`open_url`](Self::open_url), useful for large files
    /// where only some objects are read.
    pub fn open_ranged(path: impl AsRef<Path>) -> Result<RFile> {
        Self::from_source(Box::new(FileSource::open(path)?))
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
        // This is the sole `unsafe` in the workspace (the `unsafe_code` lint is
        // denied everywhere else); memmap2's `map` is unavoidably `unsafe`.
        #[allow(unsafe_code)]
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Self::from_source(Box::new(super::source::MmapSource::new(mmap)))
    }

    /// Open a ROOT file over HTTP(S), reading only the byte ranges each object
    /// touches via range requests — never downloading the file whole, the way
    /// ROOT and uproot read remote files. The server must honor
    /// `Range: bytes=…` requests (`Accept-Ranges: bytes`).
    ///
    /// Requires the `http` feature.
    #[cfg(feature = "http")]
    pub fn open_url(url: &str) -> Result<RFile> {
        Self::from_source(Box::new(super::http::HttpSource::open(url)?))
    }

    /// Parse a ROOT file already held in memory.
    pub fn from_bytes(data: Vec<u8>) -> Result<RFile> {
        Self::from_source(Box::new(BytesSource::new(data)))
    }

    /// Parse a ROOT file from an arbitrary [`ByteSource`]. The header, root
    /// directory, and key list are read at open; everything else is on demand.
    pub fn from_source(source: Box<dyn ByteSource>) -> Result<RFile> {
        let size = source.len();
        let head = source.read_at(0, HEADER_PROBE.min(size) as usize)?;
        let header = {
            let mut r = RBuffer::new(&head);
            FileHeader::read(&mut r)?
        };
        let root_dir = Directory::read_root(&*source, &header)?;
        Ok(RFile {
            source,
            size,
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

    /// Navigate into a subdirectory, returning its parsed [`Directory`] (with the
    /// keys it directly contains). A `/`-separated `path` descends through nested
    /// subdirectories (`"cal/pedestals"`); a plain name selects a single level.
    /// Errors if any path component has no such subdirectory key.
    ///
    /// Accepts both `TDirectory` (the in-memory class oxiroot writes) and
    /// `TDirectoryFile` (the class official ROOT C++ records on disk), so
    /// subdirectories of ROOT-written files are navigable.
    pub fn subdir(&self, path: &str) -> Result<Directory> {
        let mut current: Option<Directory> = None;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            current = Some(self.read_subdir(current.as_ref(), part)?);
        }
        current.ok_or_else(|| Error::Format(format!("empty subdirectory path {path:?}")))
    }

    /// Read the subdirectory `name` directly inside `parent` (or the root
    /// directory when `parent` is `None`).
    fn read_subdir(&self, parent: Option<&Directory>, name: &str) -> Result<Directory> {
        let keys = parent.map_or(self.root_dir.keys.as_slice(), |d| d.keys.as_slice());
        let key = keys
            .iter()
            .find(|k| {
                k.name == name && (k.class_name == "TDirectory" || k.class_name == "TDirectoryFile")
            })
            .ok_or_else(|| Error::Format(format!("no subdirectory named {name:?}")))?;
        Directory::read(&*self.source, key.payload_start(self.size as usize)? as u64)
    }

    /// Return the class name and decompressed object bytes for key `name` inside
    /// subdirectory `subdir`.
    pub fn object_in(&self, subdir: &str, name: &str) -> Result<(String, Vec<u8>)> {
        let (class, object, _keylen) = self.object_in_keyed(subdir, name)?;
        Ok((class, object))
    }

    /// Like [`object_in`](Self::object_in) but also return the key's header
    /// length (`fKeyLen`), needed to resolve object-reference back-references in
    /// streamed objects (e.g. `TH2Poly`'s bins) read from a subdirectory.
    pub fn object_in_keyed(&self, subdir: &str, name: &str) -> Result<(String, Vec<u8>, usize)> {
        let dir = self.subdir(subdir)?;
        let key = dir
            .keys
            .iter()
            .filter(|k| k.name == name && !k.is_deleted())
            .max_by_key(|k| k.cycle)
            .ok_or_else(|| Error::Format(format!("no key {name:?} in subdirectory {subdir:?}")))?;
        let payload = self.key_payload(key)?;
        let object = oxiroot_compress::decompress(&payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
        Ok((key.class_name.clone(), object, key.key_len as usize))
    }

    /// Read a top-level object of **any** class into a dynamic [`Value`] tree,
    /// driven entirely by the file's `TStreamerInfo` — no typed model required.
    /// This is the generic reader behind rootls / rootprint-style inspection; use
    /// it when you do not have (or do not want) a `TH1`/`TGraph`/… struct.
    ///
    /// A class the reader cannot decode comes back as [`Value::Unsupported`]
    /// rather than an error.
    pub fn get_value(&self, name: &str) -> Result<Value> {
        let (class, object, keylen) = crate::object_io::object_bytes_any_keyed(self, name)?;
        let reg = self.streamer_registry()?;
        Ok(read_object(&reg, &class, &object, keylen))
    }

    /// Like [`get_value`](Self::get_value) but for an object in subdirectory
    /// `subdir` (a `/`-separated path).
    pub fn get_value_in(&self, subdir: &str, name: &str) -> Result<Value> {
        let (class, object, keylen) = self.object_in_keyed(subdir, name)?;
        let reg = self.streamer_registry()?;
        Ok(read_object(&reg, &class, &object, keylen))
    }

    /// The file's free-segment list (informational).
    pub fn free_segments(&self) -> Result<Vec<FreeSegment>> {
        read_free(&*self.source, &self.header)
    }

    /// Parse the file's `TStreamerInfo` records (at `fSeekInfo`) into a registry
    /// describing every class stored in the file.
    pub fn streamer_registry(&self) -> Result<StreamerRegistry> {
        let Some((object, keylen)) = self.streamer_info_decompressed()? else {
            return Ok(StreamerRegistry::default());
        };
        parse_streamer_info(&object, keylen)
    }

    /// The decompressed streamer-info object (the `TList<TStreamerInfo>` bytes at
    /// `fSeekInfo`), or `None` if the file has none. Used to carry a file's
    /// streamer info across a rewrite.
    pub fn streamer_info_object(&self) -> Result<Option<Vec<u8>>> {
        Ok(self.streamer_info_decompressed()?.map(|(object, _)| object))
    }

    /// Fetch the `[fSeekInfo, fNbytesInfo]` record and return its decompressed
    /// object bytes plus the wrapping key's `fKeyLen`, or `None` if the file has
    /// no streamer info. Only that one record is read.
    fn streamer_info_decompressed(&self) -> Result<Option<(Vec<u8>, usize)>> {
        if self.header.seek_info == 0 || self.header.nbytes_info == 0 {
            return Ok(None);
        }
        let win = self.read_at(self.header.seek_info, self.header.nbytes_info as usize)?;
        let key = TKey::read(&mut RBuffer::new(&win))?;
        let payload = payload_in_window(&win, &key)?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing streamer info: {e}")))?;
        Ok(Some((object, key.key_len as usize)))
    }

    /// Total size of the file in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Read exactly `len` bytes at absolute `offset` from the underlying source.
    /// For a resident (in-memory / mmap) file this is a zero-copy slice; for a
    /// ranged local or remote file it fetches just that range. Object, RNTuple
    /// page, and TTree basket readers go through this so they touch only the
    /// bytes they need.
    pub fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        self.source.read_at(offset, len)
    }

    /// Fetch a key's (possibly compressed) object payload — the `fNbytes −
    /// fKeyLen` bytes after its header. Bounds-checked against the file size;
    /// errors (never panics) on a malformed key.
    pub fn key_payload(&self, key: &TKey) -> Result<Bytes> {
        let start = key.payload_start(self.size as usize)? as u64;
        let len = key
            .total_bytes()
            .checked_sub(u32::from(key.key_len))
            .ok_or_else(|| Error::Format(format!("key {:?}: fKeyLen exceeds fNbytes", key.name)))?
            as usize;
        self.read_at(start, len)
    }
}

/// An [`RFile`] is itself a byte source (delegating to its backing), so page and
/// basket decoders can take a `&dyn ByteSource` and be unit-tested against a
/// bare in-memory buffer without a full file.
impl ByteSource for RFile {
    fn len(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        self.source.read_at(offset, len)
    }
}

/// Slice a wrapping key's payload out of a window fetched at the key's own
/// offset: the payload begins `fKeyLen` bytes into the record and runs for
/// `fNbytes − fKeyLen` bytes. Bounds-checked against the window.
fn payload_in_window<'a>(win: &'a [u8], key: &TKey) -> Result<&'a [u8]> {
    let start = key.key_len as usize;
    let len = key
        .total_bytes()
        .checked_sub(u32::from(key.key_len))
        .ok_or_else(|| Error::Format(format!("key {:?}: fKeyLen exceeds fNbytes", key.name)))?
        as usize;
    win.get(start..start + len)
        .ok_or_else(|| Error::Format(format!("key {:?}: payload runs past record", key.name)))
}
