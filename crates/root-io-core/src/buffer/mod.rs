//! Read and write cursors for ROOT on-disk structures.

mod reader;
mod writer;

pub use reader::{RBuffer, VersionHeader, K_BYTE_COUNT_MASK};
pub use writer::{CountToken, Patch, WBuffer};
