//! Colormaps for the TH2 heatmap.

use std::fmt;
use std::str::FromStr;

use crate::cmap_data::{PLASMA, VIRIDIS};
use crate::color::Color;

/// A perceptual or sequential colormap mapping a normalized value to a color.
///
/// The variant names mirror matplotlib's, including the `_r` reversed-map
/// convention ([`Colormap::GrayR`] = matplotlib `gray_r`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colormap {
    /// matplotlib's default perceptually-uniform map (purple→green→yellow).
    #[default]
    Viridis,
    /// matplotlib `plasma` (purple→orange→yellow).
    Plasma,
    /// Linear grayscale (black→white).
    Gray,
    /// Reversed grayscale (white→black), matplotlib `gray_r`.
    GrayR,
}

fn lut(table: &[[u8; 3]; 256], t: f64) -> Color {
    let x = (t.clamp(0.0, 1.0) * 255.0) as f32;
    let i = x.floor() as usize;
    let f = x - i as f32;
    let a = table[i.min(255)];
    let b = table[(i + 1).min(255)];
    let mix = |p: u8, q: u8| (p as f32 * (1.0 - f) + q as f32 * f).round() as u8;
    Color::rgb(mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2]))
}

impl Colormap {
    /// Sample the colormap at `t` ∈ [0, 1] (clamped).
    #[must_use]
    pub fn sample(self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        match self {
            Colormap::Viridis => lut(&VIRIDIS, t),
            Colormap::Plasma => lut(&PLASMA, t),
            Colormap::Gray => {
                let v = (t * 255.0).round() as u8;
                Color::rgb(v, v, v)
            }
            Colormap::GrayR => {
                let v = ((1.0 - t) * 255.0).round() as u8;
                Color::rgb(v, v, v)
            }
        }
    }

    /// The matplotlib colormap name (`"viridis"`, `"plasma"`, `"gray"`, `"gray_r"`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Colormap::Viridis => "viridis",
            Colormap::Plasma => "plasma",
            Colormap::Gray => "gray",
            Colormap::GrayR => "gray_r",
        }
    }
}

impl fmt::Display for Colormap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Parse a matplotlib colormap name, e.g. `"viridis"` or `"gray_r"`.
impl FromStr for Colormap {
    type Err = ParseColormapError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "viridis" => Ok(Colormap::Viridis),
            "plasma" => Ok(Colormap::Plasma),
            "gray" | "grey" => Ok(Colormap::Gray),
            "gray_r" | "grey_r" => Ok(Colormap::GrayR),
            _ => Err(ParseColormapError(s.to_string())),
        }
    }
}

/// The error returned when a string cannot be parsed as a [`Colormap`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColormapError(String);

impl fmt::Display for ParseColormapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown colormap `{}`", self.0)
    }
}

impl std::error::Error for ParseColormapError {}
