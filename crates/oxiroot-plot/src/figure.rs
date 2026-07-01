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

/// Options for saving a figure. Build with [`SaveOpts::new`] and the chained
/// setters.
///
/// # Examples
/// ```
/// use oxiroot_plot::SaveOpts;
/// let opts = SaveOpts::new().dpi(300.0).transparent();
/// ```
#[derive(Debug, Clone, Default)]
pub struct SaveOpts {
    pub(crate) dpi: Option<f32>,
    pub(crate) transparent: bool,
}

impl SaveOpts {
    /// New default options.
    #[must_use]
    pub fn new() -> Self {
        SaveOpts::default()
    }
    /// Set the output DPI (raster resolution). `None` uses the style's dpi (100).
    /// Higher values give a sharper PNG; vector outputs (SVG/PDF) are
    /// resolution-independent but their coordinate scale follows the dpi.
    #[must_use]
    pub fn dpi(mut self, dpi: f32) -> Self {
        self.dpi = Some(dpi);
        self
    }
    /// Render with a transparent background (no opaque page fill).
    #[must_use]
    pub fn transparent(mut self) -> Self {
        self.transparent = true;
        self
    }
}

/// The top-level figure: a grid of axes panels.
pub struct Figure {
    style: Style,
    grid: GridSpec,
    axes: Vec<Axes>,
    sharex: bool,
    sharey: bool,
    suptitle: Option<String>,
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
            sharey: false,
            suptitle: None,
        }
    }

    fn with_grid(style: Style, grid: GridSpec, sharex: bool, sharey: bool) -> Self {
        Figure {
            style,
            grid,
            axes: Vec::new(),
            sharex,
            sharey,
            suptitle: None,
        }
    }

    /// Share the x-axis across all panels (common x range; only the bottom row
    /// keeps its x tick labels and x-axis label).
    #[must_use]
    pub fn sharex(mut self) -> Self {
        self.sharex = true;
        self
    }

    /// Share the y-axis across all panels (common y range; only the left column
    /// keeps its y tick labels and y-axis label).
    #[must_use]
    pub fn sharey(mut self) -> Self {
        self.sharey = true;
        self
    }

    /// Set a figure-level title centered above the panels (supports `$…$` math).
    #[must_use]
    pub fn suptitle(mut self, s: impl Into<String>) -> Self {
        self.suptitle = Some(s.into());
        self
    }

    /// Place axes into the grid, row-major. Accepts anything iterable — a single
    /// `[ax]`, a `Vec<Axes>`, etc. With a shared x- or y-axis the panels are
    /// given a common range and the inner tick labels are hidden (only the bottom
    /// row keeps x labels, only the left column keeps y labels).
    #[must_use]
    pub fn with_axes(mut self, axes: impl IntoIterator<Item = Axes>) -> Self {
        let mut axes: Vec<Axes> = axes.into_iter().collect();
        let ncols = self.grid.ncols.max(1);
        if self.sharex && !axes.is_empty() {
            let (mut lo, mut hi) = axes[0].resolved_xlim();
            for ax in &axes[1..] {
                let (l, h) = ax.resolved_xlim();
                lo = lo.min(l);
                hi = hi.max(h);
            }
            let last_row = axes.len().saturating_sub(1) / ncols;
            for (i, ax) in axes.iter_mut().enumerate() {
                ax.xlim(lo..hi);
                if i / ncols < last_row {
                    ax.hide_xticklabels();
                }
            }
        }
        if self.sharey && !axes.is_empty() {
            let (mut lo, mut hi) = axes[0].resolved_ylim();
            for ax in &axes[1..] {
                let (l, h) = ax.resolved_ylim();
                lo = lo.min(l);
                hi = hi.max(h);
            }
            for (i, ax) in axes.iter_mut().enumerate() {
                ax.ylim(lo..hi);
                if i % ncols != 0 {
                    ax.hide_yticklabels();
                }
            }
        }
        self.axes = axes;
        self
    }

    /// Convenience for a ratio plot: place the main and ratio panels.
    #[must_use]
    pub fn ratio(self, main: Axes, ratio: Axes) -> Self {
        self.with_axes([main, ratio])
    }

    /// Render every panel and save to `path` (`.png`, `.svg`, or `.pdf`).
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.save_with(path, SaveOpts::default())
    }

    /// Render and save with explicit options (DPI, transparency).
    pub fn save_with(&self, path: impl AsRef<Path>, opts: SaveOpts) -> Result<()> {
        let (groups, w, h) = self.render_groups(opts.dpi);
        write_groups(
            &groups,
            w,
            h,
            self.background(opts.transparent),
            path.as_ref(),
        )
    }

    /// Render to an in-memory PNG (honoring [`SaveOpts`] DPI/transparency) instead
    /// of writing a file.
    pub fn to_png_bytes(&self, opts: SaveOpts) -> Result<Vec<u8>> {
        let (groups, w, h) = self.render_groups(opts.dpi);
        raster::render_png(&groups, w, h, self.background(opts.transparent))
    }

    /// Render to an in-memory SVG string.
    #[must_use]
    pub fn to_svg_string(&self) -> String {
        let (groups, w, h) = self.render_groups(None);
        svg::render(&groups, w, h, self.background(false))
    }

    /// Render to in-memory PDF bytes.
    #[must_use]
    pub fn to_pdf_bytes(&self) -> Vec<u8> {
        let (groups, w, h) = self.render_groups(None);
        pdf::render(&groups, w, h, self.background(false))
    }

    /// The page background color (`face_color`, or fully transparent).
    fn background(&self, transparent: bool) -> Color {
        if transparent {
            Color::TRANSPARENT
        } else {
            self.style.face_color
        }
    }

    /// Render all panels (and the suptitle) into draw groups at the effective
    /// dpi, returning them plus the pixel size used.
    fn render_groups(&self, dpi: Option<f32>) -> (Vec<DrawGroup>, u32, u32) {
        let dpi = dpi.unwrap_or(self.style.dpi);
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

        // Figure-level title, centered near the top of the figure.
        if let Some(text) = &self.suptitle {
            let s = &self.style;
            let size = s.title_size_pt * 1.1 * dpi / 72.0;
            let y = (1.0 - s.margins_frac.3) * h as f32 * 0.45;
            let mut g = DrawGroup::new(None);
            crate::mathtext::layout_label(
                &mut g,
                &s.fonts,
                text,
                w as f32 / 2.0,
                y,
                size,
                s.fg_color,
                crate::text::HAlign::Center,
                crate::text::VAlign::Middle,
                0.0,
            );
            groups.push(g);
        }
        (groups, w, h)
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
///
/// # Examples
/// ```no_run
/// use oxiroot_plot::subplots_grid;
/// let (fig, mut axs) = subplots_grid(1, 2);
/// axs[0].plot(&[0.0, 1.0], &[0.0, 1.0]);
/// axs[1].plot(&[0.0, 1.0], &[1.0, 0.0]);
/// fig.with_axes(axs).save("two.png").unwrap();
/// ```
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
    (Figure::with_grid(style, grid, false, false), axes)
}

/// A two-panel ratio plot: a main panel over a shorter ratio panel sharing the
/// x-axis (height ratios 3:1, touching). Returns `(figure, main, ratio)`; fill
/// both panels, then `fig.ratio(main, ratio).save(...)`.
///
/// # Examples
/// ```no_run
/// use oxiroot_plot::ratio_subplots;
/// use oxiroot_hist::Hist;
/// let h = Hist::reg(20, 0.0, 10.0).double().named("h");
/// let (fig, mut main, mut ratio) = ratio_subplots();
/// main.hist(&h);
/// main.ylabel("Events");
/// ratio.ylim(0.5..1.5);
/// ratio.ylabel("data/MC");
/// ratio.xlabel("x");
/// fig.ratio(main, ratio).save("ratio.svg").unwrap();
/// ```
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
    let fig = Figure::with_grid(style.clone(), grid, true, false);
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
