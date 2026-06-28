//! Legend rendering — a framed box of sample swatches and labels, drawn in the
//! top-right of the axes (matplotlib's `loc="best"` lands there for most plots).

use crate::artists::{draw_marker, LegendHandle, Marker};
use crate::axes::Axes;
use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, Rect, Stroke};
use crate::text::{self, FontStyle, HAlign, VAlign};

/// Draw the legend into `g` (the unclipped axis group).
pub(crate) fn draw_legend(g: &mut DrawGroup, ax: &Axes, box_: Rect) {
    let items = ax.legend_items();
    if items.is_empty() {
        return;
    }
    let s = &ax.style;
    let fs = s.px(s.legend_size_pt);
    let pad = s.px(5.0);
    let gap = s.px(5.0);
    let handle_w = s.px(20.0);
    let row_h = fs * 1.4;

    let max_w = items
        .iter()
        .map(|it| text::measure(&it.label, fs, FontStyle::Regular).width)
        .fold(0.0_f32, f32::max);

    let box_w = pad + handle_w + gap + max_w + pad;
    let box_h = pad * 2.0 + row_h * items.len() as f32;
    let margin = s.px(6.0);
    let x0 = box_.right() - margin - box_w;
    let y0 = box_.y + margin;

    if s.legend_frame {
        g.push(DrawCommand::Rect {
            rect: Rect::new(x0, y0, box_w, box_h),
            fill: Some(Color::WHITE.with_alpha(0.8)),
            stroke: Some(Stroke::line(Color::hex("#cccccc"), s.px(0.8))),
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
