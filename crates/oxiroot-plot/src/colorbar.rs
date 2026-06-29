//! Colorbar rendering for the TH2 heatmap.

use crate::cmap::Colormap;
use crate::draw::{DrawCommand, DrawGroup, Rect, Stroke};
use crate::style::Style;
use crate::text::{self, FontStyle, HAlign, VAlign};
use crate::ticker;

/// Everything needed to draw a colorbar next to the axes.
#[derive(Debug, Clone)]
pub(crate) struct ColorbarSpec {
    pub vmin: f64,
    pub vmax: f64,
    pub cmap: Colormap,
    pub label: Option<String>,
}

/// Draw the colorbar gradient, frame, ticks, labels, and optional axis label.
pub(crate) fn draw_colorbar(g: &mut DrawGroup, rect: Rect, spec: &ColorbarSpec, s: &Style) {
    let fg = s.fg_color;

    // Gradient: stacked strips from vmin (bottom) to vmax (top).
    let n = 128;
    for j in 0..n {
        let frac = j as f64 / (n - 1) as f64;
        let color = spec.cmap.sample(frac);
        let y_top = rect.bottom() - ((j + 1) as f32 / n as f32) * rect.h;
        let h = rect.h / n as f32 + 0.6; // small overlap to avoid seams
        g.push(DrawCommand::Rect {
            rect: Rect::new(rect.x, y_top, rect.w, h),
            fill: Some(color),
            stroke: None,
        });
    }

    // Frame.
    g.push(DrawCommand::Rect {
        rect,
        fill: None,
        stroke: Some(Stroke::line(fg, s.px(s.axes_linewidth_pt))),
    });

    // Ticks + labels on the right edge.
    let span = (spec.vmax - spec.vmin).max(f64::MIN_POSITIVE);
    let target = ((rect.h / 50.0).round() as usize).clamp(3, 9);
    let ticks = ticker::ticks(spec.vmin, spec.vmax, target);
    let step = ticker::nice_step(spec.vmin, spec.vmax, target);
    let labels = ticker::format_ticks(&ticks, step);
    let tlen = s.px(s.tick_major_len_pt);
    let pad = s.px(s.tick_pad_pt);
    let tlab = s.px(s.tick_label_size_pt);
    let tickstroke = Stroke::line(fg, s.px(s.tick_major_width_pt));

    let mut max_w = 0.0_f32;
    for (&v, lab) in ticks.iter().zip(&labels) {
        if v < spec.vmin || v > spec.vmax {
            continue;
        }
        let py = rect.bottom() - ((v - spec.vmin) / span) as f32 * rect.h;
        g.push(DrawCommand::Line {
            p0: (rect.right(), py),
            p1: (rect.right() + tlen, py),
            stroke: tickstroke.clone(),
        });
        max_w = max_w.max(text::measure(&s.fonts, lab, tlab, FontStyle::Regular).width);
        g.extend(text::layout(
            &s.fonts,
            lab,
            rect.right() + tlen + pad,
            py,
            tlab,
            FontStyle::Regular,
            fg,
            HAlign::Left,
            VAlign::Middle,
            0.0,
        ));
    }

    // Colorbar label (rotated), to the right of the tick labels.
    if let Some(lbl) = &spec.label {
        let x = rect.right() + tlen + pad + max_w + s.px(6.0);
        crate::mathtext::layout_label(
            g,
            &s.fonts,
            lbl,
            x,
            rect.y + rect.h / 2.0,
            s.px(s.label_size_pt),
            fg,
            HAlign::Center,
            VAlign::Baseline,
            -90.0,
        );
    }
}
