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
pub mod header;
pub mod reader;

pub use anchor::{RNTupleAnchor, ANCHOR_CLASS};
pub use column::ColumnType;
pub use envelope::{read_envelope, read_frame, Envelope, Frame};
pub use header::{ColumnDescriptor, FieldDescriptor, Header, StructRole};
pub use reader::RNTuple;
