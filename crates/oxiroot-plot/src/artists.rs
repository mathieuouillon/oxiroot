//! Artists — the drawable elements added to an [`crate::axes::Axes`]. Each artist
//! owns its data in data-coordinates and knows how to compute its data bounds
//! (for autoscaling) and how to draw itself through a [`Transform`].

use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, Rect, Stroke};
use crate::style::Style;
use crate::transform::{Bounds, Transform};

/// Marker shape for data points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Marker {
    /// No marker.
    #[default]
    None,
    /// Filled circle (matplotlib `o`).
    Circle,
    /// Filled square (`s`).
    Square,
    /// Upward triangle (`^`).
    TriangleUp,
}

/// A legend handle describing how to draw a sample swatch.
#[derive(Debug, Clone)]
pub(crate) enum LegendHandle {
    /// A line/marker sample.
    Line {
        color: Color,
        width_pt: f32,
        dash: Option<Vec<f32>>,
        marker: Marker,
        marker_size_pt: f32,
    },
    /// A filled/outlined patch sample (histograms).
    Patch {
        face: Option<Color>,
        edge: Option<Color>,
        width_pt: f32,
    },
}

/// One legend entry.
#[derive(Debug, Clone)]
pub(crate) struct LegendItem {
    pub label: String,
    pub handle: LegendHandle,
}

/// Draw a marker centered at `c` with pixel diameter `size_px`.
pub(crate) fn draw_marker(
    g: &mut DrawGroup,
    shape: Marker,
    c: (f32, f32),
    size_px: f32,
    fill: Option<Color>,
    edge: Option<Stroke>,
) {
    let r = size_px / 2.0;
    match shape {
        Marker::None => {}
        Marker::Circle => g.push(DrawCommand::Circle {
            c,
            r,
            fill,
            stroke: edge,
        }),
        Marker::Square => g.push(DrawCommand::Rect {
            rect: Rect::new(c.0 - r, c.1 - r, 2.0 * r, 2.0 * r),
            fill,
            stroke: edge,
        }),
        Marker::TriangleUp => g.push(DrawCommand::Polygon {
            pts: vec![
                (c.0, c.1 - r),
                (c.0 + r * 0.866, c.1 + r * 0.5),
                (c.0 - r * 0.866, c.1 + r * 0.5),
            ],
            fill,
            stroke: edge,
        }),
    }
}

fn data_bounds(xs: &[f64], ys: &[f64]) -> Option<Bounds> {
    let mut b: Option<Bounds> = None;
    for (&x, &y) in xs.iter().zip(ys) {
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        b = Some(match b {
            None => Bounds::new(x, x, y, y),
            Some(p) => p.union(Bounds::new(x, x, y, y)),
        });
    }
    b
}

/// A connected line and/or markers through `(x, y)` points (matplotlib `plot`).
#[derive(Debug, Clone)]
pub(crate) struct LineArtist {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub color: Color,
    pub width_pt: f32,
    pub dash: Option<Vec<f32>>,
    pub marker: Marker,
    pub marker_size_pt: f32,
    pub label: Option<String>,
}

impl LineArtist {
    fn bounds(&self) -> Option<Bounds> {
        data_bounds(&self.xs, &self.ys)
    }

    fn draw(&self, t: &Transform, style: &Style, g: &mut DrawGroup) {
        let pts: Vec<(f32, f32)> = self
            .xs
            .iter()
            .zip(&self.ys)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|(&x, &y)| t.pt((x, y)))
            .collect();
        if self.width_pt > 0.0 && pts.len() >= 2 {
            let mut stroke = Stroke::new(self.color, style.px(self.width_pt));
            stroke.dash = self
                .dash
                .as_ref()
                .map(|d| d.iter().map(|v| style.px(*v)).collect());
            g.push(DrawCommand::Polyline {
                pts: pts.clone(),
                stroke,
            });
        }
        if self.marker != Marker::None {
            let size = style.px(self.marker_size_pt);
            for &p in &pts {
                draw_marker(g, self.marker, p, size, Some(self.color), None);
            }
        }
    }

    fn legend(&self) -> Option<LegendItem> {
        self.label.clone().map(|label| LegendItem {
            label,
            handle: LegendHandle::Line {
                color: self.color,
                width_pt: self.width_pt,
                dash: self.dash.clone(),
                marker: self.marker,
                marker_size_pt: self.marker_size_pt,
            },
        })
    }
}

/// How an mplhep-style histogram is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistType {
    /// Staircase outline closed to the baseline (mplhep default).
    #[default]
    Step,
    /// Filled staircase down to the baseline.
    Fill,
    /// Markers at bin centers with vertical error bars (data-point look).
    Errorbar,
    /// A shaded band spanning `y ± yerr` (uncertainty band).
    Band,
}

/// Build the staircase vertices for `edges`/`values` (data coords). When
/// `baseline` is set, the first and last points drop to `y = 0`.
fn staircase(edges: &[f64], values: &[f64], baseline: bool) -> Vec<(f64, f64)> {
    let n = values.len();
    let mut pts = Vec::with_capacity(2 * n + 2);
    if n == 0 || edges.len() < n + 1 {
        return pts;
    }
    if baseline {
        pts.push((edges[0], 0.0));
    }
    pts.push((edges[0], values[0]));
    for i in 1..n {
        pts.push((edges[i], values[i - 1]));
        pts.push((edges[i], values[i]));
    }
    pts.push((edges[n], values[n - 1]));
    if baseline {
        pts.push((edges[n], 0.0));
    }
    pts
}

/// An mplhep-style histogram (a `TH1` reduced to edges + values + optional yerr).
#[derive(Debug, Clone)]
pub(crate) struct StepArtist {
    pub edges: Vec<f64>,
    pub values: Vec<f64>,
    pub errs: Option<Vec<f64>>,
    pub histtype: HistType,
    pub color: Color,
    pub fill_color: Option<Color>,
    pub width_pt: f32,
    pub marker: Marker,
    pub marker_size_pt: f32,
    pub label: Option<String>,
}

impl StepArtist {
    fn n(&self) -> usize {
        self.values.len()
    }

    fn bounds(&self) -> Option<Bounds> {
        let n = self.n();
        if n == 0 || self.edges.len() < n + 1 {
            return None;
        }
        let xmin = self.edges[0];
        let xmax = self.edges[n];
        let mut ymin = 0.0_f64;
        let mut ymax = 0.0_f64;
        for i in 0..n {
            let (mut lo, mut hi) = (self.values[i], self.values[i]);
            if let Some(e) = &self.errs {
                lo -= e[i];
                hi += e[i];
            }
            ymin = ymin.min(lo);
            ymax = ymax.max(hi);
        }
        Some(Bounds::new(xmin, xmax, ymin, ymax))
    }

    fn map(&self, t: &Transform, pts: &[(f64, f64)]) -> Vec<(f32, f32)> {
        pts.iter().map(|&p| t.pt(p)).collect()
    }

    fn draw_errorbars(&self, t: &Transform, style: &Style, g: &mut DrawGroup) {
        let Some(errs) = &self.errs else { return };
        let stroke = Stroke::new(self.color, style.px(1.0));
        // Offset indexing into edges (i, i+1) keeps the explicit index clearest.
        #[allow(clippy::needless_range_loop)]
        for i in 0..self.n() {
            let c = 0.5 * (self.edges[i] + self.edges[i + 1]);
            let cx = t.x(c);
            let ytop = t.y(self.values[i] + errs[i]);
            let ybot = t.y(self.values[i] - errs[i]);
            g.push(DrawCommand::Line {
                p0: (cx, ybot),
                p1: (cx, ytop),
                stroke: stroke.clone(),
            });
        }
    }

    fn draw(&self, t: &Transform, style: &Style, g: &mut DrawGroup) {
        let n = self.n();
        if n == 0 {
            return;
        }
        match self.histtype {
            HistType::Step => {
                let pts = self.map(t, &staircase(&self.edges, &self.values, true));
                g.push(DrawCommand::Polyline {
                    pts,
                    stroke: Stroke::new(self.color, style.px(self.width_pt)),
                });
                self.draw_errorbars(t, style, g);
            }
            HistType::Fill => {
                let pts = self.map(t, &staircase(&self.edges, &self.values, true));
                g.push(DrawCommand::Polygon {
                    pts,
                    fill: Some(self.fill_color.unwrap_or(self.color)),
                    stroke: Some(Stroke::line(self.color, style.px(self.width_pt))),
                });
                self.draw_errorbars(t, style, g);
            }
            HistType::Errorbar => {
                let size = style.px(self.marker_size_pt);
                for i in 0..n {
                    let c = 0.5 * (self.edges[i] + self.edges[i + 1]);
                    let p = t.pt((c, self.values[i]));
                    draw_marker(g, self.marker, p, size, Some(self.color), None);
                }
                self.draw_errorbars(t, style, g);
            }
            HistType::Band => {
                let errs = self.errs.clone().unwrap_or_else(|| vec![0.0; n]);
                let upper: Vec<f64> = (0..n).map(|i| self.values[i] + errs[i]).collect();
                let lower: Vec<f64> = (0..n).map(|i| self.values[i] - errs[i]).collect();
                let mut up = staircase(&self.edges, &upper, false);
                let mut lo = staircase(&self.edges, &lower, false);
                lo.reverse();
                up.append(&mut lo);
                g.push(DrawCommand::Polygon {
                    pts: self.map(t, &up),
                    fill: Some(self.fill_color.unwrap_or(self.color.with_alpha(0.3))),
                    stroke: None,
                });
            }
        }
    }

    fn legend(&self) -> Option<LegendItem> {
        self.label.clone().map(|label| {
            let handle = match self.histtype {
                HistType::Errorbar => LegendHandle::Line {
                    color: self.color,
                    width_pt: 0.0,
                    dash: None,
                    marker: self.marker,
                    marker_size_pt: self.marker_size_pt,
                },
                // mplhep represents step/fill histograms in the legend with a
                // short horizontal line in the histogram's color (not a matplotlib
                // bar patch). The shaded uncertainty band stays a filled swatch.
                HistType::Step | HistType::Fill => LegendHandle::Line {
                    color: self.color,
                    width_pt: self.width_pt,
                    dash: None,
                    marker: Marker::None,
                    marker_size_pt: 0.0,
                },
                HistType::Band => LegendHandle::Patch {
                    face: Some(self.fill_color.unwrap_or(self.color.with_alpha(0.3))),
                    edge: None,
                    width_pt: self.width_pt,
                },
            };
            LegendItem { label, handle }
        })
    }
}

/// Data points with x/y error bars and markers (matplotlib `errorbar`). Errors
/// are stored as `(low, high)` magnitude pairs (symmetric → equal halves).
#[derive(Debug, Clone)]
pub(crate) struct ErrorbarArtist {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub xerr: Option<(Vec<f64>, Vec<f64>)>,
    pub yerr: Option<(Vec<f64>, Vec<f64>)>,
    pub color: Color,
    pub marker: Marker,
    pub marker_size_pt: f32,
    pub elinewidth_pt: f32,
    pub capsize_pt: f32,
    pub line_width_pt: Option<f32>,
    pub label: Option<String>,
}

impl ErrorbarArtist {
    fn n(&self) -> usize {
        self.xs.len().min(self.ys.len())
    }

    fn err_at(pair: &Option<(Vec<f64>, Vec<f64>)>, i: usize) -> (f64, f64) {
        match pair {
            Some((lo, hi)) => (
                lo.get(i).copied().unwrap_or(0.0),
                hi.get(i).copied().unwrap_or(0.0),
            ),
            None => (0.0, 0.0),
        }
    }

    fn bounds(&self) -> Option<Bounds> {
        let n = self.n();
        let mut b: Option<Bounds> = None;
        for i in 0..n {
            let (x, y) = (self.xs[i], self.ys[i]);
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            let (xl, xh) = Self::err_at(&self.xerr, i);
            let (yl, yh) = Self::err_at(&self.yerr, i);
            let bb = Bounds::new(x - xl, x + xh, y - yl, y + yh);
            b = Some(match b {
                None => bb,
                Some(p) => p.union(bb),
            });
        }
        b
    }

    fn draw(&self, t: &Transform, style: &Style, g: &mut DrawGroup) {
        let n = self.n();
        let estroke = Stroke::new(self.color, style.px(self.elinewidth_pt));
        let cap = style.px(self.capsize_pt);

        if let Some(wpt) = self.line_width_pt {
            let pts: Vec<(f32, f32)> = (0..n).map(|i| t.pt((self.xs[i], self.ys[i]))).collect();
            if pts.len() >= 2 {
                g.push(DrawCommand::Polyline {
                    pts,
                    stroke: Stroke::new(self.color, style.px(wpt)),
                });
            }
        }

        for i in 0..n {
            let (px, py) = t.pt((self.xs[i], self.ys[i]));
            let (yl, yh) = Self::err_at(&self.yerr, i);
            if self.yerr.is_some() && (yl != 0.0 || yh != 0.0) {
                let ytop = t.y(self.ys[i] + yh);
                let ybot = t.y(self.ys[i] - yl);
                g.push(DrawCommand::Line {
                    p0: (px, ybot),
                    p1: (px, ytop),
                    stroke: estroke.clone(),
                });
                if cap > 0.0 {
                    for cy in [ytop, ybot] {
                        g.push(DrawCommand::Line {
                            p0: (px - cap, cy),
                            p1: (px + cap, cy),
                            stroke: estroke.clone(),
                        });
                    }
                }
            }
            let (xl, xh) = Self::err_at(&self.xerr, i);
            if self.xerr.is_some() && (xl != 0.0 || xh != 0.0) {
                let xleft = t.x(self.xs[i] - xl);
                let xright = t.x(self.xs[i] + xh);
                g.push(DrawCommand::Line {
                    p0: (xleft, py),
                    p1: (xright, py),
                    stroke: estroke.clone(),
                });
                if cap > 0.0 {
                    for cx in [xleft, xright] {
                        g.push(DrawCommand::Line {
                            p0: (cx, py - cap),
                            p1: (cx, py + cap),
                            stroke: estroke.clone(),
                        });
                    }
                }
            }
        }

        // Markers on top.
        if self.marker != Marker::None {
            let size = style.px(self.marker_size_pt);
            for i in 0..n {
                let p = t.pt((self.xs[i], self.ys[i]));
                draw_marker(g, self.marker, p, size, Some(self.color), None);
            }
        }
    }

    fn legend(&self) -> Option<LegendItem> {
        self.label.clone().map(|label| LegendItem {
            label,
            handle: LegendHandle::Line {
                color: self.color,
                width_pt: self.line_width_pt.unwrap_or(0.0),
                dash: None,
                marker: self.marker,
                marker_size_pt: self.marker_size_pt,
            },
        })
    }
}

/// A 2-D color mesh of a `TH2` (matplotlib `pcolormesh`). `values[ix][iy]` are
/// in-range bin contents; cell `(ix, iy)` spans `xedges[ix..ix+1]` × `yedges`.
#[derive(Debug, Clone)]
pub(crate) struct MeshArtist {
    pub xedges: Vec<f64>,
    pub yedges: Vec<f64>,
    pub values: Vec<Vec<f64>>,
    pub cmap: crate::cmap::Colormap,
    pub vmin: f64,
    pub vmax: f64,
}

impl MeshArtist {
    fn bounds(&self) -> Option<Bounds> {
        let nx = self.values.len();
        if nx == 0 {
            return None;
        }
        let ny = self.values[0].len();
        if self.xedges.len() < nx + 1 || self.yedges.len() < ny + 1 {
            return None;
        }
        Some(Bounds::new(
            self.xedges[0],
            self.xedges[nx],
            self.yedges[0],
            self.yedges[ny],
        ))
    }

    fn draw(&self, t: &Transform, _style: &Style, g: &mut DrawGroup) {
        let nx = self.values.len();
        if nx == 0 {
            return;
        }
        let span = {
            let s = self.vmax - self.vmin;
            if s.abs() < f64::EPSILON {
                1.0
            } else {
                s
            }
        };
        for ix in 0..nx {
            if self.xedges.len() < ix + 2 {
                break;
            }
            let x0 = t.x(self.xedges[ix]);
            let x1 = t.x(self.xedges[ix + 1]);
            let (rx, rw) = (x0.min(x1), (x1 - x0).abs());
            let col = &self.values[ix];
            // Offset indexing into yedges (iy, iy+1) keeps the index explicit.
            #[allow(clippy::needless_range_loop)]
            for iy in 0..col.len() {
                if self.yedges.len() < iy + 2 {
                    break;
                }
                let color = self.cmap.sample((col[iy] - self.vmin) / span);
                let y0 = t.y(self.yedges[iy]);
                let y1 = t.y(self.yedges[iy + 1]);
                let (ry, rh) = (y0.min(y1), (y1 - y0).abs());
                // Slight overlap avoids anti-aliasing seams between cells.
                g.push(DrawCommand::Rect {
                    rect: Rect::new(rx - 0.25, ry - 0.25, rw + 0.5, rh + 0.5),
                    fill: Some(color),
                    stroke: None,
                });
            }
        }
    }
}

/// The set of artist kinds an axes can hold.
#[derive(Debug, Clone)]
pub(crate) enum Artist {
    Line(LineArtist),
    Step(StepArtist),
    Errorbar(ErrorbarArtist),
    Mesh(MeshArtist),
}

impl Artist {
    pub(crate) fn bounds(&self) -> Option<Bounds> {
        match self {
            Artist::Line(a) => a.bounds(),
            Artist::Step(a) => a.bounds(),
            Artist::Errorbar(a) => a.bounds(),
            Artist::Mesh(a) => a.bounds(),
        }
    }

    pub(crate) fn draw(&self, t: &Transform, style: &Style, g: &mut DrawGroup) {
        match self {
            Artist::Line(a) => a.draw(t, style, g),
            Artist::Step(a) => a.draw(t, style, g),
            Artist::Errorbar(a) => a.draw(t, style, g),
            Artist::Mesh(a) => a.draw(t, style, g),
        }
    }

    pub(crate) fn legend(&self) -> Option<LegendItem> {
        match self {
            Artist::Line(a) => a.legend(),
            Artist::Step(a) => a.legend(),
            Artist::Errorbar(a) => a.legend(),
            Artist::Mesh(_) => None,
        }
    }
}
