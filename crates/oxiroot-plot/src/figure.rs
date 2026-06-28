//! The `Figure` — the top-level canvas. Holds one or more axes laid out by a
//! [`GridSpec`] and writes the result to PNG, SVG, or PDF (chosen by the output
//! file extension).

use std::path::Path;

use crate::axes::Axes;
use crate::color::Color;
use crate::draw::DrawGroup;
use crate::error::{Error, Result};
use crate::gridspec::GridSpec;
use crate::render::{pdf, raster, svg};
use crate::style::Style;

/// Options for saving a figure.
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// Override the dots-per-inch (raster resolution). `None` uses the style's
    /// dpi (100). Higher values give a sharper PNG; vector outputs (SVG/PDF) are
    /// resolution-independent but their coordinate scale follows the dpi.
    pub dpi: Option<f32>,
    /// Render with a transparent background (no opaque page fill).
    pub transparent: bool,
}

impl SaveOptions {
    /// New default options.
    #[must_use]
    pub fn new() -> Self {
        SaveOptions::default()
    }
    /// Set the output DPI (raster resolution).
    #[must_use]
    pub fn dpi(mut self, dpi: f32) -> Self {
        self.dpi = Some(dpi);
        self
    }
    /// Render with a transparent background.
    #[must_use]
    pub fn transparent(mut self, on: bool) -> Self {
        self.transparent = on;
        self
    }
}

/// The top-level figure: a grid of axes panels.
pub struct Figure {
    style: Style,
    grid: GridSpec,
    axes: Vec<Axes>,
    sharex: bool,
}

impl Figure {
    /// A new figure with the default style and a 1×1 grid.
    #[must_use]
    pub fn new() -> Self {
        Figure::with_style(Style::default())
    }

    /// A new figure with a custom style and a 1×1 grid.
    #[must_use]
    pub fn with_style(style: Style) -> Self {
        let grid = GridSpec::new(1, 1).margins(style.margins_frac);
        Figure {
            style,
            grid,
            axes: Vec::new(),
            sharex: false,
        }
    }

    fn with_grid(style: Style, grid: GridSpec, sharex: bool) -> Self {
        Figure {
            style,
            grid,
            axes: Vec::new(),
            sharex,
        }
    }

    /// Add a single axes (placed in the first cell).
    #[must_use]
    pub fn with(mut self, ax: Axes) -> Self {
        self.axes.push(ax);
        self
    }

    /// Add a single axes by mutable reference.
    pub fn add(&mut self, ax: Axes) -> &mut Self {
        self.axes.push(ax);
        self
    }

    /// Place a list of axes into the grid, row-major. When the figure shares its
    /// x-axis (e.g. a ratio plot), the panels are given a common x range and the
    /// upper panels' x tick labels are hidden.
    #[must_use]
    pub fn with_axes(mut self, mut axes: Vec<Axes>) -> Self {
        if self.sharex && !axes.is_empty() {
            // A common x range = the union of every panel's resolved x-limits.
            let (mut lo, mut hi) = axes[0].resolved_xlim();
            for ax in &axes[1..] {
                let (l, h) = ax.resolved_xlim();
                lo = lo.min(l);
                hi = hi.max(h);
            }
            let ncols = self.grid.ncols.max(1);
            let last_row = axes.len().saturating_sub(1) / ncols;
            for (i, ax) in axes.iter_mut().enumerate() {
                ax.set_xlim(lo, hi);
                // Only the lowest row keeps its x tick labels + x-axis label.
                if i / ncols < last_row {
                    ax.set_xticklabels_visible(false);
                }
            }
        }
        self.axes = axes;
        self
    }

    /// Convenience for a ratio plot: place the main and ratio panels.
    #[must_use]
    pub fn ratio(self, main: Axes, ratio: Axes) -> Self {
        self.with_axes(vec![main, ratio])
    }

    /// Render every panel and save to `path` (`.png`, `.svg`, or `.pdf`).
    pub fn savefig(&self, path: impl AsRef<Path>) -> Result<()> {
        self.savefig_with(path, &SaveOptions::default())
    }

    /// Render and save with explicit options (DPI, transparency).
    pub fn savefig_with(&self, path: impl AsRef<Path>, opts: &SaveOptions) -> Result<()> {
        let dpi = opts.dpi.unwrap_or(self.style.dpi);
        let w = (self.style.figsize_in.0 * dpi).round() as u32;
        let h = (self.style.figsize_in.1 * dpi).round() as u32;
        let ncols = self.grid.ncols.max(1);

        let mut groups: Vec<DrawGroup> = Vec::new();
        for (i, ax) in self.axes.iter().enumerate() {
            let (row, col) = (i / ncols, i % ncols);
            let box_ = self.grid.cell_box(w as f32, h as f32, row, row, col, col);
            // Propagate the effective dpi to the panel's style.
            let mut tmp;
            let axr = if (dpi - ax.style.dpi).abs() > f32::EPSILON {
                tmp = ax.clone();
                tmp.style.dpi = dpi;
                &tmp
            } else {
                ax
            };
            groups.extend(axr.render_at(box_));
        }
        let bg = if opts.transparent {
            Color::TRANSPARENT
        } else {
            self.style.face_color
        };
        write_groups(&groups, w, h, bg, path.as_ref())
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

/// Create a figure with an `nrows × ncols` grid of axes (row-major), matplotlib
/// `subplots(nrows, ncols)`. Fill the returned axes, then `fig.with_axes(axes)`.
#[must_use]
pub fn subplots_grid(nrows: usize, ncols: usize) -> (Figure, Vec<Axes>) {
    subplots_grid_with(Style::default(), GridSpec::new(nrows, ncols))
}

/// Like [`subplots_grid`] with a custom style and [`GridSpec`].
#[must_use]
pub fn subplots_grid_with(style: Style, grid: GridSpec) -> (Figure, Vec<Axes>) {
    let n = grid.nrows * grid.ncols;
    let grid = if grid.margins == GridSpec::new(1, 1).margins {
        grid.margins(style.margins_frac)
    } else {
        grid
    };
    let axes = (0..n).map(|_| Axes::with_style(style.clone())).collect();
    (Figure::with_grid(style, grid, false), axes)
}

/// A two-panel ratio plot: a main panel over a shorter ratio panel sharing the
/// x-axis (height ratios 3:1, touching). Returns `(figure, main, ratio)`; fill
/// both panels, then `fig.ratio(main, ratio).savefig(...)`.
#[must_use]
pub fn ratio_subplots() -> (Figure, Axes, Axes) {
    ratio_subplots_with(Style::default())
}

/// Like [`ratio_subplots`] with a custom style.
#[must_use]
pub fn ratio_subplots_with(style: Style) -> (Figure, Axes, Axes) {
    let grid = GridSpec::new(2, 1)
        .height_ratios(vec![3.0, 1.0])
        .hspace(0.0)
        .margins(style.margins_frac);
    let fig = Figure::with_grid(style.clone(), grid, true);
    let main = Axes::with_style(style.clone());
    let mut ratio = Axes::with_style(style);
    // Drop the ratio panel's top y label so it doesn't collide with the main
    // panel's bottom label at the shared seam.
    ratio.prune_ylabels(false, true);
    (fig, main, ratio)
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
        "png" => std::fs::write(path, raster::render_png(groups, w, h, bg)?)?,
        "svg" => std::fs::write(path, svg::render(groups, w, h, bg))?,
        "pdf" => std::fs::write(path, pdf::render(groups, w, h, bg))?,
        other => return Err(Error::UnknownFormat(other.to_string())),
    }
    Ok(())
}
