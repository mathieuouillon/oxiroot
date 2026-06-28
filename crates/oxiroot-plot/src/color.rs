//! Colors and the matplotlib default color cycle.

use std::fmt;
use std::str::FromStr;

/// An sRGB color with 8-bit channels and alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
    /// Alpha channel (0 = transparent, 255 = opaque).
    pub a: u8,
}

impl Color {
    /// Opaque color from 8-bit RGB.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Color from 8-bit RGBA.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Parse a `#rrggbb` or `#rrggbbaa` hex literal.
    ///
    /// This is the ergonomic constructor for color **literals** known at author
    /// time (`Color::hex("#1f77b4")`). It **panics** on a malformed string so a
    /// typo surfaces immediately rather than silently rendering black. For
    /// fallible parsing of dynamic input use [`FromStr`] (`s.parse::<Color>()`).
    ///
    /// # Panics
    /// If `s` is not a valid `#rrggbb`/`#rrggbbaa` (or `rrggbb`/`rrggbbaa`) string.
    #[must_use]
    pub fn hex(s: &str) -> Self {
        s.parse().unwrap_or_else(|e| panic!("Color::hex: {e}"))
    }

    /// Return this color with a new alpha (0.0–1.0).
    #[must_use]
    pub fn with_alpha(self, alpha: f32) -> Self {
        Color {
            a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
            ..self
        }
    }

    /// `#rrggbb` (or `#rrggbbaa` when not fully opaque) for SVG output.
    #[must_use]
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Opacity as a 0.0–1.0 float (for the SVG `*-opacity` attributes).
    #[must_use]
    pub fn opacity(self) -> f32 {
        f32::from(self.a) / 255.0
    }

    /// Pure black.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Pure white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Fully transparent.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
}

/// Defaults to opaque black.
impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        Color::rgb(r, g, b)
    }
}

impl From<[u8; 3]> for Color {
    fn from([r, g, b]: [u8; 3]) -> Self {
        Color::rgb(r, g, b)
    }
}

/// Parse a `#rrggbb` or `#rrggbbaa` hex string (the leading `#` is optional).
impl FromStr for Color {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let h = s.strip_prefix('#').unwrap_or(s);
        let err = || ParseColorError(s.to_string());
        let hex2 = |i: usize| {
            h.get(i..i + 2)
                .and_then(|b| u8::from_str_radix(b, 16).ok())
                .ok_or_else(err)
        };
        match h.len() {
            6 => Ok(Color::rgb(hex2(0)?, hex2(2)?, hex2(4)?)),
            8 => Ok(Color::rgba(hex2(0)?, hex2(2)?, hex2(4)?, hex2(6)?)),
            _ => Err(err()),
        }
    }
}

/// The error returned when a string cannot be parsed as a [`Color`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColorError(String);

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid hex color `{}` (expected #rrggbb or #rrggbbaa)",
            self.0
        )
    }
}

impl std::error::Error for ParseColorError {}

/// The matplotlib default property cycle — the `tab10` palette (`C0`..`C9`).
/// Plot elements take these colors in order unless an explicit color is set.
/// To index it, use [`Style::cycle`](crate::Style::cycle) (honors a custom
/// `color_cycle`) or index `TAB10` directly.
pub const TAB10: [Color; 10] = [
    Color::rgb(0x1f, 0x77, 0xb4), // C0 blue
    Color::rgb(0xff, 0x7f, 0x0e), // C1 orange
    Color::rgb(0x2c, 0xa0, 0x2c), // C2 green
    Color::rgb(0xd6, 0x27, 0x28), // C3 red
    Color::rgb(0x94, 0x67, 0xbd), // C4 purple
    Color::rgb(0x8c, 0x56, 0x4b), // C5 brown
    Color::rgb(0xe3, 0x77, 0xc2), // C6 pink
    Color::rgb(0x7f, 0x7f, 0x7f), // C7 gray
    Color::rgb(0xbc, 0xbd, 0x22), // C8 olive
    Color::rgb(0x17, 0xbe, 0xcf), // C9 cyan
];
