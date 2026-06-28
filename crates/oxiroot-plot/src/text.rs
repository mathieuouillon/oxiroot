//! Text layout and rendering via bundled DejaVu Sans (matplotlib's default font).
//!
//! Text is converted to filled glyph-outline [`Path`]s in pixel coordinates, so
//! the raster (tiny-skia) and SVG renderers draw identical glyphs — the output
//! is self-contained and font-independent on the viewer's side.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, OutlineCurve};

use crate::color::Color;
use crate::draw::{DrawCommand, Path, Pt, Seg};

/// Font weight/slant, mirroring matplotlib's `normal`/`bold`/`italic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// Upright regular.
    #[default]
    Regular,
    /// Bold.
    Bold,
    /// Oblique/italic.
    Italic,
}

static DEJAVU_REGULAR: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
static DEJAVU_BOLD: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");
static DEJAVU_OBLIQUE: &[u8] = include_bytes!("../assets/DejaVuSans-Oblique.ttf");

fn face(style: FontStyle) -> &'static FontRef<'static> {
    fn load(
        bytes: &'static [u8],
        cell: &'static OnceLock<FontRef<'static>>,
    ) -> &'static FontRef<'static> {
        cell.get_or_init(|| FontRef::try_from_slice(bytes).expect("bundled DejaVu Sans is valid"))
    }
    static REG: OnceLock<FontRef<'static>> = OnceLock::new();
    static BOLD: OnceLock<FontRef<'static>> = OnceLock::new();
    static OBL: OnceLock<FontRef<'static>> = OnceLock::new();
    match style {
        FontStyle::Regular => load(DEJAVU_REGULAR, &REG),
        FontStyle::Bold => load(DEJAVU_BOLD, &BOLD),
        FontStyle::Italic => load(DEJAVU_OBLIQUE, &OBL),
    }
}

/// Horizontal anchor of a text block relative to its draw position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    /// Position is the left edge.
    #[default]
    Left,
    /// Position is the horizontal center.
    Center,
    /// Position is the right edge.
    Right,
}

/// Vertical anchor of a text block relative to its draw position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    /// Position is the text baseline.
    #[default]
    Baseline,
    /// Position is the top of the cap region.
    Top,
    /// Position is the vertical middle.
    Middle,
    /// Position is the bottom (descent).
    Bottom,
}

/// Measured extents of a string at a given pixel size.
#[derive(Debug, Clone, Copy)]
pub struct TextExtents {
    /// Total advance width in pixels.
    pub width: f32,
    /// Ascent above the baseline in pixels (positive).
    pub ascent: f32,
    /// Descent below the baseline in pixels (positive).
    pub descent: f32,
}

impl TextExtents {
    /// Total line height (ascent + descent).
    #[must_use]
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// Measure a single-line string rendered at `size_px` in the given style.
#[must_use]
pub fn measure(text: &str, size_px: f32, style: FontStyle) -> TextExtents {
    let font = face(style);
    let upem = font.units_per_em().unwrap_or(2048.0);
    let f = size_px / upem;
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            width += font.kern_unscaled(p, id) * f;
        }
        width += font.h_advance_unscaled(id) * f;
        prev = Some(id);
    }
    TextExtents {
        width,
        ascent: font.ascent_unscaled() * f,
        descent: -font.descent_unscaled() * f,
    }
}

/// Lay out a single-line string into filled glyph-outline draw commands.
///
/// `(x, y)` is the anchor point; `halign`/`valign` say what part of the text
/// block that point refers to. `rotation_deg` rotates the block about the anchor
/// (positive = clockwise in screen space; use `-90.0` for an upward y-axis label).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn layout(
    text: &str,
    x: f32,
    y: f32,
    size_px: f32,
    style: FontStyle,
    color: Color,
    halign: HAlign,
    valign: VAlign,
    rotation_deg: f32,
) -> Vec<DrawCommand> {
    let font = face(style);
    let upem = font.units_per_em().unwrap_or(2048.0);
    let f = size_px / upem;
    let ext = measure(text, size_px, style);

    // Pen start (local x of the first glyph origin) from the horizontal anchor.
    let mut pen = match halign {
        HAlign::Left => 0.0,
        HAlign::Center => -ext.width / 2.0,
        HAlign::Right => -ext.width,
    };
    // Baseline local y from the vertical anchor (local frame is y-down).
    let baseline = match valign {
        VAlign::Baseline => 0.0,
        VAlign::Top => ext.ascent,
        VAlign::Middle => (ext.ascent - ext.descent) / 2.0,
        VAlign::Bottom => -ext.descent,
    };

    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let to_screen = |lx: f32, ly: f32| -> Pt { (x + lx * cos - ly * sin, y + lx * sin + ly * cos) };

    let mut cmds = Vec::new();
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            pen += font.kern_unscaled(p, id) * f;
        }
        if let Some(outline) = font.outline(id) {
            let pen_now = pen;
            // Map a font-unit point (y-up) to local frame (y-down) then to screen.
            let path = build_glyph_path(&outline.curves, |gx, gy| {
                to_screen(pen_now + gx * f, baseline - gy * f)
            });
            if !path.is_empty() {
                cmds.push(DrawCommand::Path {
                    path,
                    fill: Some(color),
                    stroke: None,
                });
            }
        }
        pen += font.h_advance_unscaled(id) * f;
        prev = Some(id);
    }
    cmds
}

/// Glyph outline paths for a string in a **local** frame: baseline at `y = 0`,
/// pen starting at `x = 0`. Used by the mixed text+math label layout.
pub(crate) fn glyph_paths_local(text: &str, size_px: f32, style: FontStyle) -> Vec<Path> {
    let font = face(style);
    let upem = font.units_per_em().unwrap_or(2048.0);
    let f = size_px / upem;
    let mut pen = 0.0;
    let mut prev = None;
    let mut out = Vec::new();
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            pen += font.kern_unscaled(p, id) * f;
        }
        if let Some(outline) = font.outline(id) {
            let pen_now = pen;
            let path = build_glyph_path(&outline.curves, |gx, gy| (pen_now + gx * f, -gy * f));
            if !path.is_empty() {
                out.push(path);
            }
        }
        pen += font.h_advance_unscaled(id) * f;
        prev = Some(id);
    }
    out
}

/// Build a closed [`Path`] from glyph outline curves, mapping each font-unit
/// point through `map`. ab_glyph emits curves contour-by-contour; a jump in the
/// start point marks a new contour, so the previous one is closed and re-moved.
fn build_glyph_path(curves: &[OutlineCurve], map: impl Fn(f32, f32) -> Pt) -> Path {
    let mut path = Path::new();
    let mut open = false;
    let mut last_end: Option<ab_glyph::Point> = None;
    for curve in curves {
        let start = curve_start(curve);
        if last_end != Some(start) {
            if open {
                path.segs.push(Seg::Close);
            }
            let (sx, sy) = map(start.x, start.y);
            path.segs.push(Seg::Move(sx, sy));
            open = true;
        }
        match curve {
            OutlineCurve::Line(_, p1) => {
                let (px, py) = map(p1.x, p1.y);
                path.segs.push(Seg::Line(px, py));
                last_end = Some(*p1);
            }
            OutlineCurve::Quad(_, c, p1) => {
                let (cx, cy) = map(c.x, c.y);
                let (px, py) = map(p1.x, p1.y);
                path.segs.push(Seg::Quad(cx, cy, px, py));
                last_end = Some(*p1);
            }
            OutlineCurve::Cubic(_, c1, c2, p1) => {
                let (c1x, c1y) = map(c1.x, c1.y);
                let (c2x, c2y) = map(c2.x, c2.y);
                let (px, py) = map(p1.x, p1.y);
                path.segs.push(Seg::Cubic(c1x, c1y, c2x, c2y, px, py));
                last_end = Some(*p1);
            }
        }
    }
    if open {
        path.segs.push(Seg::Close);
    }
    path
}

fn curve_start(curve: &OutlineCurve) -> ab_glyph::Point {
    match curve {
        OutlineCurve::Line(p0, _)
        | OutlineCurve::Quad(p0, _, _)
        | OutlineCurve::Cubic(p0, _, _, _) => *p0,
    }
}
