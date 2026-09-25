//! The ROOT object read/write framework shared by every persistable oxiroot
//! type: the [`WriteRoot`] and [`ReadRoot`] traits, plus the small helpers they
//! build on (fetching a key's class + decompressed bytes, and the single-object
//! write path).
//!
//! Concrete objects — histograms, graphs, matrices, parameters, … — live in the
//! higher-level crates and `impl` these traits; this crate owns only the
//! framework so a leaf crate (e.g. `oxiroot-linalg`) can persist its own types
//! without depending on `oxiroot-hist`.

use std::fmt;
use std::path::Path;

use std::io::Cursor;

use crate::error::{decompress_payload, Error, Result};
use crate::file::{ContainerWriter, DirId, KSTART_BIG_FILE};
use crate::streamer_gen::Cls;
use crate::{Compression, FileReader};

/// Read a ROOT object of this type from an open file by key name, auto-detecting
/// the on-disk precision where one applies (`TH1D`/`F`/`I`/`S`/`C`/`L` all read
/// into a `Hist1D`). This is the way to read any single object:
///
/// ```ignore
/// let f = FileReader::open("in.root")?;
/// let h = Hist1D::read_root(&f, "h")?;               // any of TH1D/F/I/S/C/L
/// let s = Hist1D::read_root_in(&f, "by_region", "sig")?; // from a subdirectory
/// ```
pub trait ReadRoot: Sized {
    /// Read the object stored under key `name` in the file's top directory.
    fn read_root(file: &FileReader, name: &str) -> Result<Self>;
    /// Read the object stored under key `name` inside subdirectory `dir`.
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self>;
}

/// A ROOT object this workspace can serialize. Implementors provide the class
/// name, key name/title, and streamed payload; the trait supplies the
/// single-object [`write_root`](WriteRoot::write_root) shorthand.
///
/// The result reads in ROOT, uproot, and this crate. A type whose class a reader
/// may not know describes it with
/// [`streamer_classes`](WriteRoot::streamer_classes); every file that stores the
/// object embeds that description, so uproot can model it.
pub trait WriteRoot {
    /// The ROOT class name written for this object (e.g. `"TH1D"`, `"Profile1D"`).
    fn root_class(&self) -> String;
    /// The object's key name (`fName`).
    fn root_name(&self) -> &str;
    /// The object's title (`fTitle`).
    fn root_title(&self) -> &str;
    /// Serialize the streamed object payload (no file/key framing) — the bytes
    /// stored under the object's key.
    #[must_use]
    fn to_root_bytes(&self) -> Vec<u8>;

    /// The `TStreamerInfo` entries describing this object's class, and the
    /// classes of any objects it contains, for readers that do not know them.
    /// The default is none. Files that store the object embed these, once per
    /// class name.
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        Vec::new()
    }

    /// Write this object as the sole content of a new ROOT file at `path`.
    fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()>
    where
        Self: Sized,
    {
        if self.root_name().is_empty() {
            return Err(Error::InvalidInput(format!(
                "cannot write an unnamed {}; give it a key name with `.named(\"...\")`",
                self.root_class()
            )));
        }
        let mut streamers = StreamerSet::default();
        streamers.add(self);
        let record = record_of(self);
        write_named(path, |file_name| {
            ContainerWriter::build(file_name, compression, KSTART_BIG_FILE, |c| {
                c.place_key(
                    DirId::TOP,
                    &record.class_name,
                    &record.name,
                    &record.title,
                    &record.object,
                )?;
                c.place_streamer_info(&[], streamers.classes())
            })
        })
    }
}

/// An object stored as several records rather than under a single key: data
/// blocks placed anywhere in the file, then the key that locates them. A `TTree`
/// (its baskets, then the tree) and an RNTuple (its envelopes and pages, then
/// the anchor) are written this way. [`FileWriter::put`](crate::FileWriter::put)
/// stores one in a file, next to any other objects.
pub trait WriteInto {
    /// The class of the key that locates the object (e.g. `"TTree"`).
    fn root_class(&self) -> String;
    /// The name of that key.
    fn root_name(&self) -> &str;
    /// Write the object's records at the end of `file`, with its key in `dir`.
    /// This may run more than once for one file: the builder lays a file out
    /// again when it has to switch to the 64-bit form.
    fn write_into(&self, file: &mut ContainerWriter<Cursor<Vec<u8>>>, dir: DirId) -> Result<()>;
    /// The `TStreamerInfo` entries the object's classes need; see
    /// [`WriteRoot::streamer_classes`].
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        Vec::new()
    }
}

/// The streamer info a set of objects needs: each class their
/// [`streamer_classes`](WriteRoot::streamer_classes) describe, once.
#[derive(Clone, Default)]
pub struct StreamerSet {
    classes: Vec<Cls<'static>>,
}

impl StreamerSet {
    /// Add what `object` needs.
    pub fn add(&mut self, object: &dyn WriteRoot) {
        self.add_classes(object.streamer_classes());
    }

    /// Add what a multi-record `object` needs.
    pub fn add_records(&mut self, object: &dyn WriteInto) {
        self.add_classes(object.streamer_classes());
    }

    /// Add each class not already present at the same version.
    pub fn add_classes(&mut self, classes: impl IntoIterator<Item = Cls<'static>>) {
        for class in classes {
            if !self
                .classes
                .iter()
                .any(|c| c.name == class.name && c.version == class.version)
            {
                self.classes.push(class);
            }
        }
    }

    /// Add everything `other` holds.
    pub fn extend(&mut self, other: &StreamerSet) {
        self.add_classes(other.classes.iter().cloned());
    }

    /// The classes, in the order they were first added.
    #[must_use]
    pub fn classes(&self) -> &[Cls<'static>] {
        &self.classes
    }
}

impl fmt::Debug for StreamerSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamerSet")
            .field(
                "classes",
                &self.classes.iter().map(|c| &*c.name).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// One object to store in a file: its class, name, title, and streamed bytes
/// (including the object's own byte count and version).
pub struct ObjectRecord {
    /// ROOT class name (e.g. `"TH1D"`).
    pub class_name: String,
    /// Object name (the key name).
    pub name: String,
    /// Object title.
    pub title: String,
    /// Streamed object bytes.
    pub object: Vec<u8>,
}

/// The on-disk record for any writable object — its class, name, title, and
/// streamed payload. Shared by the [`WriteRoot`] single-object path and the
/// multi-object file builder.
pub fn record_of(object: &dyn WriteRoot) -> ObjectRecord {
    ObjectRecord {
        class_name: object.root_class(),
        name: object.root_name().to_string(),
        title: object.root_title().to_string(),
        object: object.to_root_bytes(),
    }
}

/// Derive the in-file name from `path`, build the file bytes, and write them.
/// Shared by the single-object write path so it agrees on path handling, the
/// default name, and the error type.
fn write_named(path: impl AsRef<Path>, build: impl FnOnce(&str) -> Result<Vec<u8>>) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(path, build(file_name)?)?;
    Ok(())
}

/// Return a key's class name together with its decompressed object bytes,
/// without checking the class.
pub fn object_bytes_any(file: &FileReader, name: &str) -> Result<(String, Vec<u8>)> {
    let key = file.key(name).ok_or_else(|| Error::NotFound {
        what: "key",
        name: name.to_string(),
    })?;
    let payload = file.key_payload(key)?;
    let object = decompress_payload(&payload, key.obj_len as usize, format_args!("key {name:?}"))?;
    Ok((key.class_name.clone(), object))
}

/// Like [`object_bytes_any`], but also return the key's header length, needed by
/// the object-reference map ([`crate::object::TagReader`]) to resolve the class
/// back-references inside a collection (a `HistStack`'s `TList` of histograms, a
/// `GraphStack`'s `TList` of graphs).
pub fn object_bytes_any_keyed(file: &FileReader, name: &str) -> Result<(String, Vec<u8>, usize)> {
    let key = file.key(name).ok_or_else(|| Error::NotFound {
        what: "key",
        name: name.to_string(),
    })?;
    let payload = file.key_payload(key)?;
    let object = decompress_payload(&payload, key.obj_len as usize, format_args!("key {name:?}"))?;
    Ok((key.class_name.clone(), object, key.key_len as usize))
}
