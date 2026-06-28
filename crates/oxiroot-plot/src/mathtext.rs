//! Label typesetting with `$…$` math spans.
//!
//! A label is split into plain-text and math runs. Plain text is laid out with
//! the bundled DejaVu Sans; each `$…$` run is typeset by the ReX TeX engine
//! (using the bundled Fira Math font) through a custom [`Backend`] that turns
//! ReX's glyph/rule draw calls into the crate's own [`Path`]/polygon IR. Both
//! kinds of run share one baseline, then the whole block is anchored and rotated
//! exactly like [`crate::text::layout`]. A malformed math run falls back to a
//! stripped plain-text rendering rather than failing.

use rex::font::backend::ttf_parser::TtfMathFont;
use rex::font::common::GlyphId;
use rex::layout::engine::LayoutBuilder;
use rex::parser::parse;
use rex::render::{Backend, Cursor, FontBackend, GraphicsBackend, Renderer, RGBA};

use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, Path, Pt, Seg};
use crate::text::{self, FontStyle, HAlign, VAlign};

static FIRA_MATH: &[u8] = include_bytes!("../assets/FiraMath-Regular.otf");

/// A glyph outline or filled rule in a math run's local frame (baseline y = 0).
enum LocalPrim {
    /// A filled glyph outline.
    Fill(Path),
    /// A filled axis-aligned rule (fraction bar, radical) as a polygon.
    Poly(Vec<Pt>),
}

impl LocalPrim {
    fn offset_x(&mut self, dx: f32) {
        match self {
            LocalPrim::Fill(p) => {
                for seg in &mut p.segs {
                    match seg {
                        Seg::Move(x, _) | Seg::Line(x, _) => *x += dx,
                        Seg::Quad(cx, _, x, _) => {
                            *cx += dx;
                            *x += dx;
                        }
                        Seg::Cubic(c1x, _, c2x, _, x, _) => {
                            *c1x += dx;
                            *c2x += dx;
                            *x += dx;
                        }
                        Seg::Close => {}
                    }
                }
            }
            LocalPrim::Poly(pts) => {
                for p in pts {
                    p.0 += dx;
                }
            }
        }
    }
}

/// Render a (possibly `$…$`-containing) label into `g`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_label(
    g: &mut DrawGroup,
    label: &str,
    x: f32,
    y: f32,
    size_px: f32,
    color: Color,
    halign: HAlign,
    valign: VAlign,
    rotation_deg: f32,
) {
    if !label.contains('$') {
        g.extend(text::layout(
            label,
            x,
            y,
            size_px,
            FontStyle::Regular,
            color,
            halign,
            valign,
            rotation_deg,
        ));
        return;
    }

    // Lay out the runs along one baseline (x from 0), collecting local prims.
    let mut prims: Vec<LocalPrim> = Vec::new();
    let mut pen = 0.0_f32;
    let mut ascent = size_px * 0.7;
    let mut descent = size_px * 0.2;

    for (is_math, part) in split_runs(label) {
        if part.is_empty() {
            continue;
        }
        if is_math {
            if let Some((mut mp, w, a, d)) = render_math(&part, size_px) {
                for p in &mut mp {
                    p.offset_x(pen);
                }
                prims.append(&mut mp);
                pen += w;
                ascent = ascent.max(a);
                descent = descent.max(d);
                continue;
            }
            // Fallback: render the math source as plain text.
            let plain = strip_math(&part);
            push_text_run(&mut prims, &plain, pen, size_px);
            let ext = text::measure(&plain, size_px, FontStyle::Regular);
            pen += ext.width;
            ascent = ascent.max(ext.ascent);
            descent = descent.max(ext.descent);
        } else {
            push_text_run(&mut prims, &part, pen, size_px);
            let ext = text::measure(&part, size_px, FontStyle::Regular);
            pen += ext.width;
            ascent = ascent.max(ext.ascent);
            descent = descent.max(ext.descent);
        }
    }

    let width = pen;
    let hoff = match halign {
        HAlign::Left => 0.0,
        HAlign::Center => -width / 2.0,
        HAlign::Right => -width,
    };
    let voff = match valign {
        VAlign::Baseline => 0.0,
        VAlign::Top => ascent,
        VAlign::Middle => (ascent - descent) / 2.0,
        VAlign::Bottom => -descent,
    };
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let xf = move |lx: f32, ly: f32| -> Pt {
        let px = lx + hoff;
        let py = ly + voff;
        (x + px * cos - py * sin, y + px * sin + py * cos)
    };

    for prim in prims {
        match prim {
            LocalPrim::Fill(p) => g.push(DrawCommand::Path {
                path: transform_path(&p, &xf),
                fill: Some(color),
                stroke: None,
            }),
            LocalPrim::Poly(pts) => g.push(DrawCommand::Polygon {
                pts: pts.iter().map(|&(lx, ly)| xf(lx, ly)).collect(),
                fill: Some(color),
                stroke: None,
            }),
        }
    }
}

fn push_text_run(prims: &mut Vec<LocalPrim>, text: &str, pen: f32, size_px: f32) {
    for mut p in text::glyph_paths_local(text, size_px, FontStyle::Regular) {
        let mut lp = LocalPrim::Fill(std::mem::take(&mut p));
        lp.offset_x(pen);
        prims.push(lp);
    }
}

fn transform_path(path: &Path, xf: &impl Fn(f32, f32) -> Pt) -> Path {
    let segs = path
        .segs
        .iter()
        .map(|seg| match *seg {
            Seg::Move(x, y) => {
                let (a, b) = xf(x, y);
                Seg::Move(a, b)
            }
            Seg::Line(x, y) => {
                let (a, b) = xf(x, y);
                Seg::Line(a, b)
            }
            Seg::Quad(cx, cy, x, y) => {
                let (a, b) = xf(cx, cy);
                let (c, d) = xf(x, y);
                Seg::Quad(a, b, c, d)
            }
            Seg::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                let (a, b) = xf(c1x, c1y);
                let (c, d) = xf(c2x, c2y);
                let (e, f) = xf(x, y);
                Seg::Cubic(a, b, c, d, e, f)
            }
            Seg::Close => Seg::Close,
        })
        .collect();
    Path { segs }
}

/// Split a label into runs at `$` delimiters: even runs are text, odd are math.
fn split_runs(s: &str) -> Vec<(bool, String)> {
    let mut runs = Vec::new();
    let mut is_math = false;
    for part in s.split('$') {
        runs.push((is_math, part.to_string()));
        is_math = !is_math;
    }
    runs
}

/// Typeset one math run with ReX, returning local prims plus `(width, ascent,
/// descent)` in pixels. `None` on a font or parse error.
fn render_math(tex: &str, size_px: f32) -> Option<(Vec<LocalPrim>, f32, f32, f32)> {
    let face = ttf_parser::Face::parse(FIRA_MATH, 0).ok()?;
    let font = TtfMathFont::new(face).ok()?;
    let engine = LayoutBuilder::new(&font)
        .font_size(f64::from(size_px))
        .build();
    let nodes = parse(tex).ok()?;
    let layout = engine.layout(&nodes).ok()?;
    let dims = layout.size();
    let mut collector = MathCollector { prims: Vec::new() };
    Renderer::new().render(&layout, &mut collector);
    Some((
        collector.prims,
        dims.width as f32,
        dims.height as f32,
        dims.depth as f32,
    ))
}

/// Strip `$`/braces and a few escapes for the plain-text fallback.
fn strip_math(s: &str) -> String {
    s.replace("\\mathrm", "")
        .replace("\\,", " ")
        .replace(['{', '}'], "")
}

/// A ReX [`Backend`] collecting glyph outlines + rules into [`LocalPrim`]s.
struct MathCollector {
    prims: Vec<LocalPrim>,
}

impl FontBackend<TtfMathFont<'_>> for MathCollector {
    fn symbol(&mut self, pos: Cursor, gid: GlyphId, scale: f64, ctx: &TtfMathFont<'_>) {
        let m = ctx.font_matrix();
        let mut b = OutlineToPath {
            path: Path::new(),
            m: (m.sx, m.ky, m.kx, m.sy, m.tx, m.ty),
            scale: scale as f32,
            pos: (pos.x as f32, pos.y as f32),
        };
        ctx.font().outline_glyph(gid.into(), &mut b);
        if !b.path.is_empty() {
            self.prims.push(LocalPrim::Fill(b.path));
        }
    }
}

impl GraphicsBackend for MathCollector {
    fn rule(&mut self, pos: Cursor, width: f64, height: f64) {
        let (x, y, w, h) = (pos.x as f32, pos.y as f32, width as f32, height as f32);
        self.prims.push(LocalPrim::Poly(vec![
            (x, y),
            (x + w, y),
            (x + w, y + h),
            (x, y + h),
        ]));
    }
    fn begin_color(&mut self, _color: RGBA) {}
    fn end_color(&mut self) {}
}

impl Backend<TtfMathFont<'_>> for MathCollector {}

/// Maps a glyph outline (font units) through the ReX font matrix + scale + a
/// Y-flip + the glyph position, building a [`Path`] in the math run's local frame.
struct OutlineToPath {
    path: Path,
    m: (f32, f32, f32, f32, f32, f32),
    scale: f32,
    pos: (f32, f32),
}

impl OutlineToPath {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let (sx, ky, kx, sy, tx, ty) = self.m;
        let x1 = sx * x + kx * y + tx;
        let y1 = ky * x + sy * y + ty;
        (self.scale * x1 + self.pos.0, -self.scale * y1 + self.pos.1)
    }
}

impl ttf_parser::OutlineBuilder for OutlineToPath {
    fn move_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.map(x, y);
        self.path.segs.push(Seg::Move(a, b));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.map(x, y);
        self.path.segs.push(Seg::Line(a, b));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (a, b) = self.map(x1, y1);
        let (c, d) = self.map(x, y);
        self.path.segs.push(Seg::Quad(a, b, c, d));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b) = self.map(x1, y1);
        let (c, d) = self.map(x2, y2);
        let (e, f) = self.map(x, y);
        self.path.segs.push(Seg::Cubic(a, b, c, d, e, f));
    }
    fn close(&mut self) {
        self.path.segs.push(Seg::Close);
    }
}
