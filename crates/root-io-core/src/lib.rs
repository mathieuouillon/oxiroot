//! Core ROOT (TFile) container support for `root-rs`.
//!
//! This crate is the format-agnostic foundation that `root-rntuple` and
//! `root-hist` build on. It owns:
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
pub mod error;
pub mod file;
pub mod streamer;
pub mod streamer_info;

pub use error::{Error, Result};
pub use file::{Directory, FileHeader, FreeSegment, RFile, TDatime, TKey, TUuid};
pub use streamer::{read_tnamed, read_tobject, skip_versioned, TNamed, TObjectHeader};
pub use streamer_info::{parse_streamer_info, StreamerElement, StreamerInfo, StreamerRegistry};
