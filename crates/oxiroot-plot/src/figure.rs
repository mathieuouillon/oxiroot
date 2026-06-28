//! The `Figure` — the top-level canvas owning one or more axes and writing the
//! result to PNG or SVG (chosen by the output file extension).

use std::path::Path;

use crate::axes::Axes;
use crate::color::Color;
use crate::draw::DrawGroup;
use crate::error::{Error, Result};
use crate::render::{raster, svg};
use crate::style::Style;

/// The top-level figure.
pub struct Figure {
    style: Style,
    axes: Vec<Axes>,
}

impl Figure {
    /// A new figure with the default (matplotlib) style.
    #[must_use]
    pub fn new() -> Self {
        Figure {
            style: Style::default(),
            axes: Vec::new(),
        }
    }

    /// A new figure with a custom style.
    #[must_use]
    pub fn with_style(style: Style) -> Self {
        Figure {
            style,
            axes: Vec::new(),
        }
    }

    /// Add an axes and return the figure (builder style).
    #[must_use]
    pub fn with(mut self, ax: Axes) -> Self {
        self.axes.push(ax);
        self
    }

    /// Add an axes by mutable reference.
    pub fn add(&mut self, ax: Axes) -> &mut Self {
        self.axes.push(ax);
        self
    }

    /// Render every axes and save to `path` (`.png` or `.svg`).
    pub fn savefig(&self, path: impl AsRef<Path>) -> Result<()> {
        let (w, h) = self.style.figsize_px();
        let mut groups: Vec<DrawGroup> = Vec::new();
        for ax in &self.axes {
            groups.extend(ax.render(w, h));
        }
        write_groups(&groups, w, h, self.style.face_color, path.as_ref())
    }
}

impl Default for Figure {
    fn default() -> Self {
        Figure::new()
    }
}

/// Create a figure and a single axes sharing the default style (matplotlib's
/// `subplots()`).
#[must_use]
pub fn subplots() -> (Figure, Axes) {
    let style = Style::default();
    (Figure::with_style(style.clone()), Axes::with_style(style))
}

/// Like [`subplots`] but with a custom style (e.g. [`Style::mplhep`]).
#[must_use]
pub fn subplots_with(style: Style) -> (Figure, Axes) {
    (Figure::with_style(style.clone()), Axes::with_style(style))
}

/// Render draw groups and write the chosen image format to `path`.
pub(crate) fn write_groups(
    groups: &[DrawGroup],
    w: u32,
    h: u32,
    bg: Color,
    path: &Path,
) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => {
            let bytes = raster::render_png(groups, w, h, bg)?;
            std::fs::write(path, bytes)?;
        }
        "svg" => {
            let s = svg::render(groups, w, h, bg);
            std::fs::write(path, s)?;
        }
        other => return Err(Error::UnknownFormat(other.to_string())),
    }
    Ok(())
}
