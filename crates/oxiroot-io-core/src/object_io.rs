//! The ROOT object read/write framework shared by every persistable oxiroot
//! type: the [`WriteRoot`] and [`ReadRoot`] traits, plus the small helpers they
//! build on (fetching a key's class + decompressed bytes, and the single-object
//! write path).
//!
//! Concrete objects — histograms, graphs, matrices, parameters, … — live in the
//! higher-level crates and `impl` these traits; this crate owns only the
//! framework so a leaf crate (e.g. `oxiroot-linalg`) can persist its own types
//! without depending on `oxiroot-hist`.

use std::borrow::Cow;
use std::path::Path;

use crate::error::{Error, Result};
use crate::file::{write_root_file_with_streamers, ObjectRecord};
use crate::{Compression, RFile};

/// Read a ROOT object of this type from an open file by key name, auto-detecting
/// the on-disk precision where one applies (`TH1D`/`F`/`I`/`S`/`C`/`L` all read
/// into a `TH1`). This is the way to read any single object:
///
/// ```ignore
/// let f = RFile::open("in.root")?;
/// let h = TH1::read_root(&f, "h")?;               // any of TH1D/F/I/S/C/L
/// let s = TH1::read_root_in(&f, "by_region", "sig")?; // from a subdirectory
/// ```
pub trait ReadRoot: Sized {
    /// Read the object stored under key `name` in the file's top directory.
    fn read_root(file: &RFile, name: &str) -> Result<Self>;
    /// Read the object stored under key `name` inside subdirectory `dir`.
    fn read_root_in(file: &RFile, dir: &str, name: &str) -> Result<Self>;
}

/// A ROOT object this workspace can serialize. Implementors provide the class
/// name, key name/title, and streamed payload; the trait supplies the
/// single-object [`write_root`](WriteRoot::write_root) shorthand.
///
/// The result reads in ROOT, uproot, and this crate. Types that need to describe
/// their class to uproot (so it can model an unknown object) override
/// [`streamer_blob`](WriteRoot::streamer_blob); collections override
/// [`contained_classes`](WriteRoot::contained_classes) so their members'
/// streamer info is embedded too.
pub trait WriteRoot {
    /// The ROOT class name written for this object (e.g. `"TH1D"`, `"TProfile"`).
    fn root_class(&self) -> String;
    /// The object's key name (`fName`).
    fn root_name(&self) -> &str;
    /// The object's title (`fTitle`).
    fn root_title(&self) -> &str;
    /// Serialize the streamed object payload (no file/key framing) — the bytes
    /// stored under the object's key.
    #[must_use]
    fn to_root_bytes(&self) -> Vec<u8>;

    /// The class names of any objects this one *contains* (a collection's
    /// members), so their `TStreamerInfo` is embedded too. Empty for the leaf
    /// types; overridden by collections such as `ObjList`.
    fn contained_classes(&self) -> Vec<String> {
        Vec::new()
    }

    /// The `TList<TStreamerInfo>` blob to embed when this object is written on
    /// its own via [`write_root`](WriteRoot::write_root), describing its class to
    /// uproot/ROOT. The default is empty (no streamer info); types that need to
    /// be self-describing override it (histograms return a baked blob, matrices
    /// bake their class from a `Cls` list via
    /// [`streamer_info_list`](crate::streamer_gen::streamer_info_list)).
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        Cow::Borrowed(&[])
    }

    /// Write this object as the sole content of a new ROOT file at `path`.
    fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()>
    where
        Self: Sized,
    {
        if self.root_name().is_empty() {
            return Err(Error::Format(format!(
                "cannot write an unnamed {}; give it a key name with `.named(\"...\")`",
                self.root_class()
            )));
        }
        let streamers = self.streamer_blob();
        let streamers = (!streamers.is_empty()).then(|| streamers.as_ref());
        write_named(path, |file_name| {
            write_root_file_with_streamers(
                file_name,
                &[record_of(self)],
                compression.setting(),
                streamers,
            )
        })
    }
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
pub fn object_bytes_any(file: &RFile, name: &str) -> Result<(String, Vec<u8>)> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    let payload = key.payload(file.data())?;
    let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
    Ok((key.class_name.clone(), object))
}

/// Like [`object_bytes_any`], but also return the key's header length, needed by
/// the object-reference map ([`crate::object::TagReader`]) to resolve the class
/// back-references inside a collection (a `THStack`'s `TList` of histograms, a
/// `TMultiGraph`'s `TList` of graphs).
pub fn object_bytes_any_keyed(file: &RFile, name: &str) -> Result<(String, Vec<u8>, usize)> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    let payload = key.payload(file.data())?;
    let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
    Ok((key.class_name.clone(), object, key.key_len as usize))
}
