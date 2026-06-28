//! Colormaps for the TH2 heatmap.

use crate::cmap_data::{PLASMA, VIRIDIS};
use crate::color::Color;

/// A perceptual or sequential colormap mapping a normalized value to a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colormap {
    /// matplotlib's default perceptually-uniform map (purple→green→yellow).
    #[default]
    Viridis,
    /// matplotlib `plasma` (purple→orange→yellow).
    Plasma,
    /// Linear grayscale (black→white).
    Gray,
    /// Reversed grayscale (white→black).
    GrayReversed,
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
            Colormap::GrayReversed => {
                let v = ((1.0 - t) * 255.0).round() as u8;
                Color::rgb(v, v, v)
            }
        }
    }
}
