//! The compression setting a writer applies to object payloads and pages.

/// How a writer should compress object payloads and RNTuple pages.
///
/// Maps to ROOT's `algorithm*100 + level` setting integer. These are the
/// algorithms this crate can *encode*; LZMA is supported for reading only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// Store uncompressed.
    #[default]
    None,
    /// Zstandard at the given level (1–22; ROOT's default is 5).
    Zstd(u32),
    /// zlib / DEFLATE at the given level (1–9; ROOT's classic default is 1).
    Zlib(u32),
    /// LZ4 at the given level (1–9; the pure-Rust backend is fast-only).
    Lz4(u32),
}

impl Compression {
    /// The ROOT setting integer (`algorithm*100 + level`, 0 = none).
    #[must_use]
    pub const fn setting(self) -> u32 {
        match self {
            Compression::None => 0,
            Compression::Zstd(level) => 500 + level,
            Compression::Zlib(level) => 100 + level,
            Compression::Lz4(level) => 400 + level,
        }
    }

    /// Whether anything is compressed (i.e. not [`Compression::None`]).
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Compression::None)
    }
}
