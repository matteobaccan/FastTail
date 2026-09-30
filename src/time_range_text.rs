// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The time range as text, shared by the GUI popup (`ui::time_range`) and the terminal
//! dialog: reading a side (absolute, relative or empty), the shortcuts, the calendar day
//! and clock edits of a side, and the label of the span the visible lines cover.

use crate::timestamp::{days_to_date, format_clock, format_millis, is_bare_date, parse_user_time};

/// Longest control label, in characters, before the span loses its seconds.
const MAX_LABEL_CHARS: usize = 40;

const DAY_MS: i64 = 86_400_000;

const HOUR_MS: i64 = 3_600_000;

/// One side of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    From,
    To,
}

/// The instant a side names, `None` when it is empty or cannot be read. A relative side
/// (`-15m`, `now`) is read against the local clock, which only matters for the calendar
/// it opens on; the engine reads it on the stream's own clock.
pub fn parse(text: &str, reference: i64) -> Option<i64> {
    crate::timestamp::parse_relative(text, crate::timestamp::local_now_millis())
        .or_else(|| parse_user_time(text, reference))
}

/// The relative shortcuts: label and the "from" side they fill ("to" stays empty).
pub const RELATIVE_SHORTCUTS: &[(&str, &str)] = &[
    ("5m", "-5m"),
    ("15m", "-15m"),
    ("1h", "-1h"),
    ("6h", "-6h"),
    ("24h", "-24h"),
    ("7d", "-7d"),
];

/// Whether a side can be applied: empty (an open end) or readable.
pub fn side_readable(text: &str, reference: i64) -> bool {
    text.trim().is_empty() || parse(text, reference).is_some()
}

pub fn day_of(millis: i64) -> i64 {
    millis.div_euclid(DAY_MS)
}

/// `YYYY-MM-DD` of a day count since the epoch.
pub fn format_date(days: i64) -> String {
    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A day picked on a side's calendar: the side keeps the time it has, otherwise it
/// becomes the bare date (the whole day on the "to" side, its start on the "from" side).
pub fn pick_day(text: &str, reference: i64, days: i64) -> String {
    let date = format_date(days);
    match parse(text, reference) {
        Some(millis) if !is_bare_date(text) => format!("{date} {}", format_clock(millis)),
        _ => date,
    }
}

/// Hour, minute and second the spinners of a side show: the side's time; the start of
/// the day on an empty "from" side, and its last second on a "to" side that is empty or
/// a bare date, since that is what the side covers.
pub fn clock_of(text: &str, reference: i64, side: Side) -> (u32, u32, u32) {
    let whole_day = side == Side::To && (text.trim().is_empty() || is_bare_date(text));
    match parse(text, reference) {
        Some(millis) if !whole_day => {
            let rest = millis.rem_euclid(DAY_MS) / 1000;
            (
                (rest / 3600) as u32,
                (rest / 60 % 60) as u32,
                (rest % 60) as u32,
            )
        }
        _ if side == Side::To => (23, 59, 59),
        _ => (0, 0, 0),
    }
}

/// A side after a spinner changed: `YYYY-MM-DD HH:MM:SS` on the side's day, or on
/// `fallback_days` when the side names none.
pub fn with_clock(
    text: &str,
    reference: i64,
    fallback_days: i64,
    (hour, minute, second): (u32, u32, u32),
) -> String {
    let days = parse(text, reference).map(day_of).unwrap_or(fallback_days);
    format!("{} {hour:02}:{minute:02}:{second:02}", format_date(days))
}

/// "First day" / "Last day": both sides on the date of `millis`.
pub fn whole_day(millis: i64) -> (String, String) {
    let date = format_date(day_of(millis));
    (date.clone(), date)
}

/// "Last hour": the hour ending at the log's last timestamp.
pub fn last_hour(last: i64) -> (String, String) {
    (format_millis(last - HOUR_MS), format_millis(last))
}

/// The span of the visible lines as the control writes it: the date once when both ends
/// share it, and without the seconds when the label would exceed 40 characters.
pub fn span_text(from: i64, to: i64) -> String {
    if from == to {
        return format_millis(from);
    }
    let (a, b) = (format_millis(from), format_millis(to));
    if a[..10] == b[..10] {
        return format!("{a} → {}", &b[11..]);
    }
    let full = format!("{a} → {b}");
    // "🕘 " comes before it.
    if full.chars().count() + 2 > MAX_LABEL_CHARS {
        format!("{} → {}", &a[..16], &b[..16])
    } else {
        full
    }
}

/// What the control knows about the stream.
#[derive(Clone, Copy, Debug, Default)]
pub struct ControlState {
    /// Earliest and latest timestamp of the visible lines.
    pub span: Option<(i64, i64)>,
    /// Every line has been timed.
    pub timed: bool,
    /// Most lines can be placed in time.
    pub usable: bool,
    /// A window narrows the view.
    pub filtered: bool,
    /// A window waits for the background timing.
    pub pending: bool,
    /// Progress of a running timing scan, 0..=1.
    pub progress: Option<f32>,
    /// The window has a relative side and slides with the clock.
    pub live: bool,
}

impl ControlState {
    /// A timed stream without usable timestamps (and no window to show).
    pub fn no_timestamps(&self) -> bool {
        self.timed && !self.usable && !self.filtered
    }
}

/// Label of the control: `🕘` and the span, `…` (with the timing progress) while the
/// stream has not been timed far enough to say, "no timestamps" on a stream without
/// usable ones, `—` when no visible line carries a time; `⏳` while a window is held.
pub fn control_label(state: &ControlState, no_timestamps: &str) -> String {
    let mut label = if state.no_timestamps() {
        format!("🕘 {no_timestamps}")
    } else if let Some((from, to)) = state.span {
        format!("🕘 {}", span_text(from, to))
    } else if !state.timed {
        match state.progress {
            Some(p) => format!("🕘 … {:.0}%", (p * 100.0).clamp(0.0, 100.0)),
            None => "🕘 …".to_string(),
        }
    } else {
        "🕘 —".to_string()
    };
    if state.live {
        label = label.replacen("🕘 ", "🕘 ⟳ ", 1);
    }
    if state.pending {
        label.push_str(" ⏳");
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::date_to_days;

    fn ms(text: &str) -> i64 {
        parse_user_time(text, 0).unwrap()
    }

    fn days(y: i64, m: u32, d: u32) -> i64 {
        date_to_days(y, m, d).unwrap()
    }

    #[test]
    fn picking_a_day_keeps_the_time_or_gives_the_bare_date() {
        let reference = ms("2026-09-18 00:00:00");
        // Empty sides become the bare date: the start of the day on "from", the whole
        // day on "to" (both read by `end_of_typed_time`).
        assert_eq!(pick_day("", reference, days(2026, 9, 18)), "2026-09-18");
        assert_eq!(
            pick_day("2026-09-18", reference, days(2026, 9, 19)),
            "2026-09-19"
        );
        // A side with a time keeps it.
        assert_eq!(
            pick_day("2026-09-18 14:02:00", reference, days(2026, 9, 19)),
            "2026-09-19 14:02:00"
        );
        assert_eq!(
            pick_day("14:02", reference, days(2026, 9, 20)),
            "2026-09-20 14:02:00"
        );
        // Unreadable text is replaced.
        assert_eq!(
            pick_day("14:6x", reference, days(2026, 9, 18)),
            "2026-09-18"
        );
    }

    #[test]
    fn spinners_show_what_the_side_covers_and_write_a_full_time() {
        let reference = ms("2026-09-18 00:00:00");
        assert_eq!(clock_of("", reference, Side::From), (0, 0, 0));
        assert_eq!(clock_of("", reference, Side::To), (23, 59, 59));
        assert_eq!(clock_of("2026-09-18", reference, Side::From), (0, 0, 0));
        assert_eq!(clock_of("2026-09-18", reference, Side::To), (23, 59, 59));
        assert_eq!(
            clock_of("2026-09-18 14:02:05", reference, Side::To),
            (14, 2, 5)
        );
        assert_eq!(clock_of("14:02", reference, Side::From), (14, 2, 0));
        assert_eq!(clock_of("rubbish", reference, Side::From), (0, 0, 0));

        let fallback = days(2026, 9, 1);
        assert_eq!(
            with_clock("2026-09-18", reference, fallback, (14, 2, 0)),
            "2026-09-18 14:02:00"
        );
        assert_eq!(
            with_clock("2026-09-18 23:59:59", reference, fallback, (23, 30, 59)),
            "2026-09-18 23:30:59"
        );
        assert_eq!(
            with_clock("", reference, fallback, (8, 0, 0)),
            "2026-09-01 08:00:00"
        );
        // A bare time belongs to the day of the log.
        assert_eq!(
            with_clock("14:02", reference, fallback, (15, 2, 0)),
            "2026-09-18 15:02:00"
        );
    }

    #[test]
    fn shortcuts_fill_both_sides() {
        let last = ms("2026-05-29 23:38:12");
        assert_eq!(
            whole_day(last),
            ("2026-05-29".to_string(), "2026-05-29".to_string())
        );
        assert_eq!(
            last_hour(last),
            (
                "2026-05-29 22:38:12".to_string(),
                "2026-05-29 23:38:12".to_string()
            )
        );
        // Across midnight.
        assert_eq!(
            last_hour(ms("2026-05-30 00:10:00")).0,
            "2026-05-29 23:10:00"
        );
    }

    #[test]
    fn the_label_writes_the_date_once_and_stays_short() {
        let a = ms("2026-09-18 14:02:05");
        let b = ms("2026-09-18 16:30:12");
        assert_eq!(span_text(a, b), "2026-09-18 14:02:05 → 16:30:12");
        assert_eq!(span_text(a, a), "2026-09-18 14:02:05");
        let c = ms("2026-09-19 16:30:12");
        let long = span_text(a, c);
        assert_eq!(long, "2026-09-18 14:02 → 2026-09-19 16:30");
        assert!(long.chars().count() + 2 <= MAX_LABEL_CHARS);

        let state = ControlState {
            span: Some((a, b)),
            timed: true,
            usable: true,
            ..Default::default()
        };
        assert_eq!(
            control_label(&state, "no timestamps"),
            "🕘 2026-09-18 14:02:05 → 16:30:12"
        );
        let pending = ControlState {
            pending: true,
            ..state
        };
        assert!(control_label(&pending, "no timestamps").ends_with(" ⏳"));
        let none = ControlState {
            span: None,
            timed: true,
            usable: false,
            ..Default::default()
        };
        assert_eq!(control_label(&none, "no timestamps"), "🕘 no timestamps");
        let timing = ControlState {
            progress: Some(0.42),
            ..Default::default()
        };
        assert_eq!(control_label(&timing, "no timestamps"), "🕘 … 42%");
        assert_eq!(control_label(&ControlState::default(), "x"), "🕘 …");
        let hidden = ControlState {
            timed: true,
            usable: true,
            filtered: true,
            ..Default::default()
        };
        assert_eq!(control_label(&hidden, "x"), "🕘 —");
    }

    #[test]
    fn only_readable_or_empty_sides_can_be_applied() {
        let reference = ms("2026-09-18 00:00:00");
        assert!(side_readable("", reference));
        assert!(side_readable("  ", reference));
        assert!(side_readable("14:02", reference));
        assert!(side_readable("2026-09-18", reference));
        assert!(!side_readable("14:6x", reference));
        assert!(!side_readable("14:0", reference));
    }
}
