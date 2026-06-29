//! The `Axes` — a single plot panel: data limits, the frame, ticks, labels, and
//! the artists drawn within it. Mirrors matplotlib's `Axes` API.

use std::ops::Range;

use oxiroot_hist::{GraphErrors, TGraph, TProfile, TH1, TH2};

use crate::artists::{
    Artist, ErrorbarArtist, HistType, LineArtist, Marker, MeshArtist, StepArtist,
};
use crate::cmap::Colormap;
use crate::color::Color;
use crate::colorbar::ColorbarSpec;
use crate::draw::{DrawCommand, DrawGroup, Rect, Stroke};
use crate::figure::SaveOpts;
use crate::style::{Style, TickDir};
use crate::text::{self, FontStyle, HAlign, VAlign};
use crate::ticker;
use crate::transform::{Bounds, Transform};

/// A single plot panel.
#[derive(Clone)]
pub struct Axes {
    pub(crate) style: Style,
    xlim: Option<(f64, f64)>,
    ylim: Option<(f64, f64)>,
    xlabel: Option<String>,
    ylabel: Option<String>,
    title: Option<String>,
    pub(crate) artists: Vec<Artist>,
    color_idx: usize,
    show_legend: bool,
    /// When set (by `hist`/`hist_with`), the y autoscale starts at 0.
    pub(crate) y_from_zero: bool,
    colorbar: Option<ColorbarSpec>,
    grid_minor: bool,
    /// Whether the x tick labels and x-axis label are drawn (hidden on the upper
    /// panel of a shared-x layout such as a ratio plot).
    show_xticklabels: bool,
    /// Whether the y tick labels and y-axis label are drawn (hidden on the
    /// non-left columns of a shared-y grid).
    show_yticklabels: bool,
    /// Drop the `(bottom, top)` y tick labels (avoids overlap at a shared seam).
    ylabel_prune: (bool, bool),
}

impl Axes {
    /// A new empty axes with the given style.
    #[must_use]
    pub fn with_style(style: Style) -> Self {
        Axes {
            style,
            xlim: None,
            ylim: None,
            xlabel: None,
            ylabel: None,
            title: None,
            artists: Vec::new(),
            color_idx: 0,
            show_legend: false,
            y_from_zero: false,
            colorbar: None,
            grid_minor: false,
            show_xticklabels: true,
            show_yticklabels: true,
            ylabel_prune: (false, false),
        }
    }

    /// Drop the bottom-most and/or top-most y tick label (used to avoid overlap
    /// at the shared seam of a ratio plot).
    pub(crate) fn prune_ylabels(&mut self, bottom: bool, top: bool) {
        self.ylabel_prune = (bottom, top);
    }

    /// A new empty axes with the default (matplotlib) style.
    #[must_use]
    pub fn new() -> Self {
        Axes::with_style(Style::default())
    }

    /// Next color from the style's cycle (advances the cycle).
    pub(crate) fn next_color(&mut self) -> Color {
        let c = self.style.cycle(self.color_idx);
        self.color_idx += 1;
        c
    }

    pub(crate) fn add_artist(&mut self, a: Artist) {
        self.artists.push(a);
    }

    pub(crate) fn legend_items(&self) -> Vec<crate::artists::LegendItem> {
        self.artists.iter().filter_map(Artist::legend).collect()
    }

    /// Set the x-axis label (supports `$…$` math).
    pub fn xlabel(&mut self, s: impl Into<String>) -> &mut Self {
        self.xlabel = Some(s.into());
        self
    }
    /// Set the y-axis label.
    pub fn ylabel(&mut self, s: impl Into<String>) -> &mut Self {
        self.ylabel = Some(s.into());
        self
    }
    /// Set the title.
    pub fn title(&mut self, s: impl Into<String>) -> &mut Self {
        self.title = Some(s.into());
        self
    }
    /// Set the x-axis limits, e.g. `ax.xlim(0.0..100.0)`.
    pub fn xlim(&mut self, range: Range<f64>) -> &mut Self {
        self.xlim = Some((range.start, range.end));
        self
    }
    /// Set the y-axis limits, e.g. `ax.ylim(0.5..1.5)`.
    pub fn ylim(&mut self, range: Range<f64>) -> &mut Self {
        self.ylim = Some((range.start, range.end));
        self
    }
    /// Enable the legend.
    pub fn legend(&mut self) -> &mut Self {
        self.show_legend = true;
        self
    }

    /// Show a matplotlib-style grid at the major tick positions (light grey
    /// solid lines behind the data).
    ///
    /// # Examples
    /// ```
    /// # use oxiroot_plot::Axes;
    /// let mut ax = Axes::new();
    /// ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]);
    /// ax.grid();
    /// ```
    pub fn grid(&mut self) -> &mut Self {
        self.style.grid = true;
        self
    }

    /// Also show fainter grid lines at the minor tick positions (implies the
    /// major grid and minor ticks).
    pub fn grid_minor(&mut self) -> &mut Self {
        self.grid_minor = true;
        self.style.grid = true;
        self.style.minor_ticks = true;
        self
    }

    /// Hide the x tick labels and the x-axis label (e.g. on the upper panel of a
    /// shared-x layout). The tick marks themselves stay.
    pub fn hide_xticklabels(&mut self) -> &mut Self {
        self.show_xticklabels = false;
        self
    }

    /// Hide the y tick labels and the y-axis label (e.g. on the non-left columns
    /// of a shared-y grid). The tick marks themselves stay.
    pub fn hide_yticklabels(&mut self) -> &mut Self {
        self.show_yticklabels = false;
        self
    }

    /// The resolved x-axis limits (for sharing the x-axis across panels).
    #[must_use]
    pub(crate) fn resolved_xlim(&self) -> (f64, f64) {
        let (xmin, xmax, _, _) = self.limits();
        (xmin, xmax)
    }

    /// The resolved y-axis limits (for sharing the y-axis across panels).
    #[must_use]
    pub(crate) fn resolved_ylim(&self) -> (f64, f64) {
        let (_, _, ymin, ymax) = self.limits();
        (ymin, ymax)
    }

    /// Render this single axes as a full figure and save to `path` (`.png`,
    /// `.svg`, or `.pdf`) — the convenient path for a one-panel plot.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> crate::error::Result<()> {
        self.save_with(path, SaveOpts::default())
    }

    /// Like [`Axes::save`] with explicit options (e.g. a higher DPI for a sharper
    /// PNG, or a transparent background).
    ///
    /// # Examples
    /// ```no_run
    /// use oxiroot_plot::{Axes, SaveOpts};
    /// let mut ax = Axes::new();
    /// ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    /// ax.save_with("plot.png", SaveOpts::new().dpi(300.0)).unwrap();
    /// ```
    pub fn save_with(
        &self,
        path: impl AsRef<std::path::Path>,
        opts: SaveOpts,
    ) -> crate::error::Result<()> {
        let (groups, w, h) = self.groups(opts.dpi);
        let bg = self.background(opts.transparent);
        crate::figure::write_groups(&groups, w, h, bg, path.as_ref())
    }

    /// Render to an in-memory PNG (honoring [`SaveOpts`] DPI/transparency) instead
    /// of writing a file.
    pub fn to_png_bytes(&self, opts: SaveOpts) -> crate::error::Result<Vec<u8>> {
        let (groups, w, h) = self.groups(opts.dpi);
        crate::render::raster::render_png(&groups, w, h, self.background(opts.transparent))
    }

    /// Render to an in-memory SVG string.
    #[must_use]
    pub fn to_svg_string(&self) -> String {
        let (groups, w, h) = self.groups(None);
        crate::render::svg::render(&groups, w, h, self.background(false))
    }

    /// Render to in-memory PDF bytes.
    #[must_use]
    pub fn to_pdf_bytes(&self) -> Vec<u8> {
        let (groups, w, h) = self.groups(None);
        crate::render::pdf::render(&groups, w, h, self.background(false))
    }

    /// The page background color (`face_color`, or fully transparent).
    fn background(&self, transparent: bool) -> Color {
        if transparent {
            Color::TRANSPARENT
        } else {
            self.style.face_color
        }
    }

    /// Build the draw groups at an optional DPI override, with the pixel size used.
    fn groups(&self, dpi: Option<f32>) -> (Vec<DrawGroup>, u32, u32) {
        let mut tmp;
        let ax = match dpi {
            Some(dpi) => {
                tmp = self.clone();
                tmp.style.dpi = dpi;
                &tmp
            }
            None => self,
        };
        let (w, h) = ax.style.figsize_px();
        (ax.render(w, h), w, h)
    }

    /// Plot a `TH1` as an mplhep step staircase (the matplotlib `hist` analog).
    pub fn hist(&mut self, h: &TH1) -> &mut Self {
        self.hist_with(h, HistOpts::default())
    }

    /// Plot a `TH1` with explicit options (histtype, error bars, color, label).
    ///
    /// # Examples
    /// ```no_run
    /// use oxiroot_plot::{Axes, HistOpts, HistType};
    /// use oxiroot_hist::TH1;
    /// let h = TH1::new(20, 0.0, 10.0).named("h");
    /// let mut ax = Axes::new();
    /// ax.hist_with(&h, HistOpts::new().histtype(HistType::Step).yerr().label("MC"));
    /// ax.save("h.svg").unwrap();
    /// ```
    pub fn hist_with(&mut self, h: &TH1, opts: HistOpts) -> &mut Self {
        let edges = h.edges();
        let values = h.values().to_vec();
        let n = values.len();
        let color = opts.color.unwrap_or_else(|| self.next_color());
        let errs = if opts.yerr {
            Some((0..n).map(|i| h.bin_error(i + 1)).collect())
        } else {
            None
        };
        if n > 0 && edges.len() > n {
            let (lo, hi) = (edges[0], edges[n]);
            self.xlim = Some(match self.xlim {
                Some((a, b)) => (a.min(lo), b.max(hi)),
                None => (lo, hi),
            });
        }
        self.y_from_zero = true;
        let width = opts.linewidth_pt.unwrap_or(self.style.line_width_pt);
        let msize = self.style.marker_size_pt * 0.7;
        self.add_artist(Artist::Step(StepArtist {
            edges,
            values,
            errs,
            histtype: opts.histtype,
            color,
            fill_color: opts.fill_color,
            width_pt: width,
            marker: Marker::Circle,
            marker_size_pt: msize,
            label: opts.label,
        }));
        self
    }

    /// Plot a `TGraph` (any error variant) as data points with error bars.
    ///
    /// # Examples
    /// ```no_run
    /// use oxiroot_plot::Axes;
    /// use oxiroot_hist::TGraph;
    /// let g = TGraph::with_errors(vec![1.0, 2.0], vec![3.0, 4.0], vec![0.1, 0.1], vec![0.2, 0.3]);
    /// let mut ax = Axes::new();
    /// ax.errorbar(&g);
    /// ax.save("g.png").unwrap();
    /// ```
    pub fn errorbar(&mut self, g: &TGraph) -> &mut Self {
        self.errorbar_with(g, ErrorbarOpts::default())
    }

    /// Plot a `TGraph` with explicit options.
    pub fn errorbar_with(&mut self, g: &TGraph, opts: ErrorbarOpts) -> &mut Self {
        let n = g.len();
        let xs = g.x[..n].to_vec();
        let ys = g.y[..n].to_vec();
        let fit = |v: &[f64]| {
            let mut o = v.to_vec();
            o.resize(n, 0.0);
            o
        };
        let (xerr, yerr) = match &g.errors {
            GraphErrors::Symmetric { ex, ey } => {
                (Some((fit(ex), fit(ex))), Some((fit(ey), fit(ey))))
            }
            GraphErrors::Asymmetric {
                ex_low,
                ex_high,
                ey_low,
                ey_high,
            } => (
                Some((fit(ex_low), fit(ex_high))),
                Some((fit(ey_low), fit(ey_high))),
            ),
            _ => (None, None),
        };
        let color = opts.color.unwrap_or_else(|| self.next_color());
        self.add_artist(Artist::Errorbar(ErrorbarArtist {
            xs,
            ys,
            xerr,
            yerr,
            color,
            marker: opts.marker,
            marker_size_pt: opts
                .marker_size_pt
                .unwrap_or(self.style.marker_size_pt * 0.8),
            elinewidth_pt: 1.0,
            capsize_pt: opts.capsize_pt,
            line_width_pt: opts.line.then_some(self.style.line_width_pt),
            label: opts.label,
        }));
        self
    }

    /// Plot a `TProfile` as data points with y error bars at bin centers.
    ///
    /// # Examples
    /// ```no_run
    /// use oxiroot_plot::Axes;
    /// use oxiroot_hist::TProfile;
    /// let tp = TProfile::new(10, 0.0, 10.0).named("p");
    /// let mut ax = Axes::new();
    /// ax.profile(&tp);
    /// ax.save("profile.png").unwrap();
    /// ```
    pub fn profile(&mut self, tp: &TProfile) -> &mut Self {
        let edges = tp.edges();
        let vals = tp.values();
        let n = vals.len();
        if edges.len() < n + 1 {
            return self;
        }
        let xs: Vec<f64> = (0..n).map(|i| 0.5 * (edges[i] + edges[i + 1])).collect();
        let yerr: Vec<f64> = (0..n).map(|i| tp.bin_error(i + 1)).collect();
        let color = self.next_color();
        self.add_artist(Artist::Errorbar(ErrorbarArtist {
            xs,
            ys: vals,
            xerr: None,
            yerr: Some((yerr.clone(), yerr)),
            color,
            marker: Marker::Circle,
            marker_size_pt: self.style.marker_size_pt * 0.8,
            elinewidth_pt: 1.0,
            capsize_pt: 0.0,
            line_width_pt: None,
            label: None,
        }));
        self
    }

    /// Plot a `TH2` as a color mesh with a colorbar (matplotlib `pcolormesh`).
    pub fn hist2d(&mut self, h: &TH2) -> &mut Self {
        self.hist2d_with(h, Hist2dOpts::default())
    }

    /// Plot a `TH2` with explicit options (colormap, value range, colorbar label).
    ///
    /// # Examples
    /// ```no_run
    /// use oxiroot_plot::{Axes, Colormap, Hist2dOpts};
    /// use oxiroot_hist::TH2;
    /// let h = TH2::new(10, 0.0, 1.0, 10, 0.0, 1.0).named("h2");
    /// let mut ax = Axes::new();
    /// ax.hist2d_with(&h, Hist2dOpts::new().cmap(Colormap::Viridis).label("entries"));
    /// ax.save("h2.png").unwrap();
    /// ```
    pub fn hist2d_with(&mut self, h: &TH2, opts: Hist2dOpts) -> &mut Self {
        let xedges = h.xaxis.edges();
        let yedges = h.yaxis.edges();
        let values = h.values();
        // Autoscale over the bins that actually hold data. Empty bins (content
        // exactly 0) are not drawn — they show the page background, like ROOT's
        // COLZ — so they must not anchor the color scale at 0 either.
        let (mut dmin, mut dmax) = (f64::INFINITY, f64::NEG_INFINITY);
        for row in &values {
            for &v in row {
                if v != 0.0 {
                    dmin = dmin.min(v);
                    dmax = dmax.max(v);
                }
            }
        }
        if !dmin.is_finite() {
            // No filled bins at all.
            dmin = 0.0;
            dmax = 1.0;
        }
        let vmin = opts.vmin.unwrap_or(dmin);
        let vmax = opts.vmax.unwrap_or(dmax);
        let nx = values.len();
        let ny = values.first().map_or(0, Vec::len);
        if nx > 0 && ny > 0 && xedges.len() > nx && yedges.len() > ny {
            self.xlim = Some((xedges[0], xedges[nx]));
            self.ylim = Some((yedges[0], yedges[ny]));
        }
        self.colorbar = Some(ColorbarSpec {
            vmin,
            vmax,
            cmap: opts.cmap,
            label: opts.label,
        });
        self.add_artist(Artist::Mesh(MeshArtist {
            xedges,
            yedges,
            values,
            cmap: opts.cmap,
            vmin,
            vmax,
        }));
        self
    }

    /// Plot a connected line through `(x, y)` points (matplotlib `plot`).
    pub fn plot(&mut self, xs: &[f64], ys: &[f64]) -> &mut Self {
        let color = self.next_color();
        self.add_artist(Artist::Line(LineArtist {
            xs: xs.to_vec(),
            ys: ys.to_vec(),
            color,
            width_pt: self.style.line_width_pt,
            dash: None,
            marker: Marker::None,
            marker_size_pt: self.style.marker_size_pt,
            label: None,
        }));
        self
    }

    /// Plot a function `f` sampled over `range` as a smooth curve — e.g. to
    /// overlay a fitted model on a histogram.
    ///
    /// # Examples
    /// ```
    /// # use oxiroot_plot::Axes;
    /// let mut ax = Axes::new();
    /// // Overlay a Gaussian curve over x ∈ [0, 10].
    /// ax.function(|x| 1000.0 * (-(x - 5.0_f64).powi(2) / 2.0).exp(), 0.0..10.0);
    /// ```
    pub fn function<F: Fn(f64) -> f64>(&mut self, f: F, range: Range<f64>) -> &mut Self {
        self.function_with(f, range, CurveOpts::default())
    }

    /// Plot a function with explicit options (color, label, line width, samples).
    pub fn function_with<F: Fn(f64) -> f64>(
        &mut self,
        f: F,
        range: Range<f64>,
        opts: CurveOpts,
    ) -> &mut Self {
        let n = opts.samples.max(2);
        let (x0, x1) = if range.start <= range.end {
            (range.start, range.end)
        } else {
            (range.end, range.start)
        };
        let xs: Vec<f64> = (0..n)
            .map(|i| x0 + (x1 - x0) * i as f64 / (n - 1) as f64)
            .collect();
        let ys: Vec<f64> = xs.iter().map(|&x| f(x)).collect();
        let color = opts.color.unwrap_or_else(|| self.next_color());
        self.add_artist(Artist::Line(LineArtist {
            xs,
            ys,
            color,
            width_pt: opts.linewidth_pt.unwrap_or(self.style.line_width_pt),
            dash: opts.dash,
            marker: Marker::None,
            marker_size_pt: self.style.marker_size_pt,
            label: opts.label,
        }));
        self
    }

    /// Overlay a fitted [`oxiroot_fit::Model`] curve over `range` (the `fit`
    /// feature). The model is evaluated with its current parameters, so fit it
    /// first (`let r = h.fit(&model)`, then pass `model.with_params(r.params)`).
    #[cfg(feature = "fit")]
    pub fn model(&mut self, model: &oxiroot_fit::Model, range: Range<f64>) -> &mut Self {
        self.function_with(|x| model.eval(x), range, CurveOpts::default())
    }

    /// Overlay a fitted [`oxiroot_fit::Model`] curve with explicit options.
    #[cfg(feature = "fit")]
    pub fn model_with(
        &mut self,
        model: &oxiroot_fit::Model,
        range: Range<f64>,
        opts: CurveOpts,
    ) -> &mut Self {
        self.function_with(|x| model.eval(x), range, opts)
    }

    /// Resolve the data limits, honoring explicit limits and autoscaling the rest.
    fn limits(&self) -> (f64, f64, f64, f64) {
        let auto = self
            .artists
            .iter()
            .filter_map(Artist::bounds)
            .reduce(Bounds::union);
        let m = self.style.margin as f64;

        let (xmin, xmax) = match self.xlim {
            Some(l) => l,
            None => match auto {
                Some(b) => {
                    let pad = (b.xmax - b.xmin) * m;
                    let pad = if pad.abs() < f64::EPSILON { 0.5 } else { pad };
                    (b.xmin - pad, b.xmax + pad)
                }
                None => (0.0, 1.0),
            },
        };
        let (ymin, ymax) = match self.ylim {
            Some(l) => l,
            None => match auto {
                Some(b) => {
                    let span = b.ymax - b.ymin;
                    let pad = if span.abs() < f64::EPSILON {
                        0.5
                    } else {
                        span * m
                    };
                    if self.y_from_zero {
                        let top = b.ymax.max(0.0);
                        (0.0, top + top.abs().max(1.0) * m)
                    } else {
                        (b.ymin - pad, b.ymax + pad)
                    }
                }
                None => (0.0, 1.0),
            },
        };
        (xmin, xmax, ymin, ymax)
    }

    fn axes_box(&self, fig_w: f32, fig_h: f32) -> Rect {
        let (l, r, b, t) = self.style.margins_frac;
        Rect::new(
            l * fig_w,
            (1.0 - t) * fig_h,
            (r - l) * fig_w,
            (t - b) * fig_h,
        )
    }

    /// Build the draw groups using the default (margins-based) axes box.
    pub(crate) fn render(&self, fig_w: u32, fig_h: u32) -> Vec<DrawGroup> {
        let box_ = self.axes_box(fig_w as f32, fig_h as f32);
        self.render_at(box_)
    }

    /// Build the draw groups with the data area placed at an explicit pixel box
    /// (used by [`crate::figure::Figure`] for grid/ratio layouts).
    pub(crate) fn render_at(&self, mut box_: Rect) -> Vec<DrawGroup> {
        let s = &self.style;
        // Reserve space on the right for a colorbar, if present.
        let cb_rect = self.colorbar.as_ref().map(|_| {
            let cb_w = s.px(14.0);
            let gap = s.px(12.0);
            let label_space = s.px(46.0);
            box_.w -= gap + cb_w + label_space;
            Rect::new(box_.right() + gap, box_.y, cb_w, box_.h)
        });
        let (xmin, xmax, ymin, ymax) = self.limits();
        let t = Transform::new(box_, xmin, xmax, ymin, ymax);

        let xticks = ticker::ticks(xmin, xmax, ((box_.w / 70.0).round() as usize).clamp(3, 11));
        let yticks = ticker::ticks(ymin, ymax, ((box_.h / 50.0).round() as usize).clamp(3, 9));
        let xstep = ticker::nice_step(xmin, xmax, ((box_.w / 70.0).round() as usize).clamp(3, 11));
        let ystep = ticker::nice_step(ymin, ymax, ((box_.h / 50.0).round() as usize).clamp(3, 9));
        let xlabels = ticker::format_ticks(&xticks, xstep);
        let ylabels = ticker::format_ticks(&yticks, ystep);

        let mut grid = DrawGroup::new(Some(box_));
        let mut data = DrawGroup::new(Some(box_));
        let mut axis = DrawGroup::new(None);

        let fg = s.fg_color;
        let lw = s.px(s.axes_linewidth_pt);
        let major = s.px(s.tick_major_len_pt);
        let minor = s.px(s.tick_minor_len_pt);
        let pad = s.px(s.tick_pad_pt);
        let tlab = s.px(s.tick_label_size_pt);

        // Grid (drawn below data when enabled).
        if s.grid {
            let gstroke = Stroke::line(s.grid_color, s.px(s.grid_width_pt));
            for &xv in &xticks {
                let px = t.x(xv);
                grid.push(DrawCommand::Line {
                    p0: (px, box_.y),
                    p1: (px, box_.bottom()),
                    stroke: gstroke.clone(),
                });
            }
            for &yv in &yticks {
                let py = t.y(yv);
                grid.push(DrawCommand::Line {
                    p0: (box_.x, py),
                    p1: (box_.right(), py),
                    stroke: gstroke.clone(),
                });
            }
            // Fainter grid lines at the minor ticks.
            if self.grid_minor {
                let gminor =
                    Stroke::line(s.grid_color.with_alpha(0.5), s.px(s.grid_width_pt * 0.7));
                for &xv in &ticker::minor_ticks(xmin, xmax, &xticks, 5) {
                    let px = t.x(xv);
                    grid.push(DrawCommand::Line {
                        p0: (px, box_.y),
                        p1: (px, box_.bottom()),
                        stroke: gminor.clone(),
                    });
                }
                for &yv in &ticker::minor_ticks(ymin, ymax, &yticks, 5) {
                    let py = t.y(yv);
                    grid.push(DrawCommand::Line {
                        p0: (box_.x, py),
                        p1: (box_.right(), py),
                        stroke: gminor.clone(),
                    });
                }
            }
        }

        // Data artists.
        for a in &self.artists {
            a.draw(&t, s, &mut data);
        }

        // Frame: all four spines (matplotlib draws the full rectangle).
        axis.push(DrawCommand::Rect {
            rect: box_,
            fill: None,
            stroke: Some(Stroke::line(fg, lw)),
        });

        // Tick mark deltas for a given direction (outside, inside).
        let (out_len, in_len) = match s.tick_dir {
            TickDir::Out => (major, 0.0),
            TickDir::In => (0.0, major),
            TickDir::InOut => (major, major),
        };
        let (out_minor, in_minor) = match s.tick_dir {
            TickDir::Out => (minor, 0.0),
            TickDir::In => (0.0, minor),
            TickDir::InOut => (minor, minor),
        };
        let tickstroke = Stroke::line(fg, s.px(s.tick_major_width_pt));

        // X ticks (bottom and optionally top).
        for &xv in &xticks {
            let px = t.x(xv);
            if s.tick_sides.bottom {
                axis.push(DrawCommand::Line {
                    p0: (px, box_.bottom() - in_len),
                    p1: (px, box_.bottom() + out_len),
                    stroke: tickstroke.clone(),
                });
            }
            if s.tick_sides.top {
                axis.push(DrawCommand::Line {
                    p0: (px, box_.y + in_len),
                    p1: (px, box_.y - out_len),
                    stroke: tickstroke.clone(),
                });
            }
        }
        // Y ticks (left and optionally right).
        for &yv in &yticks {
            let py = t.y(yv);
            if s.tick_sides.left {
                axis.push(DrawCommand::Line {
                    p0: (box_.x + in_len, py),
                    p1: (box_.x - out_len, py),
                    stroke: tickstroke.clone(),
                });
            }
            if s.tick_sides.right {
                axis.push(DrawCommand::Line {
                    p0: (box_.right() - in_len, py),
                    p1: (box_.right() + out_len, py),
                    stroke: tickstroke.clone(),
                });
            }
        }
        // Minor ticks.
        if s.minor_ticks {
            let xmin_t = ticker::minor_ticks(xmin, xmax, &xticks, 5);
            let ymin_t = ticker::minor_ticks(ymin, ymax, &yticks, 5);
            for &xv in &xmin_t {
                let px = t.x(xv);
                if s.tick_sides.bottom {
                    axis.push(DrawCommand::Line {
                        p0: (px, box_.bottom() - in_minor),
                        p1: (px, box_.bottom() + out_minor),
                        stroke: tickstroke.clone(),
                    });
                }
                if s.tick_sides.top {
                    axis.push(DrawCommand::Line {
                        p0: (px, box_.y + in_minor),
                        p1: (px, box_.y - out_minor),
                        stroke: tickstroke.clone(),
                    });
                }
            }
            for &yv in &ymin_t {
                let py = t.y(yv);
                if s.tick_sides.left {
                    axis.push(DrawCommand::Line {
                        p0: (box_.x + in_minor, py),
                        p1: (box_.x - out_minor, py),
                        stroke: tickstroke.clone(),
                    });
                }
                if s.tick_sides.right {
                    axis.push(DrawCommand::Line {
                        p0: (box_.right() - in_minor, py),
                        p1: (box_.right() + out_minor, py),
                        stroke: tickstroke.clone(),
                    });
                }
            }
        }

        // X tick labels (below the bottom spine); hidden on shared-x upper panels.
        let label_y = box_.bottom() + out_len.max(0.0) + pad;
        if self.show_xticklabels {
            for (&xv, lab) in xticks.iter().zip(&xlabels) {
                let px = t.x(xv);
                axis.extend(text::layout(
                    lab,
                    px,
                    label_y,
                    tlab,
                    FontStyle::Regular,
                    fg,
                    HAlign::Center,
                    VAlign::Top,
                    0.0,
                ));
            }
        }
        // Y tick labels (left of the left spine); track max width for the ylabel.
        let mut max_ylabel_w = 0.0_f32;
        let label_x = box_.x - out_len.max(0.0) - pad;
        let nyt = yticks.len();
        if self.show_yticklabels {
            for (i, (&yv, lab)) in yticks.iter().zip(&ylabels).enumerate() {
                let py = t.y(yv);
                max_ylabel_w = max_ylabel_w.max(text::measure(lab, tlab, FontStyle::Regular).width);
                // yticks are ascending: index 0 is the bottom-most, last is the top.
                if (self.ylabel_prune.0 && i == 0) || (self.ylabel_prune.1 && i + 1 == nyt) {
                    continue;
                }
                axis.extend(text::layout(
                    lab,
                    label_x,
                    py,
                    tlab,
                    FontStyle::Regular,
                    fg,
                    HAlign::Right,
                    VAlign::Middle,
                    0.0,
                ));
            }
        }

        // Axis labels.
        let labsize = s.px(s.label_size_pt);
        if let (Some(xl), true) = (&self.xlabel, self.show_xticklabels) {
            let tick_h = text::measure("0", tlab, FontStyle::Regular).height();
            let y = label_y + tick_h + s.px(4.0);
            crate::mathtext::layout_label(
                &mut axis,
                xl,
                box_.x + box_.w / 2.0,
                y,
                labsize,
                fg,
                HAlign::Center,
                VAlign::Top,
                0.0,
            );
        }
        if let (Some(yl), true) = (&self.ylabel, self.show_yticklabels) {
            let x = label_x - max_ylabel_w - s.px(4.0);
            crate::mathtext::layout_label(
                &mut axis,
                yl,
                x,
                box_.y + box_.h / 2.0,
                labsize,
                fg,
                HAlign::Center,
                VAlign::Baseline,
                -90.0,
            );
        }
        if let Some(tt) = &self.title {
            let y = box_.y - s.px(6.0);
            crate::mathtext::layout_label(
                &mut axis,
                tt,
                box_.x + box_.w / 2.0,
                y,
                s.px(s.title_size_pt),
                fg,
                HAlign::Center,
                VAlign::Bottom,
                0.0,
            );
        }

        // Legend.
        if self.show_legend {
            crate::legend::draw_legend(&mut axis, self, box_);
        }

        // Colorbar.
        if let (Some(spec), Some(cbr)) = (&self.colorbar, cb_rect) {
            crate::colorbar::draw_colorbar(&mut axis, cbr, spec, s);
        }

        vec![grid, data, axis]
    }
}

impl Default for Axes {
    fn default() -> Self {
        Axes::new()
    }
}

/// Options for [`Axes::hist_with`]. Build with [`HistOpts::new`] and the chained
/// setters. Defaults to an mplhep step outline with no error bars and the next
/// cycle color.
#[derive(Debug, Clone, Default)]
pub struct HistOpts {
    pub(crate) histtype: HistType,
    pub(crate) yerr: bool,
    pub(crate) color: Option<Color>,
    pub(crate) fill_color: Option<Color>,
    pub(crate) label: Option<String>,
    pub(crate) linewidth_pt: Option<f32>,
}

impl HistOpts {
    /// New default options.
    #[must_use]
    pub fn new() -> Self {
        HistOpts::default()
    }
    /// Set the histogram type.
    #[must_use]
    pub fn histtype(mut self, t: HistType) -> Self {
        self.histtype = t;
        self
    }
    /// Draw `√N`/Sumw2 error bars at the bin centers.
    #[must_use]
    pub fn yerr(mut self) -> Self {
        self.yerr = true;
        self
    }
    /// Set the color.
    #[must_use]
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    /// Set the fill color.
    #[must_use]
    pub fn fill_color(mut self, c: Color) -> Self {
        self.fill_color = Some(c);
        self
    }
    /// Set the legend label.
    #[must_use]
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    /// Set the line width in points.
    #[must_use]
    pub fn linewidth(mut self, pt: f32) -> Self {
        self.linewidth_pt = Some(pt);
        self
    }
}

/// Options for [`Axes::errorbar_with`]. Build with [`ErrorbarOpts::new`] and the
/// chained setters. Defaults to the HEP data-point look: round markers,
/// vertical/horizontal error bars, no caps, no connecting line.
#[derive(Debug, Clone)]
pub struct ErrorbarOpts {
    pub(crate) color: Option<Color>,
    pub(crate) marker: Marker,
    pub(crate) marker_size_pt: Option<f32>,
    pub(crate) capsize_pt: f32,
    pub(crate) line: bool,
    pub(crate) label: Option<String>,
}

impl Default for ErrorbarOpts {
    fn default() -> Self {
        ErrorbarOpts {
            color: None,
            marker: Marker::Circle,
            marker_size_pt: None,
            capsize_pt: 0.0,
            line: false,
            label: None,
        }
    }
}

impl ErrorbarOpts {
    /// New default options.
    #[must_use]
    pub fn new() -> Self {
        ErrorbarOpts::default()
    }
    /// Set the color.
    #[must_use]
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    /// Set the marker shape.
    #[must_use]
    pub fn marker(mut self, m: Marker) -> Self {
        self.marker = m;
        self
    }
    /// Set the marker size in points.
    #[must_use]
    pub fn marker_size(mut self, pt: f32) -> Self {
        self.marker_size_pt = Some(pt);
        self
    }
    /// Set the error-bar cap size in points.
    #[must_use]
    pub fn capsize(mut self, pt: f32) -> Self {
        self.capsize_pt = pt;
        self
    }
    /// Draw a connecting line through the points.
    #[must_use]
    pub fn line(mut self) -> Self {
        self.line = true;
        self
    }
    /// Set the legend label.
    #[must_use]
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
}

/// Options for [`Axes::hist2d_with`]. Build with [`Hist2dOpts::new`] and the
/// chained setters.
#[derive(Debug, Clone, Default)]
pub struct Hist2dOpts {
    pub(crate) cmap: Colormap,
    pub(crate) vmin: Option<f64>,
    pub(crate) vmax: Option<f64>,
    pub(crate) label: Option<String>,
}

impl Hist2dOpts {
    /// New default options.
    #[must_use]
    pub fn new() -> Self {
        Hist2dOpts::default()
    }
    /// Set the colormap.
    #[must_use]
    pub fn cmap(mut self, c: Colormap) -> Self {
        self.cmap = c;
        self
    }
    /// Set the value (color) range, e.g. `Hist2dOpts::new().vrange(0.0..100.0)`.
    #[must_use]
    pub fn vrange(mut self, range: Range<f64>) -> Self {
        self.vmin = Some(range.start);
        self.vmax = Some(range.end);
        self
    }
    /// Set the colorbar label.
    #[must_use]
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
}

/// Options for [`Axes::function_with`] / `Axes::model_with` — a sampled analytic
/// curve. Build with [`CurveOpts::new`] and the chained setters.
#[derive(Debug, Clone)]
pub struct CurveOpts {
    pub(crate) color: Option<Color>,
    pub(crate) label: Option<String>,
    pub(crate) linewidth_pt: Option<f32>,
    pub(crate) dash: Option<Vec<f32>>,
    pub(crate) samples: usize,
}

impl Default for CurveOpts {
    fn default() -> Self {
        CurveOpts {
            color: None,
            label: None,
            linewidth_pt: None,
            dash: None,
            samples: 256,
        }
    }
}

impl CurveOpts {
    /// New default options.
    #[must_use]
    pub fn new() -> Self {
        CurveOpts::default()
    }
    /// Set the color.
    #[must_use]
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    /// Set the legend label.
    #[must_use]
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    /// Set the line width in points.
    #[must_use]
    pub fn linewidth(mut self, pt: f32) -> Self {
        self.linewidth_pt = Some(pt);
        self
    }
    /// Set a dash pattern (points).
    #[must_use]
    pub fn dashed(mut self, pattern: Vec<f32>) -> Self {
        self.dash = Some(pattern);
        self
    }
    /// Set the number of sample points.
    #[must_use]
    pub fn samples(mut self, n: usize) -> Self {
        self.samples = n;
        self
    }
}
