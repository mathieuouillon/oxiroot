//! Error type shared across the core container code.

use std::fmt;

/// Convenience alias for results produced by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors raised while reading or writing ROOT container structures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A read ran past the end of the buffer.
    UnexpectedEof { needed: usize, available: usize },
    /// A ROOT string field did not contain valid UTF-8.
    InvalidUtf8,
    /// The file did not start with the `"root"` magic bytes.
    BadMagic([u8; 4]),
    /// A streamed object's byte count did not match the bytes consumed.
    ByteCountMismatch { expected: usize, got: usize },
    /// An object class version is not supported by this reader.
    UnsupportedVersion { class: &'static str, version: u16 },
    /// A generic, described format violation.
    Format(String),
    /// An underlying I/O error (rendered to a string so `Error` stays `Clone`).
    Io(String),
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
            Error::Io(s) => write!(f, "I/O error: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}
