//! Error type for the plotting crate.

use std::fmt;

/// Errors that can occur while building or saving a figure.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// An I/O error while writing an image file.
    Io(std::io::Error),
    /// An image encoder failed.
    Encode(String),
    /// The output path had an extension other than `.png`, `.svg`, or `.pdf`.
    UnknownFormat(String),
    /// A figure dimension was zero or absurdly large.
    BadSize(String),
    /// A custom font could not be parsed.
    Font(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Encode(m) => write!(f, "image encode error: {m}"),
            Error::UnknownFormat(ext) => {
                write!(
                    f,
                    "unknown image format `{ext}` (use a .png, .svg, or .pdf path)"
                )
            }
            Error::BadSize(m) => write!(f, "invalid figure size: {m}"),
            Error::Font(m) => write!(f, "font error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, Error>;
