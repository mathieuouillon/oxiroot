//! Data-space bounds and the data→pixel transform.

use crate::draw::{Pt, Rect};

/// A 2-D data-coordinate bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Minimum x.
    pub xmin: f64,
    /// Maximum x.
    pub xmax: f64,
    /// Minimum y.
    pub ymin: f64,
    /// Maximum y.
    pub ymax: f64,
}

impl Bounds {
    /// A bounds from explicit extents.
    #[must_use]
    pub fn new(xmin: f64, xmax: f64, ymin: f64, ymax: f64) -> Self {
        Bounds {
            xmin,
            xmax,
            ymin,
            ymax,
        }
    }

    /// The union of two bounds.
    #[must_use]
    pub fn union(self, other: Bounds) -> Bounds {
        Bounds {
            xmin: self.xmin.min(other.xmin),
            xmax: self.xmax.max(other.xmax),
            ymin: self.ymin.min(other.ymin),
            ymax: self.ymax.max(other.ymax),
        }
    }

    /// Expand x and y ranges by a fractional margin on each side.
    #[must_use]
    pub fn padded(self, frac_x: f64, frac_y: f64) -> Bounds {
        let dx = (self.xmax - self.xmin) * frac_x;
        let dy = (self.ymax - self.ymin) * frac_y;
        // Guard against a degenerate (zero-width) range.
        let dx = if dx.abs() < f64::EPSILON { 0.5 } else { dx };
        let dy = if dy.abs() < f64::EPSILON { 0.5 } else { dy };
        Bounds {
            xmin: self.xmin - dx,
            xmax: self.xmax + dx,
            ymin: self.ymin - dy,
            ymax: self.ymax + dy,
        }
    }
}

/// Maps data coordinates to pixel coordinates within an axes box. The y axis is
/// inverted (larger data y → smaller pixel y), matching screen conventions.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    box_: Rect,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
}

impl Transform {
    /// Build a transform mapping `[xmin,xmax]×[ymin,ymax]` onto `box_`.
    #[must_use]
    pub fn new(box_: Rect, xmin: f64, xmax: f64, ymin: f64, ymax: f64) -> Self {
        Transform {
            box_,
            xmin,
            xmax,
            ymin,
            ymax,
        }
    }

    /// Map a data x to a pixel x.
    #[must_use]
    pub fn x(&self, dx: f64) -> f32 {
        let span = self.xmax - self.xmin;
        let t = if span.abs() < f64::EPSILON {
            0.5
        } else {
            (dx - self.xmin) / span
        };
        self.box_.x + (t as f32) * self.box_.w
    }

    /// Map a data y to a pixel y (inverted).
    #[must_use]
    pub fn y(&self, dy: f64) -> f32 {
        let span = self.ymax - self.ymin;
        let t = if span.abs() < f64::EPSILON {
            0.5
        } else {
            (dy - self.ymin) / span
        };
        self.box_.bottom() - (t as f32) * self.box_.h
    }

    /// Map a data point to a pixel point.
    #[must_use]
    pub fn pt(&self, p: (f64, f64)) -> Pt {
        (self.x(p.0), self.y(p.1))
    }

    /// The axes box in pixels.
    #[must_use]
    pub fn box_(&self) -> Rect {
        self.box_
    }
}
