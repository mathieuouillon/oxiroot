//! RNTuple — ROOT's columnar event-data format — reader and writer.
//!
//! Implements the on-disk binary specification v1.0 (ROOT v6.34 and later). Reading
//! proceeds anchor → header/footer envelopes → page list → pages → column
//! decode. The anchor is big-endian; envelopes and payloads are little-endian;
//! integrity is checked with XXH3-64 throughout.
//!
//! Spec: <https://github.com/root-project/root/blob/v6-34-00-patches/tree/ntuple/v7/doc/BinaryFormatSpecification.md>

mod anchor;
mod column;
mod envelope;
mod field;
mod footer;
mod header;
mod merge;
mod page;
mod pagelist;
mod reader;
mod streamer;
mod writer;

pub use oxiroot_io_core::Compression;

// The format modules are private: the spec is still moving (the writer already
// emits 1.0.1.0 frames). Reading and writing go through `RNTuple` and the
// writer types; the anchor, header and footer types stay public for schema
// introspection.
pub use anchor::{RNTupleAnchor, ANCHOR_CLASS};
pub use column::ColumnType;
pub use envelope::Locator;
pub use field::FieldValues;
pub use footer::{ClusterGroup, Footer};
pub use header::{ColumnDescriptor, FieldDescriptor, Header, StructRole};
pub use merge::{append_ntuples, concat_ntuples};
pub use page::ColumnValues;
pub use reader::RNTuple;
pub use writer::{rntuple_file_bytes, write_rntuple_file, Column, Field, Ntuple, RNTupleWriter};
