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
    /// The bytes break the ROOT format: a truncated record, a size that runs
    /// past its end, an offset out of range, … The file is corrupt, or not what
    /// it claims to be.
    Format(String),
    /// No key, subdirectory, branch or field of that name.
    NotFound {
        /// What was looked for: `"key"`, `"subdirectory"`, `"branch"`, `"field"`,
        /// …
        what: &'static str,
        /// The name looked for. A key in a subdirectory is named by its path
        /// (`"dir/name"`).
        name: String,
    },
    /// A key holds a different class than the one asked for: reading a `TH2F` as
    /// a `TH1`, say.
    WrongClass {
        /// The key's name (its path, in a subdirectory); empty if unknown.
        name: String,
        /// The class the key holds.
        found: String,
        /// The class, or classes, that were asked for.
        expected: String,
    },
    /// An object's class version is one this crate cannot decode: older than
    /// the first version ROOT describes through streamer info, or newer than
    /// oxiroot knows.
    UnsupportedVersion {
        /// The class.
        class: String,
        /// Its version in the file.
        version: i32,
    },
    /// The file has no `TStreamerInfo` for a class it holds, and decoding the
    /// class needs one.
    MissingStreamerInfo {
        /// The class.
        class: String,
    },
    /// A checksum stored in the file does not match its data: the data is
    /// corrupt.
    ChecksumMismatch {
        /// What was checked, e.g. `RNTuple page`.
        what: String,
        /// The checksum of the data read.
        computed: u64,
        /// The checksum stored in the file.
        stored: u64,
    },
    /// The input is valid ROOT, but oxiroot does not read or write it yet: a
    /// column encoding, a streamer type, a set of objects `hadd` cannot merge,
    /// … The message says what.
    Unsupported(String),
    /// An argument cannot be used: an object without a name, an axis without
    /// edges, no inputs to merge, a batch over a format limit, … Nothing was
    /// written. The message says what and, where there is one, the fix.
    InvalidInput(String),
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
    /// Inputs that must have the same length do not: a graph's `x` and `y`, a
    /// fill's values and weights, a tree's branches, an RNTuple's fields, …
    /// Nothing was changed or written.
    LengthMismatch {
        /// What has the wrong length, e.g. `TGraph y` or `branch "pt"`.
        what: String,
        /// The length it must have (that of the input it is paired with).
        expected: usize,
        /// Its actual length.
        found: usize,
    },
    /// Inputs that must share a schema do not: a streaming writer's batches,
    /// the trees or RNTuples being concatenated, or the trees of a chain.
    SchemaChanged {
        /// Human-readable description of the schema change.
        detail: String,
    },
    /// A file written in the 32-bit ("small") container form grew past the
    /// ~2 GiB it can address. Write it in the 64-bit form instead (e.g.
    /// `TreeWriter::create_large`); nothing it wrote is usable.
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
    /// A statistics function rejected its input (converted from
    /// [`oxiroot_stat::StatError`]; the `stat` feature).
    #[cfg(feature = "stat")]
    Stat(oxiroot_stat::StatError),
    /// A formula did not parse (converted from [`oxiroot_formula::ParseError`];
    /// the `formula` feature).
    #[cfg(feature = "formula")]
    Formula(oxiroot_formula::ParseError),
    /// Building or saving a figure failed (converted from `oxiroot_plot::Error`,
    /// whose I/O errors become [`Error::Io`]); the message says why.
    Plot(String),
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
            Error::NotFound { what, name } => write!(f, "no {what} named {name:?}"),
            Error::WrongClass {
                name,
                found,
                expected,
            } if name.is_empty() => write!(f, "the object is a {found}, not a {expected}"),
            Error::WrongClass {
                name,
                found,
                expected,
            } => write!(f, "key {name:?} is a {found}, not a {expected}"),
            Error::UnsupportedVersion { class, version } => {
                write!(f, "{class} class version {version} is not supported")
            }
            Error::MissingStreamerInfo { class } => {
                write!(f, "the file has no TStreamerInfo for {class}")
            }
            Error::ChecksumMismatch {
                what,
                computed,
                stored,
            } => write!(
                f,
                "{what} checksum mismatch: computed {computed:#018x}, stored {stored:#018x}"
            ),
            Error::Unsupported(s) | Error::InvalidInput(s) => f.write_str(s),
            Error::Decompress { context, source } if context.is_empty() => {
                write!(f, "decompression failed: {source}")
            }
            Error::Decompress { context, source } => {
                write!(f, "decompressing {context}: {source}")
            }
            Error::BinningMismatch { detail } => write!(f, "binning mismatch: {detail}"),
            Error::LengthMismatch {
                what,
                expected,
                found,
            } => write!(
                f,
                "length mismatch: {what} has length {found}, expected {expected}"
            ),
            Error::SchemaChanged { detail } => write!(f, "schema changed: {detail}"),
            Error::FileTooLarge { size } => write!(
                f,
                "the file reached {size} bytes, more than the 32-bit container form can \
                 address (2 GiB); write it in the 64-bit form"
            ),
            Error::Io { message, .. } => write!(f, "I/O error: {message}"),
            #[cfg(feature = "stat")]
            Error::Stat(e) => write!(f, "statistics: {e}"),
            #[cfg(feature = "formula")]
            Error::Formula(e) => write!(f, "invalid formula: {e}"),
            Error::Plot(message) => write!(f, "plotting: {message}"),
        }
    }
}

impl Error {
    /// This error with `context` in front of its message, for the variants that
    /// carry a free-text message (`Format`, `Unsupported`, `InvalidInput`,
    /// `SchemaChanged`, `BinningMismatch`). The other variants are returned
    /// unchanged, so the error keeps its type.
    #[must_use]
    pub fn context(self, context: impl fmt::Display) -> Error {
        match self {
            Error::Format(m) => Error::Format(format!("{context}: {m}")),
            Error::Unsupported(m) => Error::Unsupported(format!("{context}: {m}")),
            Error::InvalidInput(m) => Error::InvalidInput(format!("{context}: {m}")),
            Error::SchemaChanged { detail } => Error::SchemaChanged {
                detail: format!("{context}: {detail}"),
            },
            Error::BinningMismatch { detail } => Error::BinningMismatch {
                detail: format!("{context}: {detail}"),
            },
            other => other,
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Decompress { source, .. } => Some(source),
            #[cfg(feature = "stat")]
            Error::Stat(e) => Some(e),
            #[cfg(feature = "formula")]
            Error::Formula(e) => Some(e),
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

#[cfg(feature = "stat")]
impl From<oxiroot_stat::StatError> for Error {
    fn from(e: oxiroot_stat::StatError) -> Self {
        Error::Stat(e)
    }
}

#[cfg(feature = "formula")]
impl From<oxiroot_formula::ParseError> for Error {
    fn from(e: oxiroot_formula::ParseError) -> Self {
        Error::Formula(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}
