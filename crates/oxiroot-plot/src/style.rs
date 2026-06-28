//! Plot style — the matplotlib default `rcParams` translated to a struct.
//!
//! All sizes are stored in **points** (matplotlib's unit); convert to device
//! pixels with [`Style::px`] (`pt * dpi / 72`). The defaults reproduce a plain
//! matplotlib figure: 6.4×4.8 in at 100 dpi, DejaVu Sans 10 pt, a black 0.8 pt
//! rectangular frame, out-pointing major ticks on the bottom and left, the
//! `tab10` color cycle, no grid, no minor ticks, and a 5 % data margin.

use crate::color::{Color, TAB10};

/// Tick direction (matplotlib `xtick.direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickDir {
    /// Ticks point outward from the axes (matplotlib default).
    #[default]
    Out,
    /// Ticks point inward (HEP convention).
    In,
    /// Ticks straddle the spine.
    InOut,
}

/// Which spines/ticks are drawn on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sides {
    /// Left side.
    pub left: bool,
    /// Right side.
    pub right: bool,
    /// Bottom side.
    pub bottom: bool,
    /// Top side.
    pub top: bool,
}

/// A full style configuration.
#[derive(Debug, Clone)]
pub struct Style {
    /// Figure size in inches `(width, height)`.
    pub figsize_in: (f32, f32),
    /// Dots per inch.
    pub dpi: f32,
    /// Figure/axes background.
    pub face_color: Color,
    /// Spine, tick, and text color.
    pub fg_color: Color,
    /// Base font size (pt).
    pub font_size_pt: f32,
    /// Title font size (pt).
    pub title_size_pt: f32,
    /// Axis-label font size (pt).
    pub label_size_pt: f32,
    /// Tick-label font size (pt).
    pub tick_label_size_pt: f32,
    /// Legend font size (pt).
    pub legend_size_pt: f32,
    /// Spine / frame line width (pt).
    pub axes_linewidth_pt: f32,
    /// Major tick length (pt).
    pub tick_major_len_pt: f32,
    /// Minor tick length (pt).
    pub tick_minor_len_pt: f32,
    /// Major tick width (pt).
    pub tick_major_width_pt: f32,
    /// Tick direction.
    pub tick_dir: TickDir,
    /// Which sides draw ticks.
    pub tick_sides: Sides,
    /// Whether minor ticks are shown.
    pub minor_ticks: bool,
    /// Gap between a tick and its label (pt).
    pub tick_pad_pt: f32,
    /// Default data line width (pt).
    pub line_width_pt: f32,
    /// Default marker size (pt).
    pub marker_size_pt: f32,
    /// Fractional padding added to the data range on each side.
    pub margin: f32,
    /// Whether the grid is drawn.
    pub grid: bool,
    /// Grid color.
    pub grid_color: Color,
    /// Grid line width (pt).
    pub grid_width_pt: f32,
    /// Color cycle.
    pub color_cycle: Vec<Color>,
    /// Subplot margins as figure fractions `(left, right, bottom, top)`.
    pub margins_frac: (f32, f32, f32, f32),
    /// Whether the legend draws a frame box.
    pub legend_frame: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            figsize_in: (6.4, 4.8),
            dpi: 100.0,
            face_color: Color::WHITE,
            fg_color: Color::BLACK,
            font_size_pt: 10.0,
            title_size_pt: 12.0,
            label_size_pt: 10.0,
            tick_label_size_pt: 10.0,
            legend_size_pt: 10.0,
            axes_linewidth_pt: 0.8,
            tick_major_len_pt: 3.5,
            tick_minor_len_pt: 2.0,
            tick_major_width_pt: 0.8,
            tick_dir: TickDir::Out,
            tick_sides: Sides {
                left: true,
                right: false,
                bottom: true,
                top: false,
            },
            minor_ticks: false,
            tick_pad_pt: 3.5,
            line_width_pt: 1.5,
            marker_size_pt: 6.0,
            margin: 0.05,
            grid: false,
            grid_color: Color::rgb(0xb0, 0xb0, 0xb0),
            grid_width_pt: 0.8,
            color_cycle: TAB10.to_vec(),
            margins_frac: (0.125, 0.9, 0.11, 0.88),
            legend_frame: true,
        }
    }
}

impl Style {
    /// Convert a point size to device pixels at this style's dpi.
    #[must_use]
    pub fn px(&self, pt: f32) -> f32 {
        pt * self.dpi / 72.0
    }

    /// Figure size in device pixels `(width, height)`.
    #[must_use]
    pub fn figsize_px(&self) -> (u32, u32) {
        (
            (self.figsize_in.0 * self.dpi).round() as u32,
            (self.figsize_in.1 * self.dpi).round() as u32,
        )
    }

    /// The `n`-th cycle color (wraps).
    #[must_use]
    pub fn cycle(&self, n: usize) -> Color {
        self.color_cycle[n % self.color_cycle.len().max(1)]
    }

    /// An mplhep-flavored variant: in-pointing ticks on all four sides, minor
    /// ticks on, a slightly heavier frame, and no data margin on x. The plot
    /// look stays matplotlib; histograms are always drawn as mplhep staircases.
    #[must_use]
    pub fn mplhep() -> Self {
        Style {
            tick_dir: TickDir::In,
            tick_sides: Sides {
                left: true,
                right: true,
                bottom: true,
                top: true,
            },
            minor_ticks: true,
            axes_linewidth_pt: 1.0,
            legend_frame: false,
            ..Style::default()
        }
    }
}
