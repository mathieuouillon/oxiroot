//! Raster backend: render the [`DrawGroup`] IR into a tiny-skia `Pixmap` and
//! encode it as PNG. tiny-skia is resvg's own anti-aliased CPU rasterizer.

use tiny_skia::{
    FillRule, LineCap as SkCap, LineJoin as SkJoin, Mask, Paint, PathBuilder, Pixmap,
    Rect as SkRect, Stroke as SkStroke, StrokeDash, Transform,
};

use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, LineCap, LineJoin, Path as IrPath, Rect, Seg, Stroke};
use crate::error::{Error, Result};

fn sk_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn sk_cap(c: LineCap) -> SkCap {
    match c {
        LineCap::Butt => SkCap::Butt,
        LineCap::Round => SkCap::Round,
        LineCap::Square => SkCap::Square,
    }
}

fn sk_join(j: LineJoin) -> SkJoin {
    match j {
        LineJoin::Miter => SkJoin::Miter,
        LineJoin::Round => SkJoin::Round,
        LineJoin::Bevel => SkJoin::Bevel,
    }
}

fn sk_stroke(s: &Stroke) -> SkStroke {
    let mut sk = SkStroke {
        width: s.width.max(0.0),
        line_cap: sk_cap(s.cap),
        line_join: sk_join(s.join),
        ..SkStroke::default()
    };
    if let Some(pattern) = &s.dash {
        sk.dash = StrokeDash::new(pattern.clone(), 0.0);
    }
    sk
}

fn polyline_path(pts: &[(f32, f32)], close: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let mut iter = pts.iter();
    let first = iter.next()?;
    pb.move_to(first.0, first.1);
    for &(x, y) in iter {
        pb.line_to(x, y);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

fn ir_path(path: &IrPath) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for seg in &path.segs {
        match *seg {
            Seg::Move(x, y) => pb.move_to(x, y),
            Seg::Line(x, y) => pb.line_to(x, y),
            Seg::Quad(cx, cy, x, y) => pb.quad_to(cx, cy, x, y),
            Seg::Cubic(c1x, c1y, c2x, c2y, x, y) => pb.cubic_to(c1x, c1y, c2x, c2y, x, y),
            Seg::Close => pb.close(),
        }
    }
    pb.finish()
}

fn fill_path(pixmap: &mut Pixmap, path: &tiny_skia::Path, color: Color, clip: Option<&Mask>) {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(sk_color(color));
    pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), clip);
}

fn stroke_path(pixmap: &mut Pixmap, path: &tiny_skia::Path, stroke: &Stroke, clip: Option<&Mask>) {
    if stroke.width <= 0.0 || stroke.color.a == 0 {
        return;
    }
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(sk_color(stroke.color));
    pixmap.stroke_path(
        path,
        &paint,
        &sk_stroke(stroke),
        Transform::identity(),
        clip,
    );
}

fn rect_path(r: Rect) -> Option<tiny_skia::Path> {
    SkRect::from_xywh(r.x, r.y, r.w, r.h).map(PathBuilder::from_rect)
}

fn draw_cmd(pixmap: &mut Pixmap, cmd: &DrawCommand, clip: Option<&Mask>) {
    match cmd {
        DrawCommand::Polyline { pts, stroke } => {
            if let Some(p) = polyline_path(pts, false) {
                stroke_path(pixmap, &p, stroke, clip);
            }
        }
        DrawCommand::Polygon { pts, fill, stroke } => {
            if let Some(p) = polyline_path(pts, true) {
                if let Some(c) = fill {
                    fill_path(pixmap, &p, *c, clip);
                }
                if let Some(s) = stroke {
                    stroke_path(pixmap, &p, s, clip);
                }
            }
        }
        DrawCommand::Line { p0, p1, stroke } => {
            if let Some(p) = polyline_path(&[*p0, *p1], false) {
                stroke_path(pixmap, &p, stroke, clip);
            }
        }
        DrawCommand::Rect { rect, fill, stroke } => {
            if let Some(p) = rect_path(*rect) {
                if let Some(c) = fill {
                    fill_path(pixmap, &p, *c, clip);
                }
                if let Some(s) = stroke {
                    stroke_path(pixmap, &p, s, clip);
                }
            }
        }
        DrawCommand::Circle { c, r, fill, stroke } => {
            if let Some(p) = PathBuilder::from_circle(c.0, c.1, *r) {
                if let Some(col) = fill {
                    fill_path(pixmap, &p, *col, clip);
                }
                if let Some(s) = stroke {
                    stroke_path(pixmap, &p, s, clip);
                }
            }
        }
        DrawCommand::Path { path, fill, stroke } => {
            if let Some(p) = ir_path(path) {
                if let Some(c) = fill {
                    fill_path(pixmap, &p, *c, clip);
                }
                if let Some(s) = stroke {
                    stroke_path(pixmap, &p, s, clip);
                }
            }
        }
    }
}

fn clip_mask(width: u32, height: u32, rect: Rect) -> Option<Mask> {
    let mut mask = Mask::new(width, height)?;
    let p = rect_path(rect)?;
    mask.fill_path(&p, FillRule::Winding, true, Transform::identity());
    Some(mask)
}

/// Render the IR into a `Pixmap` filled with `bg`.
pub fn render(groups: &[DrawGroup], width: u32, height: u32, bg: Color) -> Result<Pixmap> {
    if width == 0 || height == 0 || width > 32768 || height > 32768 {
        return Err(Error::BadSize(format!("{width}x{height} px")));
    }
    let mut pixmap =
        Pixmap::new(width, height).ok_or_else(|| Error::BadSize(format!("{width}x{height} px")))?;
    pixmap.fill(sk_color(bg));
    for group in groups {
        let mask = group.clip.and_then(|r| clip_mask(width, height, r));
        for cmd in &group.cmds {
            draw_cmd(&mut pixmap, cmd, mask.as_ref());
        }
    }
    Ok(pixmap)
}

/// Render the IR to PNG bytes.
pub fn render_png(groups: &[DrawGroup], width: u32, height: u32, bg: Color) -> Result<Vec<u8>> {
    let pixmap = render(groups, width, height, bg)?;
    pixmap
        .encode_png()
        .map_err(|e| Error::Encode(e.to_string()))
}
