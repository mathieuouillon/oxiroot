//! Legend rendering — a framed box of sample swatches and labels, drawn in the
//! top-right of the axes (matplotlib's `loc="best"` lands there for most plots).

use crate::artists::{draw_marker, LegendHandle, Marker};
use crate::axes::Axes;
use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, Path, Rect, Seg, Stroke};
use crate::text::{self, FontStyle, HAlign, VAlign};

/// A rounded rectangle as a path (matplotlib `fancybox` legend frame). `r` is the
/// corner radius in pixels, clamped so it never exceeds half the shorter side.
fn rounded_rect(rect: Rect, r: f32) -> Path {
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    Path {
        segs: vec![
            Seg::Move(x + r, y),
            Seg::Line(x + w - r, y),
            Seg::Quad(x + w, y, x + w, y + r),
            Seg::Line(x + w, y + h - r),
            Seg::Quad(x + w, y + h, x + w - r, y + h),
            Seg::Line(x + r, y + h),
            Seg::Quad(x, y + h, x, y + h - r),
            Seg::Line(x, y + r),
            Seg::Quad(x, y, x + r, y),
            Seg::Close,
        ],
    }
}

/// Draw the legend into `g` (the unclipped axis group).
pub(crate) fn draw_legend(g: &mut DrawGroup, ax: &Axes, box_: Rect) {
    let items = ax.legend_items();
    if items.is_empty() {
        return;
    }
    let s = &ax.style;
    let fs = s.px(s.legend_size_pt);
    // Spacing in font-size units, matching matplotlib's legend rcParams:
    // borderpad 0.4, handlelength 2.0, handletextpad 0.8, labelspacing 0.5.
    let pad = 0.4 * fs;
    let gap = 0.8 * fs;
    let handle_w = 2.0 * fs;
    let row_h = fs * 1.5;

    let max_w = items
        .iter()
        .map(|it| text::measure(&s.fonts, &it.label, fs, FontStyle::Regular).width)
        .fold(0.0_f32, f32::max);

    let box_w = pad + handle_w + gap + max_w + pad;
    let box_h = pad * 2.0 + row_h * items.len() as f32;
    let margin = s.px(s.axes_linewidth_pt) + 0.5 * fs; // borderaxespad ~0.5
    let x0 = box_.right() - margin - box_w;
    let y0 = box_.y + margin;

    if s.legend_frame {
        // matplotlib `fancybox`: a rounded white box (framealpha 0.8) with a light
        // grey edge (legend.edgecolor "0.8") at the axes line width.
        g.push(DrawCommand::Path {
            path: rounded_rect(Rect::new(x0, y0, box_w, box_h), 0.2 * fs),
            fill: Some(Color::WHITE.with_alpha(0.8)),
            stroke: Some(Stroke::line(
                Color::hex("#cccccc"),
                s.px(s.axes_linewidth_pt),
            )),
        });
    }

    for (i, it) in items.iter().enumerate() {
        let cy = y0 + pad + row_h * i as f32 + row_h / 2.0;
        let hx0 = x0 + pad;
        let hx1 = hx0 + handle_w;
        let hcx = (hx0 + hx1) / 2.0;
        match &it.handle {
            LegendHandle::Line {
                color,
                width_pt,
                dash,
                marker,
                marker_size_pt,
            } => {
                if *width_pt > 0.0 {
                    let mut st = Stroke::new(*color, s.px(*width_pt));
                    st.dash = dash.as_ref().map(|d| d.iter().map(|v| s.px(*v)).collect());
                    g.push(DrawCommand::Line {
                        p0: (hx0, cy),
                        p1: (hx1, cy),
                        stroke: st,
                    });
                }
                if *marker != Marker::None {
                    draw_marker(
                        g,
                        *marker,
                        (hcx, cy),
                        s.px(*marker_size_pt),
                        Some(*color),
                        None,
                    );
                }
            }
            LegendHandle::Patch {
                face,
                edge,
                width_pt,
            } => {
                let h = fs * 0.8;
                g.push(DrawCommand::Rect {
                    rect: Rect::new(hx0, cy - h / 2.0, handle_w, h),
                    fill: *face,
                    stroke: edge.map(|c| Stroke::line(c, s.px(*width_pt))),
                });
            }
        }
        // Labels go through the math layout so `$…$` renders like axis labels.
        crate::mathtext::layout_label(
            g,
            &s.fonts,
            &it.label,
            hx1 + gap,
            cy,
            fs,
            s.fg_color,
            HAlign::Left,
            VAlign::Middle,
            0.0,
        );
    }
}
