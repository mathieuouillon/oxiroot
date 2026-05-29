//! The compression setting a writer applies to object payloads and pages.

/// How a writer should compress object payloads and RNTuple pages.
///
/// Maps to ROOT's `algorithm*100 + level` setting integer. Only `None` and
/// `Zstd` are offered because those are the algorithms this crate can *encode*
/// (zlib and LZ4 are supported for reading only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// Store uncompressed.
    #[default]
    None,
    /// Zstandard at the given level (1–22; ROOT's default is 5).
    Zstd(u32),
}

impl Compression {
    /// The ROOT setting integer (`algorithm*100 + level`, 0 = none).
    pub const fn setting(self) -> u32 {
        match self {
            Compression::None => 0,
            Compression::Zstd(level) => 500 + level,
        }
    }

    /// Whether anything is compressed (i.e. not [`Compression::None`]).
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Compression::None)
    }
}
