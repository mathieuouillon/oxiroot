//! [`Compression`]: the algorithm and level a writer applies, as ROOT's
//! `algorithm * 100 + level` setting integer.

use crate::Algorithm;

/// How a writer should compress object payloads and RNTuple pages.
///
/// Maps to ROOT's `algorithm * 100 + level` setting integer
/// ([`setting`](Compression::setting)), which files record in their header and
/// which [`compress`](crate::compress) takes. This crate encodes and decodes all
/// four algorithms. Levels run from 1 to 9 in ROOT; a level above 99 is treated
/// as 99, since it would otherwise spill into the algorithm digit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// Store uncompressed.
    #[default]
    None,
    /// Zstandard (ROOT's default algorithm, at level 5). The pure-Rust encoder
    /// has a single compressing level, so the level is recorded but does not
    /// change the ratio.
    Zstd(u32),
    /// zlib / DEFLATE (ROOT's classic default, at level 1). The level tunes the
    /// encoder.
    Zlib(u32),
    /// LZ4. The pure-Rust encoder is fast-only, so the level is recorded but does
    /// not change the ratio.
    Lz4(u32),
    /// LZMA, as an XZ stream. The level selects the XZ preset.
    Lzma(u32),
}

impl Compression {
    /// The ROOT setting integer (`algorithm * 100 + level`, 0 = none).
    #[must_use]
    pub const fn setting(self) -> u32 {
        let (algorithm, level) = match self {
            Compression::None => return 0,
            Compression::Zlib(level) => (1, level),
            Compression::Lzma(level) => (2, level),
            Compression::Lz4(level) => (4, level),
            Compression::Zstd(level) => (5, level),
        };
        algorithm * 100 + if level > 99 { 99 } else { level }
    }

    /// The compression a ROOT setting integer describes, or `None` for an
    /// algorithm this crate cannot encode. `0` (and any setting whose algorithm
    /// digit is 0) is [`Compression::None`].
    #[must_use]
    pub const fn from_setting(setting: u32) -> Option<Compression> {
        let level = setting % 100;
        Some(match setting / 100 {
            0 => Compression::None,
            1 => Compression::Zlib(level),
            2 => Compression::Lzma(level),
            4 => Compression::Lz4(level),
            5 => Compression::Zstd(level),
            _ => return None,
        })
    }

    /// The block algorithm this setting encodes with, or `None` for
    /// [`Compression::None`].
    #[must_use]
    pub const fn algorithm(self) -> Option<Algorithm> {
        match self {
            Compression::None => None,
            Compression::Zstd(_) => Some(Algorithm::Zstd),
            Compression::Zlib(_) => Some(Algorithm::Zlib),
            Compression::Lz4(_) => Some(Algorithm::Lz4),
            Compression::Lzma(_) => Some(Algorithm::Lzma),
        }
    }

    /// Whether anything is compressed (i.e. not [`Compression::None`]).
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Compression::None)
    }
}

#[cfg(test)]
mod tests {
    use super::Compression;

    #[test]
    fn settings_round_trip() {
        for c in [
            Compression::None,
            Compression::Zstd(5),
            Compression::Zlib(1),
            Compression::Lz4(4),
            Compression::Lzma(9),
        ] {
            assert_eq!(Compression::from_setting(c.setting()), Some(c));
        }
        assert_eq!(Compression::Zstd(5).setting(), 505);
        assert_eq!(
            Compression::from_setting(301),
            None,
            "the legacy algorithm 3"
        );
    }

    #[test]
    fn an_out_of_range_level_keeps_its_algorithm() {
        assert_eq!(Compression::Zlib(250).setting(), 199);
        assert_eq!(Compression::Zstd(u32::MAX).setting(), 599);
    }
}
