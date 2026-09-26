//! Leading-timestamp detection for log lines.
//!
//! Every format here is fixed, so parsing is a handful of byte comparisons over the first
//! [`SCAN_BYTES`] of a line — no allocation, no regex, no chrono. A stream remembers which
//! format matched last (`FormatHint`) and tries it first, which is the common case: a log
//! file does not change its timestamp format halfway through.
//!
//! The result is milliseconds on the clock the log printed, counted from 1970-01-01 00:00
//! of that clock. A zone suffix (`Z`, `+02:00`, `+0200`) is read past but not applied:
//! FastTail compares timestamps with each other and with what the user typed, never with
//! wall-clock time, so `14:02` must mean the `14:02` written in the line - converting an
//! Apache `+0200` log to UTC would put every typed time two hours off. Bare epoch values
//! have no printed clock and are read as UTC.

/// Only the head of a line can carry the timestamp; a match further in is a coincidence.
pub const SCAN_BYTES: usize = 64;

/// Which format matched last on a stream, so it can be tried first next time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatHint {
    #[default]
    Unknown,
    Iso8601,
    Syslog,
    Apache,
    Epoch,
}

/// Milliseconds since the Unix epoch for the timestamp at the start of `line`, if any.
pub fn detect_timestamp(line: &str, hint: FormatHint) -> Option<(i64, FormatHint)> {
    let head = &line.as_bytes()[..line.len().min(SCAN_BYTES)];
    // The hinted format first: on a log that does not change format this is the only
    // parser that ever runs.
    let order = match hint {
        FormatHint::Iso8601 => [
            FormatHint::Iso8601,
            FormatHint::Apache,
            FormatHint::Syslog,
            FormatHint::Epoch,
        ],
        FormatHint::Syslog => [
            FormatHint::Syslog,
            FormatHint::Iso8601,
            FormatHint::Apache,
            FormatHint::Epoch,
        ],
        FormatHint::Apache => [
            FormatHint::Apache,
            FormatHint::Iso8601,
            FormatHint::Syslog,
            FormatHint::Epoch,
        ],
        FormatHint::Epoch => [
            FormatHint::Epoch,
            FormatHint::Iso8601,
            FormatHint::Apache,
            FormatHint::Syslog,
        ],
        FormatHint::Unknown => [
            FormatHint::Iso8601,
            FormatHint::Apache,
            FormatHint::Syslog,
            FormatHint::Epoch,
        ],
    };
    for format in order {
        let parsed = match format {
            FormatHint::Iso8601 => parse_iso8601(head),
            FormatHint::Syslog => parse_syslog(head),
            FormatHint::Apache => parse_apache(head),
            FormatHint::Epoch => parse_epoch(head),
            FormatHint::Unknown => None,
        };
        if let Some(millis) = parsed {
            return Some((millis, format));
        }
    }
    None
}

/// `2026-09-18T14:02:05.123Z`, `2026-09-18 14:02:05,123`, `2026-09-18 14:02:05+02:00`.
/// The date and the time are required; the fraction and the zone are optional, and the
/// zone is skipped rather than applied (see the module comment).
fn parse_iso8601(head: &[u8]) -> Option<i64> {
    let start = skip_leading_bracket(head);
    let b = head.get(start..)?;
    if b.len() < 19 {
        return None;
    }
    let year = number(b, 0, 4)?;
    if b[4] != b'-' {
        return None;
    }
    let month = number(b, 5, 2)?;
    if b[7] != b'-' {
        return None;
    }
    let day = number(b, 8, 2)?;
    if b[10] != b'T' && b[10] != b' ' && b[10] != b'_' {
        return None;
    }
    let (time_millis, after_time) = parse_clock(b, 11)?;
    let (frac, _) = parse_fraction(b, after_time);
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400_000 + time_millis + frac)
}

/// Syslog: `Sep 18 14:02:05` — no year, so the current one is assumed, which is what every
/// other syslog reader does.
fn parse_syslog(head: &[u8]) -> Option<i64> {
    let start = skip_leading_bracket(head);
    let b = head.get(start..)?;
    if b.len() < 15 {
        return None;
    }
    let month = month_from_name(&b[0..3])?;
    if b[3] != b' ' {
        return None;
    }
    // The day is space-padded for single digits: "Sep  8 14:02:05".
    let (day, time_at) = if b[4] == b' ' {
        (number(b, 5, 1)?, 7)
    } else {
        (number(b, 4, 2)?, 7)
    };
    if b[time_at - 1] != b' ' {
        return None;
    }
    let (time_millis, after_time) = parse_clock(b, time_at)?;
    let (frac, _) = parse_fraction(b, after_time);
    let days = days_from_civil(current_year(), month, day)?;
    Some(days * 86_400_000 + time_millis + frac)
}

/// Apache and nginx: `[18/Sep/2026:14:02:05 +0200]`, the zone skipped as in ISO 8601.
fn parse_apache(head: &[u8]) -> Option<i64> {
    let start = head.iter().position(|b| *b == b'[').map(|i| i + 1)?;
    // The bracket has to be at the very start of the line, or right after the client and
    // user fields of a combined-log line, which is still inside the scan window.
    let b = head.get(start..)?;
    if b.len() < 20 {
        return None;
    }
    let day = number(b, 0, 2)?;
    if b[2] != b'/' {
        return None;
    }
    let month = month_from_name(&b[3..6])?;
    if b[6] != b'/' {
        return None;
    }
    let year = number(b, 7, 4)?;
    if b[11] != b':' {
        return None;
    }
    let (time_millis, _) = parse_clock(b, 12)?;
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400_000 + time_millis)
}

/// Bare epoch seconds (10 digits) or milliseconds (13 digits), the way container runtimes
/// and some JSON loggers write them. Shorter runs of digits are line numbers, not times.
fn parse_epoch(head: &[u8]) -> Option<i64> {
    let digits = head.iter().take_while(|b| b.is_ascii_digit()).count();
    // A longer digit run is an id, not a time; a following digit would make it one.
    let next = head.get(digits);
    if !matches!(
        next,
        None | Some(b' ') | Some(b'\t') | Some(b',') | Some(b'.')
    ) {
        return None;
    }
    match digits {
        10 => Some(number_i64(head, 0, 10)? * 1000),
        13 => number_i64(head, 0, 13),
        _ => None,
    }
}

/// `HH:MM:SS` at `at`, returning the milliseconds into the day and the index after it.
fn parse_clock(b: &[u8], at: usize) -> Option<(i64, usize)> {
    if b.len() < at + 8 {
        return None;
    }
    let hour = number(b, at, 2)?;
    if b[at + 2] != b':' {
        return None;
    }
    let minute = number(b, at + 3, 2)?;
    if b[at + 5] != b':' {
        return None;
    }
    let second = number(b, at + 6, 2)?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let millis = (hour as i64 * 3600 + minute as i64 * 60 + second as i64) * 1000;
    Some((millis, at + 8))
}

/// `.123`, `,123456` — up to nanoseconds, kept to milliseconds.
fn parse_fraction(b: &[u8], at: usize) -> (i64, usize) {
    if !matches!(b.get(at), Some(b'.') | Some(b',')) {
        return (0, at);
    }
    let mut millis = 0i64;
    let mut seen = 0;
    let mut idx = at + 1;
    while let Some(d) = b.get(idx) {
        if !d.is_ascii_digit() {
            break;
        }
        if seen < 3 {
            millis = millis * 10 + (d - b'0') as i64;
            seen += 1;
        }
        idx += 1;
    }
    if seen == 0 {
        return (0, at);
    }
    while seen < 3 {
        millis *= 10;
        seen += 1;
    }
    (millis, idx)
}

fn skip_leading_bracket(head: &[u8]) -> usize {
    match head.first() {
        Some(b'[') | Some(b'(') => 1,
        _ => 0,
    }
}

fn number(b: &[u8], at: usize, len: usize) -> Option<u32> {
    let slice = b.get(at..at + len)?;
    let mut value = 0u32;
    for d in slice {
        if !d.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (d - b'0') as u32;
    }
    Some(value)
}

fn number_i64(b: &[u8], at: usize, len: usize) -> Option<i64> {
    let slice = b.get(at..at + len)?;
    let mut value = 0i64;
    for d in slice {
        if !d.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (d - b'0') as i64;
    }
    Some(value)
}

fn month_from_name(name: &[u8]) -> Option<u32> {
    const MONTHS: [&[u8; 3]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name))
        .map(|i| i as u32 + 1)
}

/// Days since the Unix epoch, from Howard Hinnant's `days_from_civil`. Returns `None` for
/// impossible dates, which is how a line of digits that looks like a date is rejected.
fn days_from_civil(year: u32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let y = year as i64 - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Year used for formats that omit it (syslog). Read once per process: a log open across
/// New Year's Eve is not worth a system call per line.
fn current_year() -> u32 {
    use std::sync::OnceLock;
    static YEAR: OnceLock<u32> = OnceLock::new();
    *YEAR.get_or_init(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        year_from_days(secs / 86_400)
    })
}

/// Civil year of a day count since the epoch (the inverse of `days_from_civil`, year only).
fn year_from_days(days: i64) -> u32 {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2)) as u32
}

/// `YYYY-MM-DD HH:MM:SS` for a timestamp in milliseconds, for the status bar and the
/// tooltips: the clock the log printed, since the parser never applies a zone.
pub fn format_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let rest = millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let (h, m, sec) = (rest / 3_600_000, (rest / 60_000) % 60, (rest / 1000) % 60);
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{sec:02}")
}

/// Time of day only (`HH:MM:SS`), for places where the date is already clear.
pub fn format_clock(millis: i64) -> String {
    let rest = millis.rem_euclid(86_400_000);
    format!(
        "{:02}:{:02}:{:02}",
        rest / 3_600_000,
        (rest / 60_000) % 60,
        (rest / 1000) % 60
    )
}

/// Civil date of a day count since the epoch (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m as u32, d as u32)
}

/// Reads what the user typed into the time-range or go-to-time fields.
///
/// Accepts `HH:MM`, `HH:MM:SS` (that time on the day of `reference`), `YYYY-MM-DD HH:MM`
/// (`T` or a space in between), a bare `YYYY-MM-DD` (the start of that day) and anything
/// the line parser reads, so a timestamp copied straight out of the log works.
/// `reference` is the day the bare times belong to — the first timestamp of the stream,
/// which is what the user means by "14:02" while looking at yesterday's log.
pub fn parse_user_time(input: &str, reference: i64) -> Option<i64> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    // A full timestamp, in any of the formats a log line can carry.
    if let Some((millis, _)) = detect_timestamp(text, FormatHint::Unknown) {
        return Some(millis);
    }
    // A date and a time without the seconds, which the line parser insists on.
    if is_date_minute(text) {
        return detect_timestamp(&format!("{text}:00"), FormatHint::Iso8601).map(|(ms, _)| ms);
    }
    // A date alone: midnight, the start of that day.
    if is_date(text) {
        return detect_timestamp(&format!("{text} 00:00:00"), FormatHint::Iso8601)
            .map(|(ms, _)| ms);
    }
    // `HH:MM` or `HH:MM:SS` on the reference day.
    let b = text.as_bytes();
    let (hour, minute, second) = match b.len() {
        5 => (number(b, 0, 2)?, number(b, 3, 2)?, 0),
        8 => (number(b, 0, 2)?, number(b, 3, 2)?, number(b, 6, 2)?),
        _ => return None,
    };
    if b[2] != b':' || (b.len() == 8 && b[5] != b':') {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let day_start = reference.div_euclid(86_400_000) * 86_400_000;
    Some(day_start + (hour as i64 * 3600 + minute as i64 * 60 + second as i64) * 1000)
}

/// The end of the day, minute or second the user named, so "from 14:02 to 14:05" includes
/// everything stamped 14:05:59.999 and "to 2026-09-18" everything up to 23:59:59.999 —
/// the window a person means when they type two times.
pub fn end_of_typed_time(input: &str, millis: i64) -> i64 {
    let text = input.trim();
    match text.len() {
        10 if is_date(text) => millis + 86_399_999, // YYYY-MM-DD -> to the end of that day
        5 => millis + 59_999,                       // HH:MM  -> to the end of that minute
        8 => millis + 999,                          // HH:MM:SS -> to the end of that second
        16 if is_date_minute(text) => millis + 59_999, // YYYY-MM-DD HH:MM
        19 if is_date_minute(&text[..16]) => millis + 999, // YYYY-MM-DD HH:MM:SS
        _ => millis,
    }
}

/// `YYYY-MM-DD`, nothing after the day.
fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && [0, 1, 2, 3, 5, 6, 8, 9]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
        && b[4] == b'-'
        && b[7] == b'-'
}

/// `YYYY-MM-DD HH:MM` or `YYYY-MM-DDTHH:MM`, nothing after the minutes.
fn is_date_minute(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 16
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
        && b[4] == b'-'
        && b[7] == b'-'
        && matches!(b[10], b' ' | b'T')
        && b[13] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(line: &str) -> Option<i64> {
        detect_timestamp(line, FormatHint::Unknown).map(|(millis, _)| millis)
    }

    #[test]
    fn iso8601_in_its_many_shapes() {
        // 2026-09-18T14:02:05.123Z
        let base = 1_789_740_125_123;
        assert_eq!(detect("2026-09-18T14:02:05.123Z ERROR boom"), Some(base));
        assert_eq!(detect("2026-09-18 14:02:05,123 ERROR boom"), Some(base));
        assert_eq!(detect("2026-09-18T14:02:05.123456Z trailing"), Some(base));
        assert_eq!(detect("2026-09-18T14:02:05Z no fraction"), Some(base - 123));
        assert_eq!(detect("[2026-09-18 14:02:05.123] bracketed"), Some(base));
        // The zone is read past, not applied: 14:02 stays 14:02, as printed.
        assert_eq!(
            detect("2026-09-18T14:02:05.123+02:00 zoned"),
            Some(base),
            "the printed clock must be kept"
        );
        assert_eq!(detect("2026-09-18T14:02:05.123-0300 zoned"), Some(base));
    }

    #[test]
    fn syslog_apache_and_epoch() {
        let syslog = detect("Sep 18 14:02:05 host app[1]: started").expect("syslog");
        let padded = detect("Sep  8 14:02:05 host app[1]: started").expect("space-padded day");
        assert_eq!(syslog - padded, 10 * 86_400_000, "ten days apart");

        assert_eq!(
            detect("127.0.0.1 - - [18/Sep/2026:14:02:05 +0000] \"GET / HTTP/1.1\" 200"),
            Some(1_789_740_125_000)
        );
        assert_eq!(
            detect("[18/Sep/2026:14:02:05 +0200] zoned"),
            Some(1_789_740_125_000),
            "the printed clock must be kept"
        );

        assert_eq!(detect("1789480925 epoch seconds"), Some(1_789_480_925_000));
        assert_eq!(
            detect("1789480925123 epoch millis"),
            Some(1_789_480_925_123)
        );
    }

    #[test]
    fn lines_without_a_timestamp_are_rejected() {
        assert_eq!(detect("    at Foo.bar(Foo.java:10)"), None);
        assert_eq!(detect("ERROR something went wrong"), None);
        assert_eq!(detect(""), None);
        // Impossible dates are not timestamps.
        assert_eq!(detect("2026-13-01T00:00:00Z"), None);
        assert_eq!(detect("2026-02-30T00:00:00Z"), None);
        assert_eq!(detect("2026-09-18T25:00:00Z"), None);
        // Digit runs that are ids or line numbers, not epochs.
        assert_eq!(detect("12345 short"), None);
        assert_eq!(detect("178948092512345 too long"), None);
        assert_eq!(detect("1789480925x glued"), None);
        // A timestamp further in than the scan window does not count.
        let far = format!("{}2026-09-18T14:02:05Z", " ".repeat(SCAN_BYTES));
        assert_eq!(detect(&far), None);
    }

    #[test]
    fn leap_years_and_the_epoch_itself() {
        assert_eq!(detect("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(detect("2024-02-29T00:00:00Z"), Some(1_709_164_800_000));
        assert_eq!(
            detect("2023-02-29T00:00:00Z"),
            None,
            "2023 is not a leap year"
        );
        assert_eq!(detect("2000-02-29T00:00:00Z"), Some(951_782_400_000));
        assert_eq!(
            detect("1900-02-29T00:00:00Z"),
            None,
            "1900 is not a leap year"
        );
    }

    #[test]
    fn user_input_in_the_shapes_a_person_types() {
        let day = detect("2026-09-18T00:00:00Z").unwrap();
        let noon = detect("2026-09-18T12:00:00Z").unwrap();

        // Bare times land on the day of the reference timestamp, wherever in it it falls.
        assert_eq!(
            parse_user_time("14:02", noon),
            Some(day + 14 * 3_600_000 + 2 * 60_000)
        );
        assert_eq!(
            parse_user_time("14:02:05", noon),
            Some(day + 14 * 3_600_000 + 2 * 60_000 + 5_000)
        );
        // A timestamp copied out of the log works as it is, reference or not.
        assert_eq!(
            parse_user_time("2026-09-18T14:02:05.123Z", 0),
            Some(1_789_740_125_123)
        );
        assert_eq!(
            parse_user_time("2026-09-18 14:02:05", 0),
            Some(1_789_740_125_000)
        );
        // Without the seconds, with a space or a `T`.
        assert_eq!(
            parse_user_time("2026-09-18 14:02", 0),
            Some(1_789_740_120_000)
        );
        assert_eq!(
            parse_user_time("2026-09-18T14:02", 0),
            Some(1_789_740_120_000)
        );
        // A date alone is the start of that day.
        assert_eq!(parse_user_time("2026-09-18", noon), Some(day));
        assert_eq!(parse_user_time("2026-09-18", 0), Some(day));
        assert_eq!(parse_user_time("2026-02-30", 0), None);
        assert_eq!(parse_user_time("2026-13-01", 0), None);
        // A typed time is the clock of the log: an Apache +0200 line at 14:02 matches 14:02.
        let apache = detect("[18/Sep/2026:14:02:05 +0200] GET /").unwrap();
        assert_eq!(parse_user_time("14:02:05", apache), Some(apache));
        // Rubbish stays rubbish.
        assert_eq!(parse_user_time("", noon), None);
        assert_eq!(parse_user_time("25:00", noon), None);
        assert_eq!(parse_user_time("14:60", noon), None);
        assert_eq!(parse_user_time("later", noon), None);
        assert_eq!(parse_user_time("14.02", noon), None);
    }

    #[test]
    fn formatting_is_the_inverse_of_parsing() {
        for iso in [
            "2026-09-18T14:02:05Z",
            "1970-01-01T00:00:00Z",
            "2024-02-29T23:59:59Z",
            "2000-01-01T00:00:00Z",
        ] {
            let millis = detect(iso).expect(iso);
            let shown = format_millis(millis);
            assert_eq!(
                detect(&format!("{shown}Z").replace(' ', "T")),
                Some(millis),
                "{iso} formatted as {shown} must read back the same"
            );
        }
        assert_eq!(format_millis(1_789_740_125_123), "2026-09-18 14:02:05");
        assert_eq!(format_clock(1_789_740_125_123), "14:02:05");
    }

    #[test]
    fn a_typed_time_covers_the_unit_it_names() {
        // "to 14:05" means through 14:05:59.999, not 14:05:00.000.
        assert_eq!(end_of_typed_time("14:05", 1_000), 60_999);
        assert_eq!(end_of_typed_time("14:05:30", 1_000), 1_999);
        assert_eq!(end_of_typed_time("2026-09-18T14:05:30Z", 1_000), 1_000);
        assert_eq!(end_of_typed_time("2026-09-18 14:05", 1_000), 60_999);
        assert_eq!(end_of_typed_time("2026-09-18 14:05:30", 1_000), 1_999);
        // "to 2026-09-18" means through 23:59:59.999 of that day.
        assert_eq!(end_of_typed_time("2026-09-18", 1_000), 86_400_999);
    }

    #[test]
    fn the_hint_is_returned_and_honoured() {
        let (_, hint) = detect_timestamp("2026-09-18T14:02:05Z x", FormatHint::Unknown).unwrap();
        assert_eq!(hint, FormatHint::Iso8601);
        // A hint for another format still parses the line, and corrects itself.
        let (millis, corrected) = detect_timestamp("2026-09-18T14:02:05Z x", FormatHint::Syslog)
            .expect("a wrong hint must not lose the timestamp");
        assert_eq!(corrected, FormatHint::Iso8601);
        assert_eq!(millis, 1_789_740_125_000);

        let (_, epoch) = detect_timestamp("1789480925 x", FormatHint::Epoch).unwrap();
        assert_eq!(epoch, FormatHint::Epoch);
    }
}
