//! Time axes: tick positions and labels for values that are times.
//!
//! A ROOT time axis holds seconds counted from an epoch, and a `fTimeFormat`
//! that says how a label reads — a `strftime` format, optionally followed by
//! `%F` and the epoch itself (`"%H:%M%F2024-01-01 00:00:00"`). This module reads
//! that format, picks tick steps a person would pick (5 minutes, 6 hours, a day
//! — never 4,237 seconds), and renders the labels.
//!
//! The supported format specifiers are `%Y %y %m %d %H %M %S %j %b %B %a %A %p`
//! and `%%`; anything else is copied through, so an unknown one shows itself
//! rather than disappearing.

/// A time axis's format: how a label reads, and the epoch its values count from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeFormat {
    /// The `strftime`-style format for a label.
    pub format: String,
    /// Seconds between the Unix epoch and the epoch the axis counts from, as
    /// ROOT's `%F` suffix gives it (`0` when the format carries none).
    pub offset: i64,
}

impl TimeFormat {
    /// Read a ROOT `fTimeFormat`: the label format, and the epoch after `%F`
    /// when it carries one (`"%H:%M%F2024-01-01 00:00:00"`). An empty format
    /// falls back to one that shows the date and the time.
    #[must_use]
    pub fn parse(time_format: &str) -> TimeFormat {
        let (format, epoch) = match time_format.find("%F") {
            Some(at) => (&time_format[..at], Some(&time_format[at + 2..])),
            None => (time_format, None),
        };
        let format = if format.is_empty() {
            "%Y-%m-%d %H:%M:%S"
        } else {
            format
        };
        TimeFormat {
            format: format.to_string(),
            offset: epoch.map_or(0, parse_epoch),
        }
    }

    /// The label for `value`, a number of seconds on this axis.
    #[must_use]
    pub fn label(&self, value: f64) -> String {
        format_time(
            self.offset.saturating_add(value.round() as i64),
            &self.format,
        )
    }
}

/// Read ROOT's `%F` epoch, `"1995-01-01 00:00:00"`, as seconds from the Unix
/// epoch. Anything it cannot read is `0` — the Unix epoch itself.
fn parse_epoch(text: &str) -> i64 {
    let text = text.trim();
    let (date, time) = text.split_once(' ').unwrap_or((text, "00:00:00"));
    let mut date = date.split('-').map(str::parse::<i64>);
    let (Some(Ok(year)), Some(Ok(month)), Some(Ok(day))) = (date.next(), date.next(), date.next())
    else {
        return 0;
    };
    let mut clock = time.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next().and_then(Result::ok).unwrap_or(0),
        clock.next().and_then(Result::ok).unwrap_or(0),
        clock.next().and_then(Result::ok).unwrap_or(0),
    );
    days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
}

/// Days from the Unix epoch to `year-month-day` (proleptic Gregorian), by
/// Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400; // [0, 399]
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1; // [0, 365]
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse: `year-month-day` from days since the Unix epoch.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11], March = 0
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Thursday", // the Unix epoch, day 0, was a Thursday
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// Render `seconds` (from the Unix epoch, UTC) through a `strftime` subset.
#[must_use]
pub fn format_time(seconds: i64, format: &str) -> String {
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (in_day / 3600, (in_day / 60) % 60, in_day % 60);
    let month_name = MONTHS[(month as usize).clamp(1, 12) - 1];
    let weekday = WEEKDAYS[days.rem_euclid(7) as usize];
    let day_of_year = days - days_from_civil(year, 1, 1) + 1;

    let mut out = String::with_capacity(format.len() + 8);
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&year.to_string()),
            Some('y') => out.push_str(&format!("{:02}", year.rem_euclid(100))),
            Some('m') => out.push_str(&format!("{month:02}")),
            Some('d') => out.push_str(&format!("{day:02}")),
            Some('H') => out.push_str(&format!("{hour:02}")),
            Some('M') => out.push_str(&format!("{minute:02}")),
            Some('S') => out.push_str(&format!("{second:02}")),
            Some('j') => out.push_str(&format!("{day_of_year:03}")),
            Some('b') => out.push_str(&month_name[..3]),
            Some('B') => out.push_str(month_name),
            Some('a') => out.push_str(&weekday[..3]),
            Some('A') => out.push_str(weekday),
            Some('p') => out.push_str(if hour < 12 { "AM" } else { "PM" }),
            Some('%') => out.push('%'),
            // Not one we render: show it, rather than swallow it.
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Tick steps a person would pick, in seconds: seconds, minutes, hours, days,
/// then whole years (a month is not a fixed number of seconds, so the ladder
/// stops at weeks and years, which is what ROOT's own steps do).
const STEPS: [i64; 21] = [
    1, 2, 5, 10, 15, 30, // seconds
    60, 120, 300, 600, 900, 1800, // minutes
    3600, 7200, 10_800, 21_600, 43_200, // hours
    86_400, 172_800, 604_800, // days and a week
    31_536_000,
];

/// Tick positions for a time axis over `[lo, hi]` seconds, aiming for about
/// `target` ticks: the smallest step from the ladder that does not overshoot,
/// with the ticks landing on multiples of it.
#[must_use]
pub fn time_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    let span = hi - lo;
    if !span.is_finite() || span <= 0.0 || target == 0 {
        return Vec::new();
    }
    let want = (span / target as f64).max(1.0);
    let step = STEPS
        .iter()
        .copied()
        .find(|&s| s as f64 >= want)
        // Past a year, fall back to whole years.
        .unwrap_or_else(|| 31_536_000 * ((want / 31_536_000.0).ceil() as i64).max(1));
    let step = step as f64;
    let first = (lo / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut tick = first;
    while tick <= hi + step * 1e-9 {
        ticks.push(tick);
        tick += step;
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_unix_epoch_and_a_known_date() {
        assert_eq!(format_time(0, "%Y-%m-%d %H:%M:%S"), "1970-01-01 00:00:00");
        // 2024-02-29T12:34:56Z — a leap day, to exercise the calendar.
        assert_eq!(
            format_time(1_709_210_096, "%Y-%m-%d %H:%M:%S"),
            "2024-02-29 12:34:56"
        );
        assert_eq!(format_time(1_709_210_096, "%d/%m/%y"), "29/02/24");
        assert_eq!(format_time(1_709_210_096, "%a %b %j %p"), "Thu Feb 060 PM");
        // Unknown specifiers and literals come through as they are.
        assert_eq!(format_time(0, "100%% at %Q"), "100% at %Q");
    }

    #[test]
    fn civil_dates_round_trip() {
        for &(y, m, d) in &[
            (1970, 1, 1),
            (1969, 12, 31),
            (2000, 2, 29),
            (2024, 12, 31),
            (1901, 7, 4),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d), "{y}-{m}-{d}");
        }
    }

    #[test]
    fn reads_roots_format_and_epoch() {
        let fmt = TimeFormat::parse("%H:%M%F2024-01-01 00:00:00");
        assert_eq!(fmt.format, "%H:%M");
        assert_eq!(fmt.offset, 1_704_067_200);
        // An axis value is seconds from that epoch.
        assert_eq!(fmt.label(3661.0), "01:01");

        // No %F: the values count from the Unix epoch, and an empty format
        // shows the whole date and time.
        let plain = TimeFormat::parse("");
        assert_eq!(plain.offset, 0);
        assert_eq!(plain.label(0.0), "1970-01-01 00:00:00");
    }

    #[test]
    fn ticks_land_on_steps_a_person_would_pick() {
        // Six hours, about six ticks: every hour.
        let ticks = time_ticks(0.0, 21_600.0, 6);
        assert_eq!(ticks.first(), Some(&0.0));
        assert_eq!(ticks[1] - ticks[0], 3600.0);

        // A day, about four ticks: every six hours.
        let ticks = time_ticks(0.0, 86_400.0, 4);
        assert_eq!(ticks[1] - ticks[0], 21_600.0);

        // Ninety seconds, about three ticks: every thirty seconds, starting at
        // the first multiple inside the range.
        let ticks = time_ticks(10.0, 100.0, 3);
        assert_eq!(ticks, vec![30.0, 60.0, 90.0]);

        // A degenerate range asks for nothing.
        assert!(time_ticks(5.0, 5.0, 4).is_empty());
    }
}
