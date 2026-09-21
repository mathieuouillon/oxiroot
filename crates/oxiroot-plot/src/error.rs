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
    /// The output needs a crate feature this build does not enable.
    MissingFeature {
        /// The output that was asked for (e.g. `"PNG"`).
        output: &'static str,
        /// The `oxiroot-plot` feature that provides it.
        feature: &'static str,
    },
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
            Error::MissingFeature { output, feature } => write!(
                f,
                "{output} output needs the `{feature}` feature of oxiroot-plot"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// With the `hist` feature, plotting errors convert into the error the rest of
/// oxiroot uses, so `?` works in a function returning `oxiroot::Result` that both
/// reads files and saves figures. I/O errors keep their kind
/// ([`oxiroot_io_core::Error::Io`]); the others become
/// [`oxiroot_io_core::Error::Plot`] with this error's message.
#[cfg(feature = "hist")]
impl From<Error> for oxiroot_io_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(e) => oxiroot_io_core::Error::from(e),
            other => oxiroot_io_core::Error::Plot(other.to_string()),
        }
    }
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, Error>;
