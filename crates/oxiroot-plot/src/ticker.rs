//! Tick location and formatting — a "nice numbers" locator (like matplotlib's
//! `MaxNLocator`) and a plain decimal/scientific formatter (`ScalarFormatter`).

/// Round `x` to a "nice" number (1, 2, 5, or 10 × a power of ten).
fn nice_num(x: f64, round: bool) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let exp = x.log10().floor();
    let frac = x / 10f64.powf(exp);
    let nice = if round {
        if frac < 1.5 {
            1.0
        } else if frac < 3.0 {
            2.0
        } else if frac < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if frac <= 1.0 {
        1.0
    } else if frac <= 2.0 {
        2.0
    } else if frac <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * 10f64.powf(exp)
}

/// Major tick step for a `[lo, hi]` range targeting roughly `target` ticks.
#[must_use]
pub fn nice_step(lo: f64, hi: f64, target: usize) -> f64 {
    let span = (hi - lo).abs();
    if span < f64::EPSILON || !span.is_finite() {
        return 1.0;
    }
    let range = nice_num(span, false);
    nice_num(range / (target.max(2) - 1) as f64, true)
}

/// Major tick positions within `[lo, hi]` (inclusive, with a tiny tolerance).
#[must_use]
pub fn ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let step = nice_step(lo, hi, target);
    if step <= 0.0 || !step.is_finite() {
        return vec![lo, hi];
    }
    let start = (lo / step).floor() * step;
    let mut out = Vec::new();
    let tol = step * 1e-6;
    let mut v = start;
    // Bound the loop defensively.
    for _ in 0..1000 {
        if v > hi + tol {
            break;
        }
        if v >= lo - tol {
            // Snap values extremely close to zero to exactly zero.
            out.push(if v.abs() < tol { 0.0 } else { v });
        }
        v += step;
    }
    if out.is_empty() {
        out.push(lo);
        out.push(hi);
    }
    out
}

/// Minor tick positions between the majors (matplotlib `AutoMinorLocator`):
/// subdivide each major interval into `n` parts.
#[must_use]
pub fn minor_ticks(lo: f64, hi: f64, majors: &[f64], n: usize) -> Vec<f64> {
    if majors.len() < 2 || n < 2 {
        return Vec::new();
    }
    let step = majors[1] - majors[0];
    let sub = step / n as f64;
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let tol = step * 1e-6;
    let mut out = Vec::new();
    // Extend one major step beyond the ends so partial intervals get minors too.
    let start = majors[0] - step;
    let mut v = start;
    for _ in 0..10000 {
        if v > hi + tol {
            break;
        }
        // Skip positions coinciding with a major.
        let on_major = ((v - majors[0]) / step).round();
        let nearest_major = majors[0] + on_major * step;
        if (v - nearest_major).abs() > tol && v >= lo - tol && v <= hi + tol {
            out.push(v);
        }
        v += sub;
    }
    out
}

/// Number of decimal places needed to show ticks at the given step.
fn decimals_for_step(step: f64) -> usize {
    if step <= 0.0 || !step.is_finite() {
        return 1;
    }
    let d = -step.log10().floor();
    d.clamp(0.0, 10.0) as usize
}

/// Format a set of tick values (a plain `ScalarFormatter`). Uses fixed decimals
/// derived from the step, switching to scientific notation for very large or
/// very small magnitudes.
#[must_use]
pub fn format_ticks(values: &[f64], step: f64) -> Vec<String> {
    let max_abs = values.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let use_sci = max_abs != 0.0 && !(1e-4..1e5).contains(&max_abs);
    let decimals = decimals_for_step(step);
    values
        .iter()
        .map(|&v| {
            let v = if v == 0.0 { 0.0 } else { v }; // normalize -0
            if use_sci {
                format_sci(v)
            } else {
                let s = format!("{v:.decimals$}");
                // Normalize a "-0" / "-0.00" result.
                if s.trim_start_matches('-')
                    .chars()
                    .all(|c| c == '0' || c == '.')
                {
                    s.trim_start_matches('-').to_string()
                } else {
                    s
                }
            }
        })
        .collect()
}

fn format_sci(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let exp = v.abs().log10().floor() as i32;
    let mantissa = v / 10f64.powi(exp);
    let m = format!("{mantissa:.1}");
    let m = m.trim_end_matches('0').trim_end_matches('.');
    format!("{m}e{exp}")
}
