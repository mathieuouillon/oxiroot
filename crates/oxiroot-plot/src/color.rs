//! Colors and the matplotlib default color cycle.

/// An sRGB color with 8-bit channels and alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Parse a `#rrggbb` or `#rrggbbaa` hex string. Returns black on a malformed
    /// string (so style tables can use literals without unwrapping).
    #[must_use]
    pub fn hex(s: &str) -> Self {
        let s = s.strip_prefix('#').unwrap_or(s);
        let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0);
        match s.len() {
            6 => Color::rgb(byte(0), byte(2), byte(4)),
            8 => Color::rgba(byte(0), byte(2), byte(4), byte(6)),
            _ => Color::BLACK,
        }
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

/// The matplotlib default property cycle — the `tab10` palette (`C0`..`C9`).
/// Plot elements take these colors in order unless an explicit color is set.
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

/// The `n`-th color of the default cycle (wraps after 10), like matplotlib `CN`.
#[must_use]
pub fn cycle_color(n: usize) -> Color {
    TAB10[n % TAB10.len()]
}
