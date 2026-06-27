//! Error type shared across the core container code.

use std::fmt;

/// Convenience alias for results produced by this crate.
pub type Result<T> = std::result::Result<T, Error>;

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
    /// An object class version is not supported by this reader.
    UnsupportedVersion {
        /// The ROOT class name.
        class: &'static str,
        /// The unsupported on-disk class version.
        version: u16,
    },
    /// A generic, described format violation.
    Format(String),
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
            Error::UnsupportedVersion { class, version } => {
                write!(f, "unsupported {class} version {version}")
            }
            Error::Format(s) => write!(f, "format error: {s}"),
            Error::BinningMismatch { detail } => write!(f, "binning mismatch: {detail}"),
            Error::SchemaChanged { detail } => write!(f, "schema changed: {detail}"),
            Error::Io { message, .. } => write!(f, "I/O error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}
