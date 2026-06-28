//! The backend-independent drawing IR.
//!
//! A figure is rendered to a list of [`DrawGroup`]s in **pixel coordinates**
//! (origin top-left, y increasing downward — the convention shared by tiny-skia
//! and SVG). Both the raster and SVG renderers consume the same IR, so the two
//! outputs share identical geometry.

use crate::color::Color;

/// A point in pixel space.
pub type Pt = (f32, f32);

/// An axis-aligned rectangle in pixel space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Construct from position and size.
    #[must_use]
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
    /// Right edge (`x + w`).
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    /// Bottom edge (`y + h`).
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Line cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    /// Square butt (matplotlib `projecting`/`butt`).
    #[default]
    Butt,
    /// Rounded cap (matplotlib default for data lines).
    Round,
    /// Projecting square cap. Part of the complete stroke model; no current
    /// artist emits it.
    #[allow(dead_code)]
    Square,
}

/// Line join style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    /// Mitered corner.
    Miter,
    /// Rounded corner (matplotlib default).
    #[default]
    Round,
    /// Beveled corner. Part of the complete stroke model; no current artist
    /// emits it.
    #[allow(dead_code)]
    Bevel,
}

/// A stroke style.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// Stroke color.
    pub color: Color,
    /// Stroke width in pixels.
    pub width: f32,
    /// Cap style.
    pub cap: LineCap,
    /// Join style.
    pub join: LineJoin,
    /// Optional dash pattern (on/off lengths in pixels).
    pub dash: Option<Vec<f32>>,
}

impl Stroke {
    /// A solid stroke with the default round cap/join (matplotlib line style).
    #[must_use]
    pub fn new(color: Color, width: f32) -> Self {
        Stroke {
            color,
            width,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dash: None,
        }
    }

    /// A solid stroke with butt caps and miter joins (axis spines, frames).
    #[must_use]
    pub fn line(color: Color, width: f32) -> Self {
        Stroke {
            color,
            width,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            dash: None,
        }
    }
}

/// A path segment in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    /// Start a new subpath at the point.
    Move(f32, f32),
    /// Straight line to the point.
    Line(f32, f32),
    /// Quadratic Bézier (control, end).
    Quad(f32, f32, f32, f32),
    /// Cubic Bézier (control1, control2, end).
    Cubic(f32, f32, f32, f32, f32, f32),
    /// Close the current subpath.
    Close,
}

/// A path: a sequence of segments. Used for glyph outlines and arbitrary shapes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    /// The ordered segments.
    pub segs: Vec<Seg>,
}

impl Path {
    /// An empty path.
    #[must_use]
    pub fn new() -> Self {
        Path::default()
    }
    /// `true` if the path has no segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }
}

/// One primitive to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// An open polyline (no implicit close); stroked.
    Polyline {
        /// Vertices.
        pts: Vec<Pt>,
        /// Stroke style.
        stroke: Stroke,
    },
    /// A closed polygon; optionally filled and/or stroked.
    Polygon {
        /// Vertices (auto-closed).
        pts: Vec<Pt>,
        /// Optional fill.
        fill: Option<Color>,
        /// Optional outline.
        stroke: Option<Stroke>,
    },
    /// A single line segment.
    Line {
        /// Start point.
        p0: Pt,
        /// End point.
        p1: Pt,
        /// Stroke style.
        stroke: Stroke,
    },
    /// An axis-aligned rectangle.
    Rect {
        /// Geometry.
        rect: Rect,
        /// Optional fill.
        fill: Option<Color>,
        /// Optional outline.
        stroke: Option<Stroke>,
    },
    /// A circle (markers, points).
    Circle {
        /// Center.
        c: Pt,
        /// Radius in pixels.
        r: f32,
        /// Optional fill.
        fill: Option<Color>,
        /// Optional outline.
        stroke: Option<Stroke>,
    },
    /// An arbitrary filled/stroked path (glyph outlines, math).
    Path {
        /// The path geometry.
        path: Path,
        /// Optional fill.
        fill: Option<Color>,
        /// Optional outline.
        stroke: Option<Stroke>,
    },
}

/// A set of draw commands sharing an optional clip rectangle. Data artists are
/// emitted in a group clipped to the axes box; frames, ticks, and labels are
/// emitted unclipped.
#[derive(Debug, Clone, Default)]
pub struct DrawGroup {
    /// Clip rectangle, or `None` for no clipping.
    pub clip: Option<Rect>,
    /// The commands in this group.
    pub cmds: Vec<DrawCommand>,
}

impl DrawGroup {
    /// A new group with the given clip.
    #[must_use]
    pub fn new(clip: Option<Rect>) -> Self {
        DrawGroup {
            clip,
            cmds: Vec::new(),
        }
    }
    /// Append a command.
    pub fn push(&mut self, cmd: DrawCommand) {
        self.cmds.push(cmd);
    }
    /// Append several commands.
    pub fn extend(&mut self, cmds: impl IntoIterator<Item = DrawCommand>) {
        self.cmds.extend(cmds);
    }
}
