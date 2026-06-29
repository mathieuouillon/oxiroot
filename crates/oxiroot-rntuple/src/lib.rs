//! RNTuple — ROOT's columnar event-data format — reader.
//!
//! Implements the on-disk binary specification v1.0.0.0 (ROOT v6.34). Reading
//! proceeds anchor → header/footer envelopes → page list → pages → column
//! decode. The anchor is big-endian; envelopes and payloads are little-endian;
//! integrity is checked with XXH3-64 throughout.
//!
//! Spec: <https://github.com/root-project/root/blob/v6-34-00-patches/tree/ntuple/v7/doc/BinaryFormatSpecification.md>

pub mod anchor;
pub mod column;
pub mod envelope;
pub mod field;
pub mod footer;
pub mod header;
pub mod page;
pub mod pagelist;
pub mod reader;
pub mod streamer;
pub mod writer;

pub use oxiroot_io_core::Compression;

pub use anchor::{RNTupleAnchor, ANCHOR_CLASS};
pub use column::ColumnType;
pub use envelope::{read_envelope, read_frame, read_locator, Envelope, Frame, Locator};
pub use field::FieldValues;
pub use footer::{ClusterGroup, Footer};
pub use header::{ColumnDescriptor, FieldDescriptor, Header, StructRole};
pub use page::{read_column, ColumnValues};
pub use pagelist::{ClusterPages, ClusterSummary, ColumnPages, PageInfo, PageList};
pub use reader::RNTuple;
pub use writer::{rntuple_file_bytes, write_rntuple_file, Column, Field, Ntuple, RNTupleWriter};
