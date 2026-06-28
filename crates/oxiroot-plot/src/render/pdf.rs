//! PDF backend: render the [`DrawGroup`] IR to a single-page PDF (PDF 1.4).
//!
//! Vector output, hand-written like the SVG backend — no font embedding is
//! needed because text is already reduced to glyph-outline paths. PDF user space
//! has its origin at the bottom-left with y pointing up, so every coordinate is
//! y-flipped against our top-left pixel space. Translucent fills use an
//! `ExtGState` (`/ca` /`/CA`); rectangle clips use `q … re W n … Q`.

use std::fmt::Write;

use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, LineCap, LineJoin, Path as IrPath, Seg, Stroke};

/// Circle → 4 cubic Béziers (PDF has no arc/circle operator).
const KAPPA: f32 = 0.552_285;

struct Builder {
    content: String,
    /// Distinct `(ca, CA)` alpha pairs (as 0–255) needing an `ExtGState`.
    gstates: Vec<(u8, u8)>,
    h: f32,
}

fn num(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn rgb(c: Color) -> String {
    format!(
        "{} {} {}",
        num(f32::from(c.r) / 255.0),
        num(f32::from(c.g) / 255.0),
        num(f32::from(c.b) / 255.0)
    )
}

impl Builder {
    fn new(h: f32) -> Self {
        Builder {
            content: String::new(),
            gstates: Vec::new(),
            h,
        }
    }

    fn fy(&self, y: f32) -> f32 {
        self.h - y
    }

    /// Register the alpha pair for a shape; returns the `gs` invocation if any
    /// channel is translucent.
    fn alpha_gs(&mut self, fill: Option<Color>, stroke: Option<&Stroke>) -> Option<String> {
        let ca = fill.map_or(255, |c| c.a);
        let cap = stroke.map_or(255, |s| s.color.a);
        if ca == 255 && cap == 255 {
            return None;
        }
        let pair = (ca, cap);
        let idx = self
            .gstates
            .iter()
            .position(|p| *p == pair)
            .unwrap_or_else(|| {
                self.gstates.push(pair);
                self.gstates.len() - 1
            });
        Some(format!("/GS{} gs\n", idx + 1))
    }

    fn moveto(&mut self, x: f32, y: f32) {
        let _ = writeln!(self.content, "{} {} m", num(x), num(self.fy(y)));
    }
    fn lineto(&mut self, x: f32, y: f32) {
        let _ = writeln!(self.content, "{} {} l", num(x), num(self.fy(y)));
    }
    fn curveto(&mut self, c1: (f32, f32), c2: (f32, f32), p: (f32, f32)) {
        let _ = writeln!(
            self.content,
            "{} {} {} {} {} {} c",
            num(c1.0),
            num(self.fy(c1.1)),
            num(c2.0),
            num(self.fy(c2.1)),
            num(p.0),
            num(self.fy(p.1))
        );
    }

    fn stroke_state(&mut self, s: &Stroke) {
        let _ = writeln!(self.content, "{} RG", rgb(s.color));
        let _ = writeln!(self.content, "{} w", num(s.width.max(0.0)));
        let cap = match s.cap {
            LineCap::Butt => 0,
            LineCap::Round => 1,
            LineCap::Square => 2,
        };
        let join = match s.join {
            LineJoin::Miter => 0,
            LineJoin::Round => 1,
            LineJoin::Bevel => 2,
        };
        let _ = writeln!(self.content, "{cap} J {join} j");
        match &s.dash {
            Some(d) => {
                let pat: Vec<String> = d.iter().map(|v| num(*v)).collect();
                let _ = writeln!(self.content, "[{}] 0 d", pat.join(" "));
            }
            None => self.content.push_str("[] 0 d\n"),
        }
    }

    /// Emit a path body (without the paint op), translating the IR path.
    fn emit_ir_path(&mut self, path: &IrPath) {
        let mut cur = (0.0, 0.0);
        for seg in &path.segs {
            match *seg {
                Seg::Move(x, y) => {
                    self.moveto(x, y);
                    cur = (x, y);
                }
                Seg::Line(x, y) => {
                    self.lineto(x, y);
                    cur = (x, y);
                }
                Seg::Quad(cx, cy, x, y) => {
                    // Quadratic → cubic (exact): C1 = S + 2/3(Q-S), C2 = E + 2/3(Q-E).
                    let c1 = (
                        cur.0 + 2.0 / 3.0 * (cx - cur.0),
                        cur.1 + 2.0 / 3.0 * (cy - cur.1),
                    );
                    let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
                    self.curveto(c1, c2, (x, y));
                    cur = (x, y);
                }
                Seg::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                    self.curveto((c1x, c1y), (c2x, c2y), (x, y));
                    cur = (x, y);
                }
                Seg::Close => self.content.push_str("h\n"),
            }
        }
    }

    fn paint(&mut self, fill: Option<Color>, stroke: Option<&Stroke>) {
        match (fill, stroke) {
            (Some(_), Some(_)) => self.content.push_str("B\n"),
            (Some(_), None) => self.content.push_str("f\n"),
            (None, Some(_)) => self.content.push_str("S\n"),
            (None, None) => self.content.push_str("n\n"),
        }
    }

    /// Draw a fillable/strokable shape with the proper color + alpha state.
    fn shape(&mut self, fill: Option<Color>, stroke: Option<&Stroke>, build: impl Fn(&mut Self)) {
        let stroke = stroke.filter(|s| s.width > 0.0 && s.color.a != 0);
        let fill = fill.filter(|c| c.a != 0);
        if fill.is_none() && stroke.is_none() {
            return;
        }
        let gs = self.alpha_gs(fill, stroke);
        let wrap = gs.is_some();
        if wrap {
            self.content.push_str("q\n");
            if let Some(g) = gs {
                self.content.push_str(&g);
            }
        }
        if let Some(c) = fill {
            let _ = writeln!(self.content, "{} rg", rgb(c));
        }
        if let Some(s) = stroke {
            self.stroke_state(s);
        }
        build(self);
        self.paint(fill, stroke);
        if wrap {
            self.content.push_str("Q\n");
        }
    }

    fn draw(&mut self, cmd: &DrawCommand) {
        match cmd {
            DrawCommand::Polyline { pts, stroke } => {
                if pts.len() < 2 {
                    return;
                }
                let pts = pts.clone();
                self.shape(None, Some(stroke), |b| {
                    b.moveto(pts[0].0, pts[0].1);
                    for p in &pts[1..] {
                        b.lineto(p.0, p.1);
                    }
                });
            }
            DrawCommand::Polygon { pts, fill, stroke } => {
                if pts.len() < 2 {
                    return;
                }
                let pts = pts.clone();
                self.shape(*fill, stroke.as_ref(), |b| {
                    b.moveto(pts[0].0, pts[0].1);
                    for p in &pts[1..] {
                        b.lineto(p.0, p.1);
                    }
                    b.content.push_str("h\n");
                });
            }
            DrawCommand::Line { p0, p1, stroke } => {
                let (p0, p1) = (*p0, *p1);
                self.shape(None, Some(stroke), |b| {
                    b.moveto(p0.0, p0.1);
                    b.lineto(p1.0, p1.1);
                });
            }
            DrawCommand::Rect { rect, fill, stroke } => {
                let r = *rect;
                self.shape(*fill, stroke.as_ref(), |b| {
                    // re takes the lower-left corner; after the y-flip that is
                    // (x, H - (y + h)) with the same width/height.
                    let _ = writeln!(
                        b.content,
                        "{} {} {} {} re",
                        num(r.x),
                        num(b.fy(r.y + r.h)),
                        num(r.w),
                        num(r.h)
                    );
                });
            }
            DrawCommand::Circle { c, r, fill, stroke } => {
                let (cx, cy, r) = (c.0, c.1, *r);
                self.shape(*fill, stroke.as_ref(), |b| {
                    let k = KAPPA * r;
                    b.moveto(cx + r, cy);
                    b.curveto((cx + r, cy + k), (cx + k, cy + r), (cx, cy + r));
                    b.curveto((cx - k, cy + r), (cx - r, cy + k), (cx - r, cy));
                    b.curveto((cx - r, cy - k), (cx - k, cy - r), (cx, cy - r));
                    b.curveto((cx + k, cy - r), (cx + r, cy - k), (cx + r, cy));
                    b.content.push_str("h\n");
                });
            }
            DrawCommand::Path { path, fill, stroke } => {
                let path = path.clone();
                self.shape(*fill, stroke.as_ref(), |b| b.emit_ir_path(&path));
            }
        }
    }
}

/// Render the IR to a single-page PDF document.
#[must_use]
pub fn render(groups: &[DrawGroup], width: u32, height: u32, bg: Color) -> Vec<u8> {
    let mut b = Builder::new(height as f32);

    // Opaque background fills the page first (skip when transparent).
    if bg.a != 0 {
        b.shape(Some(bg), None, |bb| {
            let _ = writeln!(bb.content, "0 0 {} {} re", width, height);
        });
    }

    for group in groups {
        if let Some(c) = group.clip {
            let _ = writeln!(
                b.content,
                "q\n{} {} {} {} re\nW n",
                num(c.x),
                num(b.fy(c.y + c.h)),
                num(c.w),
                num(c.h)
            );
        }
        for cmd in &group.cmds {
            b.draw(cmd);
        }
        if group.clip.is_some() {
            b.content.push_str("Q\n");
        }
    }

    assemble(&b.content, &b.gstates, width, height)
}

/// Assemble the PDF objects, the classic xref table, and the trailer with
/// byte-exact offsets.
fn assemble(content: &str, gstates: &[(u8, u8)], width: u32, height: u32) -> Vec<u8> {
    let has_alpha = !gstates.is_empty();

    // ExtGState objects are numbered right after Contents (object 4): 5, 6, …
    let extgstate = if has_alpha {
        let mut s = String::from(" /Resources << /ExtGState <<");
        for i in 0..gstates.len() {
            let _ = write!(s, " /GS{} {} 0 R", i + 1, 5 + i);
        }
        s.push_str(" >> >>");
        s
    } else {
        String::new()
    };

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    buf.extend_from_slice(&[b'%', 0xE2, 0xE3, 0xCF, 0xD3, b'\n']);

    let mut offsets: Vec<usize> = Vec::new();
    let put = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, n: usize, body: &str| {
        offsets.push(buf.len());
        buf.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        buf.extend_from_slice(body.as_bytes());
        buf.extend_from_slice(b"\nendobj\n");
    };

    put(
        &mut buf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );
    put(
        &mut buf,
        &mut offsets,
        2,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    put(
        &mut buf,
        &mut offsets,
        3,
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}]{extgstate} /Contents 4 0 R >>"
        ),
    );
    put(
        &mut buf,
        &mut offsets,
        4,
        &format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    );
    if has_alpha {
        for (i, (ca, cap)) in gstates.iter().enumerate() {
            put(
                &mut buf,
                &mut offsets,
                5 + i,
                &format!(
                    "<< /Type /ExtGState /ca {} /CA {} >>",
                    num(f32::from(*ca) / 255.0),
                    num(f32::from(*cap) / 255.0)
                ),
            );
        }
    }

    let xref_pos = buf.len();
    let total = offsets.len() + 1;
    buf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size {total} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n")
            .as_bytes(),
    );

    buf
}
