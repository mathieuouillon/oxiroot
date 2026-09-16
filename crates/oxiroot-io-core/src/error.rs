//! Error type shared across the core container code.

use std::fmt;

use oxiroot_compress::CompressError;

/// Convenience alias for results produced by this crate.
///
/// The error type defaults to [`Error`] but can be overridden, so
/// `Result<T, E>` still names the standard two-parameter type where this alias
/// has been glob-imported (as `oxiroot::prelude::*` does).
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors raised while reading or writing ROOT container structures.
///
/// Marked `#[non_exhaustive]`: match with a wildcard arm so new variants can be
/// added in a minor release without breaking downstream code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A read ran past the end of the buffer.
    UnexpectedEof {
        /// Bytes the read required.
        needed: usize,
        /// Bytes still available in the buffer.
        available: usize,
    },
    /// A ROOT string field did not contain valid UTF-8.
    InvalidUtf8,
    /// The file did not start with the `"root"` magic bytes.
    BadMagic([u8; 4]),
    /// A streamed object's byte count did not match the bytes consumed.
    ByteCountMismatch {
        /// Byte count the object's header declared.
        expected: usize,
        /// Bytes actually consumed reading it.
        got: usize,
    },
    /// A generic, described format violation.
    Format(String),
    /// A stored payload could not be decompressed. `source` says why; a codec
    /// this build cannot decode is [`CompressError::CodecUnavailable`].
    Decompress {
        /// What was being read (e.g. `key "h"`, `RNTuple page`); may be empty.
        context: String,
        /// The codec's error.
        source: CompressError,
    },
    /// Two objects were given the same key name in one directory (which would
    /// silently shadow on read). Name them distinctly, or write them to separate
    /// subdirectories.
    DuplicateName {
        /// The clashing key name.
        name: String,
        /// Where the clash is (e.g. `"the top directory"` or a subdirectory name).
        location: String,
    },
    /// A histogram operation was asked to combine incompatible binnings.
    BinningMismatch {
        /// Human-readable description of the mismatch.
        detail: String,
    },
    /// A streaming writer received entries whose schema differs from the
    /// schema already committed to the file.
    SchemaChanged {
        /// Human-readable description of the schema change.
        detail: String,
    },
    /// A file written in the 32-bit ("small") container form grew past the
    /// ~2 GiB it can address. Write it in the 64-bit form instead (e.g.
    /// `TTreeWriter::create_large`); nothing it wrote is usable.
    FileTooLarge {
        /// The size the file reached, in bytes.
        size: u64,
    },
    /// An underlying I/O error. The [`std::io::ErrorKind`] is preserved so
    /// callers can branch on it; the message is rendered to a string so `Error`
    /// stays `Clone`.
    Io {
        /// The kind of the originating I/O error.
        kind: std::io::ErrorKind,
        /// The rendered error message.
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DuplicateName { name, location } => {
                write!(
                    f,
                    "duplicate key name {name:?} in {location}: two objects with the same name \
                     would shadow each other on read"
                )
            }
            Error::UnexpectedEof { needed, available } => {
                write!(
                    f,
                    "unexpected end of buffer: needed {needed} bytes, {available} available"
                )
            }
            Error::InvalidUtf8 => write!(f, "invalid UTF-8 in ROOT string"),
            Error::BadMagic(m) => {
                write!(f, "bad file magic {m:02x?} (expected \"root\")")
            }
            Error::ByteCountMismatch { expected, got } => {
                write!(
                    f,
                    "byte-count mismatch: object ends at {expected} but cursor is at {got}"
                )
            }
            Error::Format(s) => write!(f, "format error: {s}"),
            Error::Decompress { context, source } if context.is_empty() => {
                write!(f, "decompression failed: {source}")
            }
            Error::Decompress { context, source } => {
                write!(f, "decompressing {context}: {source}")
            }
            Error::BinningMismatch { detail } => write!(f, "binning mismatch: {detail}"),
            Error::SchemaChanged { detail } => write!(f, "schema changed: {detail}"),
            Error::FileTooLarge { size } => write!(
                f,
                "the file reached {size} bytes, more than the 32-bit container form can \
                 address (2 GiB); write it in the 64-bit form"
            ),
            Error::Io { message, .. } => write!(f, "I/O error: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Decompress { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<CompressError> for Error {
    fn from(source: CompressError) -> Self {
        Error::Decompress {
            context: String::new(),
            source,
        }
    }
}

/// Decompress a stored payload into `len` bytes (see
/// [`oxiroot_compress::decompress`]), naming `what` was read if it fails.
pub fn decompress_payload(payload: &[u8], len: usize, what: impl fmt::Display) -> Result<Vec<u8>> {
    oxiroot_compress::decompress(payload, len).map_err(|source| Error::Decompress {
        context: what.to_string(),
        source,
    })
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}
