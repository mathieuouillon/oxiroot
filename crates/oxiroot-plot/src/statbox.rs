//! A ROOT-style fit statistics box — the analog of ROOT's `TPaveStats` with
//! `gStyle->SetOptFit`. After fitting, [`Axes::fit_stats`](crate::Axes::fit_stats)
//! draws a framed panel listing the function name, the χ²/ndf of the fit, and
//! every fitted parameter with its uncertainty.
//!
//! Everything here is behind the `fit` feature, since it reads an
//! [`oxiroot_fit::FitResult`].

use oxiroot_fit::{FitResult, Model};

use crate::axes::Axes;
use crate::color::Color;
use crate::draw::{DrawCommand, DrawGroup, Rect, Stroke};
use crate::text::{self, FontStyle, HAlign, VAlign};

/// Which corner of the frame the stat box is anchored to.
///
/// The default is [`Corner::UpperRight`], matching ROOT. When a legend is also
/// shown in the upper right, the stat box is stacked directly beneath it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Corner {
    /// Top-right of the frame (the ROOT default).
    #[default]
    UpperRight,
    /// Top-left of the frame.
    UpperLeft,
    /// Bottom-right of the frame.
    LowerRight,
    /// Bottom-left of the frame.
    LowerLeft,
}

/// Options for the fit statistics box — the analog of ROOT's
/// `gStyle->SetOptFit`. Build with [`StatBox::new`] and the chained setters, then
/// pass to [`Axes::fit_stats_with`](crate::Axes::fit_stats_with).
///
/// # Examples
/// ```
/// # use oxiroot_plot::{StatBox, Corner};
/// let opts = StatBox::new().corner(Corner::UpperLeft).prob(true).sig_figs(3);
/// ```
#[derive(Clone, Debug)]
pub struct StatBox {
    pub(crate) corner: Corner,
    pub(crate) show_name: bool,
    pub(crate) show_chi2: bool,
    pub(crate) show_prob: bool,
    pub(crate) show_errors: bool,
    pub(crate) sig_figs: usize,
    pub(crate) title: Option<String>,
}

impl Default for StatBox {
    fn default() -> Self {
        StatBox {
            corner: Corner::UpperRight,
            show_name: true,
            show_chi2: true,
            show_prob: false,
            show_errors: true,
            sig_figs: 4,
            title: None,
        }
    }
}

impl StatBox {
    /// A stat box with ROOT-like defaults: the function name, the χ²/ndf line,
    /// and every parameter with its error, anchored top-right, 4 significant
    /// figures. The goodness-of-fit probability is off by default (enable it with
    /// [`prob`](StatBox::prob)).
    #[must_use]
    pub fn new() -> Self {
        StatBox::default()
    }

    /// Anchor the box to a given [`Corner`] (default [`Corner::UpperRight`]).
    #[must_use]
    pub fn corner(mut self, corner: Corner) -> Self {
        self.corner = corner;
        self
    }

    /// Show the function name as the box header (default `true`). Overridden by
    /// [`title`](StatBox::title).
    #[must_use]
    pub fn name(mut self, show: bool) -> Self {
        self.show_name = show;
        self
    }

    /// Show the `χ²/ndf = <chi2> / <ndf>` line (default `true`).
    #[must_use]
    pub fn chi2(mut self, show: bool) -> Self {
        self.show_chi2 = show;
        self
    }

    /// Show the goodness-of-fit probability (`p`-value) line (default `false`).
    #[must_use]
    pub fn prob(mut self, show: bool) -> Self {
        self.show_prob = show;
        self
    }

    /// Show each parameter's `± error` (default `true`). A fixed parameter (zero
    /// error) is always shown without a `±`.
    #[must_use]
    pub fn errors(mut self, show: bool) -> Self {
        self.show_errors = show;
        self
    }

    /// Number of significant figures for the numbers (default `4`).
    #[must_use]
    pub fn sig_figs(mut self, sig: usize) -> Self {
        self.sig_figs = sig.max(1);
        self
    }

    /// Override the header text (default: the model's name).
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// One rendered line of the stat box.
#[derive(Clone, Debug)]
pub(crate) struct StatRow {
    text: String,
    bold: bool,
}

/// The resolved (pre-formatted) contents of a stat box, stored on the [`Axes`].
#[derive(Clone, Debug)]
pub(crate) struct StatData {
    rows: Vec<StatRow>,
    corner: Corner,
}

/// Format a value like C's `%g`: fixed notation with `sig` significant figures,
/// trailing zeros stripped, switching to scientific for very large/small values.
fn fmt_g(v: f64, sig: usize) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if v == 0.0 {
        return "0".to_string();
    }
    let sig = sig.max(1);
    let exp = v.abs().log10().floor() as i32;
    // %g uses scientific notation when the exponent is < -4 or >= the precision.
    let mut s = if exp < -4 || exp >= sig as i32 {
        format!("{:.*e}", sig - 1, v)
    } else {
        let decimals = (sig as i32 - 1 - exp).max(0) as usize;
        format!("{:.*}", decimals, v)
    };
    // Strip trailing zeros like %g — in the mantissa for scientific notation
    // (before the `e`), or the whole string for fixed notation.
    let end = s.find('e').unwrap_or(s.len());
    if s[..end].contains('.') {
        let mut mantissa_end = end;
        while s[..mantissa_end].ends_with('0') {
            mantissa_end -= 1;
        }
        if s[..mantissa_end].ends_with('.') {
            mantissa_end -= 1;
        }
        s.replace_range(mantissa_end..end, "");
    }
    s
}

/// Build the stat-box rows from a fitted model and its result.
pub(crate) fn build(model: &Model, result: &FitResult, opts: &StatBox) -> StatData {
    let mut rows = Vec::new();
    let sig = opts.sig_figs;

    // An explicit (non-empty) title always wins; otherwise the model name is the
    // header when `show_name` is on. So `title(..)` shows even with `name(false)`.
    let header = opts
        .title
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| (opts.show_name && !model.name.is_empty()).then(|| model.name.clone()));
    if let Some(header) = header {
        rows.push(StatRow {
            text: header,
            bold: true,
        });
    }
    if opts.show_chi2 {
        rows.push(StatRow {
            text: format!("χ²/ndf = {} / {}", fmt_g(result.chi2, sig), result.ndf),
            bold: false,
        });
    }
    if opts.show_prob {
        rows.push(StatRow {
            text: format!("Prob = {}", fmt_g(result.p_value(), sig)),
            bold: false,
        });
    }
    for (i, &value) in result.params.iter().enumerate() {
        let name = model
            .param_names
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("p{i}"));
        let err = result.errors.get(i).copied().unwrap_or(0.0);
        let text = if opts.show_errors && err.is_finite() && err != 0.0 {
            format!("{name} = {} ± {}", fmt_g(value, sig), fmt_g(err, sig))
        } else {
            format!("{name} = {}", fmt_g(value, sig))
        };
        rows.push(StatRow { text, bold: false });
    }

    StatData {
        rows,
        corner: opts.corner,
    }
}

/// Draw the stat box into `g` (the unclipped axis group). `legend_rect` is the
/// legend's bounding box when a legend is shown, so a top-right stat box can be
/// stacked beneath it.
pub(crate) fn draw_stats(
    g: &mut DrawGroup,
    ax: &Axes,
    frame: Rect,
    data: &StatData,
    legend_rect: Option<Rect>,
) {
    if data.rows.is_empty() {
        return;
    }
    let s = &ax.style;
    let fs = s.px(s.legend_size_pt);
    let pad = 0.5 * fs;
    let row_h = 1.45 * fs;

    let text_w = data
        .rows
        .iter()
        .map(|row| {
            let style = if row.bold {
                FontStyle::Bold
            } else {
                FontStyle::Regular
            };
            text::measure(&s.fonts, &row.text, fs, style).width
        })
        .fold(0.0_f32, f32::max);

    let box_w = text_w + 2.0 * pad;
    let box_h = data.rows.len() as f32 * row_h + 2.0 * pad;
    let margin = s.px(s.axes_linewidth_pt) + 0.5 * fs;

    let (x0, mut y0) = match data.corner {
        Corner::UpperRight => (frame.right() - margin - box_w, frame.y + margin),
        Corner::UpperLeft => (frame.x + margin, frame.y + margin),
        Corner::LowerRight => (
            frame.right() - margin - box_w,
            frame.bottom() - margin - box_h,
        ),
        Corner::LowerLeft => (frame.x + margin, frame.bottom() - margin - box_h),
    };
    // Stack beneath the legend when both are anchored top-right.
    if data.corner == Corner::UpperRight {
        if let Some(lr) = legend_rect {
            y0 = lr.bottom() + 0.5 * fs;
        }
    }
    // Keep the box pinned inside the frame (like ROOT's TPaveStats) so a long row
    // or a narrow frame cannot push it out over the ticks and axis labels.
    let x0 = x0.clamp(frame.x, (frame.right() - box_w).max(frame.x));
    let y0 = y0.clamp(frame.y, (frame.bottom() - box_h).max(frame.y));

    // The framed panel: an opaque white box with the axis-colour edge (ROOT's
    // TPaveStats is a plain rectangle, not the rounded legend fancybox).
    g.push(DrawCommand::Rect {
        rect: Rect::new(x0, y0, box_w, box_h),
        fill: Some(Color::WHITE.with_alpha(0.85)),
        stroke: Some(Stroke::line(s.fg_color, s.px(s.axes_linewidth_pt))),
    });

    for (i, row) in data.rows.iter().enumerate() {
        let cy = y0 + pad + row_h * i as f32 + row_h / 2.0;
        let style = if row.bold {
            FontStyle::Bold
        } else {
            FontStyle::Regular
        };
        g.cmds.extend(text::layout(
            &s.fonts,
            &row.text,
            x0 + pad,
            cy,
            fs,
            style,
            s.fg_color,
            HAlign::Left,
            VAlign::Middle,
            0.0,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{build, fmt_g, StatBox};

    #[test]
    fn fmt_g_matches_percent_g_style() {
        assert_eq!(fmt_g(0.0, 4), "0");
        assert_eq!(fmt_g(91.234, 4), "91.23");
        assert_eq!(fmt_g(100.0, 4), "100");
        assert_eq!(fmt_g(2.5, 4), "2.5");
        assert_eq!(fmt_g(0.0025, 4), "0.0025");
        assert_eq!(fmt_g(-1.5, 3), "-1.5");
        // scientific branch strips trailing mantissa zeros, like C's %g.
        assert_eq!(fmt_g(1_000_000.0, 4), "1e6");
        assert_eq!(fmt_g(120_000.0, 4), "1.2e5");
        assert_eq!(fmt_g(123_456.0, 4), "1.235e5");
        assert_eq!(fmt_g(1.0e-6, 3), "1e-6");
        // non-finite
        assert_eq!(fmt_g(f64::NAN, 4), "nan");
        assert_eq!(fmt_g(f64::INFINITY, 4), "inf");
    }

    #[test]
    fn build_handles_special_params_and_title() {
        use oxiroot_fit::{FitResult, Model};

        let model = Model::new("myfit", &["a", "b"], vec![0.0, 0.0], |_x, _p| 0.0);
        let r = FitResult {
            params: vec![1234.5, 2.0],
            errors: vec![f64::NAN, 0.0], // a NaN error, and a fixed parameter (0 error)
            minos: None,
            covariance: None,
            chi2: 10.0,
            ndf: 8,
            valid: true,
        };

        let rows = build(&model, &r, &StatBox::new());
        let texts: Vec<&str> = rows.rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts[0], "myfit"); // header = model name
        assert!(rows.rows[0].bold);
        assert!(texts[1].starts_with("χ²/ndf = 10"));
        assert!(!texts[2].contains('±'), "NaN error → no ±: {}", texts[2]);
        assert!(!texts[3].contains('±'), "fixed param → no ±: {}", texts[3]);

        // A title wins even when the auto name is switched off.
        let with_title = build(&model, &r, &StatBox::new().name(false).title("Custom"));
        assert_eq!(with_title.rows[0].text, "Custom");
        assert!(with_title.rows[0].bold);

        // name(false) with no title → no header (first row is the χ²/ndf line).
        let no_header = build(&model, &r, &StatBox::new().name(false));
        assert!(no_header.rows[0].text.starts_with("χ²/ndf"));
    }
}
