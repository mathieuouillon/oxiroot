//! RNTuple — ROOT's columnar event-data format — reader and writer.
//!
//! Implements the on-disk binary specification v1.0.0.0 (ROOT v6.34). The
//! reader (anchor → header/footer → page list → pages → column decode) lands in
//! milestone M3; the writer in M5.
//!
//! Spec: <https://github.com/root-project/root/blob/v6-34-00-patches/tree/ntuple/v7/doc/BinaryFormatSpecification.md>
