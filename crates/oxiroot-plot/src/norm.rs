//! Color normalization for 2-D heatmaps — how a bin value maps to a `[0, 1]`
//! position in the colormap (matplotlib's `Normalize` / `LogNorm` /
//! `SymLogNorm`).

/// How heatmap values are scaled onto the colormap.
///
/// The default is [`Norm::Linear`]. [`Norm::Log`] gives a decade colorbar and
/// masks non-positive bins (like matplotlib `LogNorm`); [`Norm::SymLog`] is a
/// symmetric log that stays linear within `±linthresh` so data straddling zero
/// still works.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum Norm {
    /// Linear scaling: `(v − vmin) / (vmax − vmin)`.
    #[default]
    Linear,
    /// Base-10 log scaling. Non-positive values (and a non-positive `vmin`) are
    /// masked — those cells are left undrawn.
    Log,
    /// Symmetric log: linear within `±linthresh` of zero, logarithmic beyond.
    SymLog {
        /// The half-width of the linear region around zero.
        linthresh: f64,
    },
}

impl Norm {
    /// Map `v` onto `[0, 1]` given the value range `[vmin, vmax]`, or `None` when
    /// the value cannot be shown under this norm (e.g. a non-positive value under
    /// [`Norm::Log`]) so the caller leaves that cell undrawn.
    #[must_use]
    pub fn normalize(&self, v: f64, vmin: f64, vmax: f64) -> Option<f64> {
        match *self {
            Norm::Linear => {
                let span = vmax - vmin;
                let span = if span.abs() < f64::EPSILON { 1.0 } else { span };
                Some(((v - vmin) / span).clamp(0.0, 1.0))
            }
            Norm::Log => {
                if v <= 0.0 || vmin <= 0.0 || vmax <= 0.0 {
                    return None;
                }
                let (lo, hi) = (vmin.ln(), vmax.ln());
                let span = hi - lo;
                if span.abs() < f64::EPSILON {
                    return Some(0.0);
                }
                Some(((v.ln() - lo) / span).clamp(0.0, 1.0))
            }
            Norm::SymLog { linthresh } => {
                let f = symlog(linthresh);
                let (a, b) = (f(vmin), f(vmax));
                let span = b - a;
                if span.abs() < f64::EPSILON {
                    return Some(0.0);
                }
                Some(((f(v) - a) / span).clamp(0.0, 1.0))
            }
        }
    }

    /// Colorbar tick values and labels for the range `[vmin, vmax]`. `target` is
    /// the desired number of ticks (a hint). Positions are found by feeding each
    /// value back through [`normalize`](Self::normalize).
    #[must_use]
    pub(crate) fn colorbar_ticks(&self, vmin: f64, vmax: f64, target: usize) -> Vec<(f64, String)> {
        match *self {
            Norm::Linear => {
                let ticks = crate::ticker::ticks(vmin, vmax, target);
                let step = crate::ticker::nice_step(vmin, vmax, target);
                let labels = crate::ticker::format_ticks(&ticks, step);
                ticks.into_iter().zip(labels).collect()
            }
            Norm::Log => decade_ticks(vmin, vmax),
            Norm::SymLog { linthresh } => {
                // Zero, plus decade ticks on whichever sides have range.
                let mut out = vec![(0.0, "0".to_string())];
                if vmax > linthresh {
                    out.extend(decade_ticks(linthresh.max(f64::MIN_POSITIVE), vmax));
                }
                if vmin < -linthresh {
                    out.extend(
                        decade_ticks(linthresh.max(f64::MIN_POSITIVE), -vmin)
                            .into_iter()
                            .map(|(v, _)| (-v, format!("-{}", fmt_value(v)))),
                    );
                }
                out
            }
        }
    }
}

/// The symlog forward transform used for `SymLog` (matplotlib's shape):
/// `sign(x) · log10(1 + |x| / linthresh)`.
fn symlog(linthresh: f64) -> impl Fn(f64) -> f64 {
    let lt = linthresh.max(f64::MIN_POSITIVE);
    move |x: f64| x.signum() * (1.0 + x.abs() / lt).log10()
}

/// Power-of-ten ticks spanning `[vmin, vmax]` (both > 0). When the span is under
/// two decades, `2·`, `3·`, `5·` subticks are added so the bar is not bare.
fn decade_ticks(vmin: f64, vmax: f64) -> Vec<(f64, String)> {
    if !(vmin > 0.0 && vmax > vmin) {
        return Vec::new();
    }
    let k0 = vmin.log10().floor() as i32;
    let k1 = vmax.log10().ceil() as i32;
    let decades = (k1 - k0).max(1);
    let subs: &[f64] = if decades <= 2 {
        &[1.0, 2.0, 3.0, 5.0]
    } else {
        &[1.0]
    };
    let mut out = Vec::new();
    for k in k0..=k1 {
        let base = 10f64.powi(k);
        for &m in subs {
            let v = m * base;
            if v >= vmin * (1.0 - 1e-9) && v <= vmax * (1.0 + 1e-9) {
                out.push((v, fmt_value(v)));
            }
        }
    }
    out
}

/// Compact number formatting for colorbar tick labels: plain in a friendly
/// magnitude range, scientific (`1e6`) otherwise.
fn fmt_value(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let a = v.abs();
    if (1e-4..1e5).contains(&a) {
        let s = format!("{v:.4}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    } else {
        // e.g. 1000000 -> "1e6", 0.00001 -> "1e-5"
        let e = a.log10().round() as i32;
        let mant = v / 10f64.powi(e);
        if (mant - 1.0).abs() < 1e-9 {
            format!("1e{e}")
        } else {
            let m = format!("{mant:.2}");
            let m = m.trim_end_matches('0').trim_end_matches('.');
            format!("{m}e{e}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_maps_endpoints() {
        assert_eq!(Norm::Linear.normalize(0.0, 0.0, 10.0), Some(0.0));
        assert_eq!(Norm::Linear.normalize(10.0, 0.0, 10.0), Some(1.0));
        assert_eq!(Norm::Linear.normalize(5.0, 0.0, 10.0), Some(0.5));
    }

    #[test]
    fn log_masks_nonpositive_and_maps_decades() {
        assert_eq!(Norm::Log.normalize(-1.0, 1.0, 100.0), None);
        assert_eq!(Norm::Log.normalize(0.0, 1.0, 100.0), None);
        assert_eq!(Norm::Log.normalize(1.0, 1.0, 100.0), Some(0.0));
        assert_eq!(Norm::Log.normalize(100.0, 1.0, 100.0), Some(1.0));
        // 10 is the geometric midpoint of [1, 100].
        let mid = Norm::Log.normalize(10.0, 1.0, 100.0).unwrap();
        assert!((mid - 0.5).abs() < 1e-9);
    }

    #[test]
    fn log_colorbar_has_decade_ticks() {
        let ticks = Norm::Log.colorbar_ticks(1.0, 1000.0, 5);
        let vals: Vec<f64> = ticks.iter().map(|(v, _)| *v).collect();
        assert!(vals.contains(&1.0) && vals.contains(&10.0) && vals.contains(&1000.0));
    }
}
