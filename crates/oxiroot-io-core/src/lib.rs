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
pub mod streamer;
pub mod streamer_info;

pub use compression::Compression;
pub use error::{Error, Result};
pub use file::{
    key_len, key_len_fmt, update_root_file, write_key_header, write_key_header_cycle,
    write_key_header_fmt, write_root_file, write_root_file_with_dirs,
    write_root_file_with_streamers, Directory, FileHeader, FreeSegment, ObjectRecord, RFile,
    Subdir, TDatime, TKey, TUuid, KSTART_BIG_FILE,
};
pub use object::{ObjHeader, TagReader};
pub use streamer::{
    read_tnamed, read_tobject, skip_versioned, write_tnamed, write_tobject, TNamed, TObjectHeader,
};
pub use streamer_info::{parse_streamer_info, StreamerElement, StreamerInfo, StreamerRegistry};
