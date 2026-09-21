//! Core ROOT (TFile) container support for `oxiroot`.
//!
//! This crate is the format-agnostic foundation that `oxiroot-rntuple` and
//! `oxiroot-hist` build on. It owns:
//!
//! - [`buffer`]: big-/little-endian read ([`buffer::RBuffer`]) and write
//!   ([`buffer::WBuffer`]) cursors, including ROOT string encoding and the
//!   streamed-object byte-count framing.
//! - The TFile header, TKey, TStreamerInfo, free list and directory tree
//!   (added in milestones M1–M2).
//!
//! ROOT's classic on-disk integers are big-endian; accessors name their
//! endianness explicitly so the same buffer types serve RNTuple's
//! little-endian payloads.

pub mod buffer;
pub mod compression;
pub mod error;
pub mod file;
pub mod object;
pub mod object_io;
pub mod objects;
pub mod read_object;
pub mod streamer;
pub mod streamer_gen;
pub mod streamer_info;
pub mod value;

pub use compression::Compression;
pub use error::{Error, Result};
pub use file::{
    compress_if_smaller, ByteSource, BytesSource, ContainerWriter, Dir, DirId, Directory,
    FileHeader, FileSource, FreeSegment, RFile, RootFile, TDatime, TKey, TUuid, DATIME,
    FILE_VERSION, KSTART_BIG_FILE,
};
pub use object::{ObjHeader, TagReader};
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
