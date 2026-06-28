//! `GridSpec` — divide a figure into a grid of cells for multi-panel layouts,
//! with optional per-row/column size ratios and inter-cell spacing.
//!
//! The cell geometry follows matplotlib's `GridSpecBase.get_grid_positions`:
//! `hspace`/`wspace` are gaps expressed as a fraction of the *average* cell size,
//! and `height_ratios`/`width_ratios` scale the rows/columns to fill the area
//! between the figure margins.

use crate::draw::Rect;

/// A grid layout for placing axes within a figure.
#[derive(Debug, Clone)]
pub struct GridSpec {
    /// Number of rows.
    pub nrows: usize,
    /// Number of columns.
    pub ncols: usize,
    /// Relative row heights (length `nrows`).
    pub height_ratios: Vec<f32>,
    /// Relative column widths (length `ncols`).
    pub width_ratios: Vec<f32>,
    /// Vertical gap as a fraction of the average cell height.
    pub hspace: f32,
    /// Horizontal gap as a fraction of the average cell width.
    pub wspace: f32,
    /// Figure margins `(left, right, bottom, top)` as fractions (matplotlib
    /// `subplotpars`, measured from the bottom-left).
    pub margins: (f32, f32, f32, f32),
}

impl GridSpec {
    /// A grid of `nrows × ncols` equal cells with default spacing/margins.
    ///
    /// # Examples
    /// ```
    /// use oxiroot_plot::{subplots_grid_with, GridSpec};
    /// // Two stacked panels, the top three times taller than the bottom.
    /// let grid = GridSpec::new(2, 1).height_ratios(vec![3.0, 1.0]).hspace(0.0);
    /// let (fig, axes) = subplots_grid_with(Default::default(), grid);
    /// assert_eq!(axes.len(), 2);
    /// ```
    #[must_use]
    pub fn new(nrows: usize, ncols: usize) -> Self {
        let nrows = nrows.max(1);
        let ncols = ncols.max(1);
        GridSpec {
            nrows,
            ncols,
            height_ratios: vec![1.0; nrows],
            width_ratios: vec![1.0; ncols],
            hspace: 0.2,
            wspace: 0.2,
            margins: (0.135, 0.93, 0.145, 0.91),
        }
    }

    /// Set the relative row heights (top to bottom).
    #[must_use]
    pub fn height_ratios(mut self, ratios: impl Into<Vec<f32>>) -> Self {
        self.height_ratios = ratios.into();
        self
    }
    /// Set the relative column widths (left to right).
    #[must_use]
    pub fn width_ratios(mut self, ratios: impl Into<Vec<f32>>) -> Self {
        self.width_ratios = ratios.into();
        self
    }
    /// Set the vertical gap (fraction of the average cell height).
    #[must_use]
    pub fn hspace(mut self, hspace: f32) -> Self {
        self.hspace = hspace;
        self
    }
    /// Set the horizontal gap (fraction of the average cell width).
    #[must_use]
    pub fn wspace(mut self, wspace: f32) -> Self {
        self.wspace = wspace;
        self
    }
    /// Set the figure margins `(left, right, bottom, top)`.
    #[must_use]
    pub fn margins(mut self, margins: (f32, f32, f32, f32)) -> Self {
        self.margins = margins;
        self
    }

    /// Per-cell `(start, end)` offsets along one axis, measured from the
    /// reference edge (top for rows, left for columns), in figure fractions.
    fn spans(n: usize, ratios: &[f32], spacing: f32, total: f32) -> Vec<(f32, f32)> {
        let n = n.max(1);
        let cell = total / (n as f32 + spacing * (n as f32 - 1.0)).max(f32::EPSILON);
        let sep = spacing * cell;
        let sum: f32 = ratios
            .iter()
            .copied()
            .take(n)
            .sum::<f32>()
            .max(f32::EPSILON);
        let norm = cell * n as f32 / sum;
        let mut cum = 0.0;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if i > 0 {
                cum += sep;
            }
            let start = cum;
            let size = ratios.get(i).copied().unwrap_or(1.0) * norm;
            cum += size;
            out.push((start, cum));
        }
        out
    }

    /// The pixel rectangle of the subplot spanning rows `r0..=r1` and columns
    /// `c0..=c1` (inclusive), within a figure of `fig_w × fig_h` pixels.
    #[must_use]
    pub fn cell_box(
        &self,
        fig_w: f32,
        fig_h: f32,
        r0: usize,
        r1: usize,
        c0: usize,
        c1: usize,
    ) -> Rect {
        let (l, r, b, t) = self.margins;
        let rows = Self::spans(self.nrows, &self.height_ratios, self.hspace, t - b);
        let cols = Self::spans(self.ncols, &self.width_ratios, self.wspace, r - l);
        let r0 = r0.min(self.nrows - 1);
        let r1 = r1.clamp(r0, self.nrows - 1);
        let c0 = c0.min(self.ncols - 1);
        let c1 = c1.clamp(c0, self.ncols - 1);

        // Rows are measured downward from `top`; columns rightward from `left`.
        let fig_top = t - rows[r0].0;
        let fig_bottom = t - rows[r1].1;
        let fig_left = l + cols[c0].0;
        let fig_right = l + cols[c1].1;

        Rect::new(
            fig_left * fig_w,
            (1.0 - fig_top) * fig_h,
            (fig_right - fig_left) * fig_w,
            (fig_top - fig_bottom) * fig_h,
        )
    }
}
