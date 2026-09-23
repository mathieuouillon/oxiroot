//! The ROOT file container for `oxiroot`: reading and writing `TFile`s and the
//! objects in them. The other oxiroot crates build on it.
//!
//! # Files
//!
//! [`FileReader`] opens a file from disk, from memory, through a memory map (the
//! `mmap` feature), or remotely over HTTP(S) (`http`) or XRootD (`xrootd`), and
//! reads its keys, directories and streamer info on demand through a
//! [`ByteSource`]. [`FileWriter`] writes a new file, or appends to an existing
//! one, with subdirectories ([`SubdirWriter`]). [`ContainerWriter`] is the layer
//! beneath it, which places keys and raw records: formats such as `TTree` and
//! RNTuple are more than one keyed object.
//!
//! # Objects
//!
//! Every persistable type implements [`WriteRoot`] and [`ReadRoot`], or
//! [`WriteInto`] when it is written as several records. The concrete classes
//! (histograms, graphs, matrices, …) live in the crates that model them. This
//! crate owns only the framework, so a crate can persist its own types without
//! depending on `oxiroot-hist`. The generic objects that belong to no format live
//! here: [`TObjString`], [`TParameter`], [`ObjList`] (`TList` and `TObjArray`)
//! and [`TMap`].
//!
//! # Any class, through its streamer info
//!
//! [`read_object`] decodes an object of any class into a [`Value`] tree, by the
//! member layout the file's own `TStreamerInfo` declares ([`StreamerRegistry`]),
//! with no compiled-in knowledge of the class. `rootls`- and `rootprint`-style
//! inspection runs on it.
//!
//! # Building blocks
//!
//! To implement the traits for a class of your own: [`RBuffer`] and [`WBuffer`]
//! read and write ROOT's encoding and its byte-count framing; [`read_tobject`],
//! [`read_tnamed`], [`write_tobject`] and [`write_tnamed`] stream the common base
//! classes; and [`TagReader`] resolves class tags and back-references. The
//! [`streamer_gen`] module describes a class's members for the `TStreamerInfo`
//! record a written file carries. ROOT's classic on-disk integers are big-endian;
//! the buffers name their endianness, so the same types serve RNTuple's
//! little-endian payloads.
//!
//! Every item is exported at the crate root, except the [`streamer_gen`] helpers,
//! whose short names (`basic`, `base`, `stl`, …) read best qualified.

mod buffer;
mod compression;
mod error;
mod file;
mod object;
mod object_io;
mod objects;
mod read_object;
mod streamer;
pub mod streamer_gen;
mod streamer_info;
mod value;

pub use buffer::{CountToken, Patch, RBuffer, VersionHeader, WBuffer, K_BYTE_COUNT_MASK};
pub use compression::Compression;
pub use error::{decompress_payload, Error, Result};
#[cfg(feature = "mmap")]
pub use file::MmapSource;
#[cfg(feature = "xrootd")]
pub use file::XrootdSource;
pub use file::{
    compress_if_smaller, find_key, read_free, split_cycle, ByteSource, BytesSource,
    ContainerWriter, DirId, Directory, FileHeader, FileReader, FileSource, FileWriter, FreeSegment,
    SubdirWriter, TDatime, TKey, TUuid, BIG_FILE_VERSION, DATIME, FILE_VERSION, KSTART_BIG_FILE,
    MAGIC,
};
pub use object::{ObjHeader, ObjectRef, TagReader};
pub use object_io::{
    object_bytes_any, object_bytes_any_keyed, record_of, ObjectRecord, ReadRoot, StreamerSet,
    WriteInto, WriteRoot,
};
pub use objects::{FromMember, ListKind, ObjList, ParamValue, TMap, TObjString, TParameter};
pub use read_object::read_object;
pub use streamer::{
    read_tnamed, read_tobject, skip_versioned, write_object_any, write_tnamed, write_tobject,
    TNamed, TObjectHeader,
};
pub use streamer_info::{parse_streamer_info, StreamerElement, StreamerInfo, StreamerRegistry};
pub use value::Value;
