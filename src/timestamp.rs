// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

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
    for format in format_order(hint) {
        if let Some((millis, _)) = parse_format(format, head) {
            return Some((millis, format));
        }
    }
    None
}

/// Byte length of the timestamp at the start of `line`, zone and closing bracket
/// included, with the same parsers and hint as `detect_timestamp`. The collapse of
/// repeated lines drops it, so lines that differ only by their time compare equal. An
/// Apache timestamp counts only when its bracket opens the line (in a combined-log line
/// it follows the client fields, which are not a prefix to drop), and a bare clock
/// (`14:02:05.123`), which is no point in time for `detect_timestamp`, is a prefix too.
pub fn leading_span(line: &str, hint: FormatHint) -> Option<usize> {
    let head = &line.as_bytes()[..line.len().min(SCAN_BYTES)];
    for format in format_order(hint) {
        if format == FormatHint::Apache && head.first() != Some(&b'[') {
            continue;
        }
        if let Some((_, end)) = parse_format(format, head) {
            return Some(end);
        }
    }
    let start = skip_leading_bracket(head);
    let b = head.get(start..)?;
    let (_, after_time) = parse_clock(b, 0)?;
    let (_, after_frac) = parse_fraction(b, after_time);
    Some(close_bracket(head, start, start + after_frac))
}

/// The four formats, the hinted one first: on a log that does not change format this is
/// the only parser that ever runs.
fn format_order(hint: FormatHint) -> [FormatHint; 4] {
    match hint {
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
    }
}

/// Milliseconds and end offset in `head` of the timestamp in `format` at its start.
fn parse_format(format: FormatHint, head: &[u8]) -> Option<(i64, usize)> {
    match format {
        FormatHint::Iso8601 => parse_iso8601(head),
        FormatHint::Syslog => parse_syslog(head),
        FormatHint::Apache => parse_apache(head),
        FormatHint::Epoch => parse_epoch(head),
        FormatHint::Unknown => None,
    }
}

/// Past the bracket closing a timestamp that opened with one (`start` > 0).
fn close_bracket(head: &[u8], start: usize, end: usize) -> usize {
    if start > 0 && matches!(head.get(end), Some(b']') | Some(b')')) {
        end + 1
    } else {
        end
    }
}

/// Past a zone suffix at `at`: `Z`, `+02:00`, `+0200`, `-03`.
fn skip_zone(b: &[u8], at: usize) -> usize {
    match b.get(at) {
        Some(b'Z') | Some(b'z') => at + 1,
        Some(b'+') | Some(b'-') if number(b, at + 1, 2).is_some() => {
            let end = at + 3;
            if b.get(end) == Some(&b':') && number(b, end + 1, 2).is_some() {
                end + 3
            } else if number(b, end, 2).is_some() {
                end + 2
            } else {
                end
            }
        }
        _ => at,
    }
}

/// `2026-09-18T14:02:05.123Z`, `2026-09-18 14:02:05,123`, `2026-09-18 14:02:05+02:00`.
/// The date and the time are required; the fraction and the zone are optional, and the
/// zone is skipped rather than applied (see the module comment).
fn parse_iso8601(head: &[u8]) -> Option<(i64, usize)> {
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
    let (frac, after_frac) = parse_fraction(b, after_time);
    let days = days_from_civil(year, month, day)?;
    let end = close_bracket(head, start, start + skip_zone(b, after_frac));
    Some((days * 86_400_000 + time_millis + frac, end))
}

/// Syslog: `Sep 18 14:02:05` — no year, so the current one is assumed, which is what every
/// other syslog reader does.
fn parse_syslog(head: &[u8]) -> Option<(i64, usize)> {
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
    let (frac, after_frac) = parse_fraction(b, after_time);
    let days = days_from_civil(current_year(), month, day)?;
    let end = close_bracket(head, start, start + after_frac);
    Some((days * 86_400_000 + time_millis + frac, end))
}

/// Apache and nginx: `[18/Sep/2026:14:02:05 +0200]`, the zone skipped as in ISO 8601.
fn parse_apache(head: &[u8]) -> Option<(i64, usize)> {
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
    let (time_millis, after_time) = parse_clock(b, 12)?;
    let days = days_from_civil(year, month, day)?;
    // The zone (` +0200`) and the closing bracket belong to the timestamp.
    let end = b[after_time..]
        .iter()
        .take(8)
        .position(|&c| c == b']')
        .map_or(after_time, |i| after_time + i + 1);
    Some((days * 86_400_000 + time_millis, start + end))
}

/// Bare epoch seconds (10 digits) or milliseconds (13 digits), the way container runtimes
/// and some JSON loggers write them. Shorter runs of digits are line numbers, not times.
fn parse_epoch(head: &[u8]) -> Option<(i64, usize)> {
    let digits = head.iter().take_while(|b| b.is_ascii_digit()).count();
    // A longer digit run is an id, not a time; a following digit would make it one.
    let next = head.get(digits);
    if !matches!(
        next,
        None | Some(b' ') | Some(b'\t') | Some(b',') | Some(b'.')
    ) {
        return None;
    }
    let millis = match digits {
        10 => number_i64(head, 0, 10)? * 1000,
        13 => number_i64(head, 0, 13)?,
        _ => return None,
    };
    // A fraction of a second (`1789480925.123`) is part of the timestamp too.
    let (_, end) = parse_fraction(head, digits);
    Some((millis, end))
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

/// Days since the Unix epoch of a civil date, for the calendar of the time range popup;
/// `None` for a date that does not exist or a year outside 1..=9999.
pub fn date_to_days(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=9999).contains(&year) {
        return None;
    }
    days_from_civil(year as u32, month, day)
}

/// Civil date `(year, month, day)` of a day count since the Unix epoch.
pub fn days_to_date(days: i64) -> (i64, u32, u32) {
    civil_from_days(days)
}

/// Day of the week of a day count since the Unix epoch, Monday = 0 (ISO 8601): the epoch
/// itself was a Thursday.
pub fn weekday(days: i64) -> u32 {
    (days + 3).rem_euclid(7) as u32
}

/// Number of days in a month of a year, 0 for a month that does not exist.
pub fn days_in_month(year: u32, month: u32) -> u32 {
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

/// A signed time difference for the delta column and the selection span, as short as the
/// size allows while keeping milliseconds where they matter: `+12.300` below a minute,
/// `+4:05.120` below an hour, `+2:03:04` below a day, `+3d 04:05` beyond.
pub fn format_delta(millis: i64) -> String {
    let sign = if millis < 0 { '-' } else { '+' };
    let abs = millis.unsigned_abs();
    let (ms, secs) = (abs % 1000, abs / 1000);
    let (s, mins) = (secs % 60, secs / 60);
    let (m, hours) = (mins % 60, mins / 60);
    let (h, days) = (hours % 24, hours / 24);
    if days > 0 {
        format!("{sign}{days}d {h:02}:{m:02}")
    } else if hours > 0 {
        format!("{sign}{h}:{m:02}:{s:02}")
    } else if mins > 0 {
        format!("{sign}{m}:{s:02}.{ms:03}")
    } else {
        format!("{sign}{s}.{ms:03}")
    }
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

/// The current instant on the local clock, in milliseconds since the epoch as if the
/// local time were UTC: what the calendar calls "today", and the same kind of value the
/// parser gives a log line, which is read on the clock it printed.
pub fn local_now_millis() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    now + local_offset_millis(now)
}

/// Offset of the local time zone from UTC at `utc_millis`, daylight saving included,
/// from the operating system (no time zone database is shipped): the instant converted
/// by `SystemTimeToTzSpecificLocalTime`, which applies the zone's daylight saving rules
/// to that date. Falls back to the offset in force now for an instant Windows cannot
/// convert (before 1601), and to 0 where nothing can be read.
#[cfg(windows)]
pub fn local_offset_millis(utc_millis: i64) -> i64 {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SystemTime {
        fields: [u16; 8],
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    extern "system" {
        fn FileTimeToSystemTime(file_time: *const FileTime, system_time: *mut SystemTime) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const std::ffi::c_void,
            universal: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
        fn SystemTimeToFileTime(system_time: *const SystemTime, file_time: *mut FileTime) -> i32;
    }
    // 100 ns ticks since 1601-01-01.
    const EPOCH_1601_MS: i64 = 11_644_473_600_000;
    let ticks = utc_millis
        .checked_add(EPOCH_1601_MS)
        .filter(|ms| *ms >= 0)
        .and_then(|ms| ms.checked_mul(10_000));
    if let Some(ticks) = ticks {
        let ticks = ticks as u64;
        let file = FileTime {
            low: ticks as u32,
            high: (ticks >> 32) as u32,
        };
        let mut utc = SystemTime { fields: [0; 8] };
        let mut local = SystemTime { fields: [0; 8] };
        let mut back = FileTime { low: 0, high: 0 };
        // SAFETY: plain C structs of the documented layouts, living for the calls; a null
        // zone means the current time zone.
        let ok = unsafe {
            FileTimeToSystemTime(&file, &mut utc) != 0
                && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
                && SystemTimeToFileTime(&local, &mut back) != 0
        };
        if ok {
            let local_ticks = (u64::from(back.high) << 32 | u64::from(back.low)) as i64;
            // Whole milliseconds: the input had no sub-millisecond part.
            return (local_ticks - ticks as i64) / 10_000;
        }
    }
    local_offset_now_millis()
}

/// Offset of the local time zone from UTC in force now (`GetTimeZoneInformation`).
#[cfg(windows)]
fn local_offset_now_millis() -> i64 {
    #[repr(C)]
    struct SystemTime {
        fields: [u16; 8],
    }
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    let mut info = TimeZoneInformation {
        bias: 0,
        standard_name: [0; 32],
        standard_date: SystemTime { fields: [0; 8] },
        standard_bias: 0,
        daylight_name: [0; 32],
        daylight_date: SystemTime { fields: [0; 8] },
        daylight_bias: 0,
    };
    // SAFETY: the struct has the layout of TIME_ZONE_INFORMATION and lives for the call.
    let kind = unsafe { GetTimeZoneInformation(&mut info) };
    // The bias is in minutes and counts from local time to UTC.
    let bias = match kind {
        0 => info.bias,
        1 => info.bias + info.standard_bias,
        2 => info.bias + info.daylight_bias,
        _ => return 0,
    };
    -i64::from(bias) * 60_000
}

/// Offset of the local time zone from UTC at `utc_millis`, daylight saving included,
/// from the C library's `localtime_r`; 0 where it cannot be read.
#[cfg(all(unix, target_pointer_width = "64"))]
pub fn local_offset_millis(utc_millis: i64) -> i64 {
    use std::os::raw::{c_char, c_int, c_long};
    // `struct tm` of glibc, musl and macOS; only the offset is read back.
    #[allow(dead_code)]
    #[repr(C)]
    struct Tm {
        fields: [c_int; 9],
        gmtoff: c_long,
        zone: *const c_char,
    }
    extern "C" {
        fn tzset();
        fn localtime_r(time: *const i64, tm: *mut Tm) -> *mut Tm;
    }
    let secs = utc_millis.div_euclid(1000);
    let mut tm = Tm {
        fields: [0; 9],
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    // SAFETY: `tm` has the layout of `struct tm` on these targets (64-bit `time_t`) and
    // both pointers live for the call.
    let ok = unsafe {
        tzset();
        !localtime_r(&secs, &mut tm).is_null()
    };
    if ok {
        i64::from(tm.gmtoff) * 1000
    } else {
        0
    }
}

/// Offset of the local time zone from UTC: unknown on this platform, so UTC.
#[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
pub fn local_offset_millis(_utc_millis: i64) -> i64 {
    0
}

/// A relative time: `now`, or `-` followed by one or more number-and-unit pairs with the
/// units `s`, `m`, `h`, `d` and `w` (case-insensitive, no spaces inside: `-15m`, `-3h`,
/// `-1h30m`), meaning `now` minus that duration. `None` for anything else, which is how
/// every form `parse_user_time` reads comes out (none starts with `-` or is `now`). The
/// result is an exact instant: a relative "to" side is not widened to the end of a unit.
pub fn parse_relative(input: &str, now: i64) -> Option<i64> {
    let text = input.trim();
    if text.eq_ignore_ascii_case("now") {
        return Some(now);
    }
    let mut rest = text.strip_prefix('-')?.as_bytes();
    if rest.is_empty() {
        return None;
    }
    let mut back: i64 = 0;
    while !rest.is_empty() {
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || digits == rest.len() {
            return None;
        }
        let n: i64 = std::str::from_utf8(&rest[..digits]).ok()?.parse().ok()?;
        let unit: i64 = match rest[digits].to_ascii_lowercase() {
            b's' => 1_000,
            b'm' => 60_000,
            b'h' => 3_600_000,
            b'd' => 86_400_000,
            b'w' => 7 * 86_400_000,
            _ => return None,
        };
        back = back.checked_add(n.checked_mul(unit)?)?;
        rest = &rest[digits + 1..];
    }
    now.checked_sub(back)
}

/// Whether the user typed a date alone (`YYYY-MM-DD`), which names a whole day rather
/// than an instant.
pub fn is_bare_date(input: &str) -> bool {
    is_date(input.trim())
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

/// The leading timestamp of a line as `detect_timestamp_zoned` reads it: milliseconds on
/// the printed clock (as `detect_timestamp`), the zone the line states, and where the
/// timestamp text sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZonedStamp {
    pub millis: i64,
    /// Offset from UTC in minutes the line states: `Z` is 0, `+02:00` is 120, and an
    /// epoch value is 0; `None` when the line names no zone.
    pub zone_minutes: Option<i32>,
    /// Byte range of the timestamp text, zone included, brackets excluded.
    pub start: usize,
    pub end: usize,
    /// Digits of the fraction of a second the log printed (0 for none).
    pub frac_digits: u8,
    pub format: FormatHint,
}

/// `detect_timestamp` with the zone offset and the span of the timestamp text.
pub fn detect_timestamp_zoned(line: &str, hint: FormatHint) -> Option<ZonedStamp> {
    let head = &line.as_bytes()[..line.len().min(SCAN_BYTES)];
    for format in format_order(hint) {
        if let Some((mut millis, _)) = parse_format(format, head) {
            // Epoch seconds keep their fraction here, which the time cache has no use for.
            if format == FormatHint::Epoch
                && head.iter().take_while(|b| b.is_ascii_digit()).count() == 10
            {
                millis += parse_fraction(head, 10).0;
            }
            return zoned_parts(format, head).map(|(zone_minutes, start, end, frac_digits)| {
                ZonedStamp {
                    millis,
                    zone_minutes,
                    start,
                    end,
                    frac_digits,
                    format,
                }
            });
        }
    }
    None
}

/// Zone, start, end and fraction digits of a timestamp `format` already parsed at the
/// start of `head`.
fn zoned_parts(format: FormatHint, head: &[u8]) -> Option<(Option<i32>, usize, usize, u8)> {
    let digits_of = |b: &[u8], at: usize| -> (usize, u8) {
        let (_, after) = parse_fraction(b, at);
        let digits = if after > at { after - at - 1 } else { 0 };
        (after, digits.min(u8::MAX as usize) as u8)
    };
    match format {
        FormatHint::Iso8601 => {
            let start = skip_leading_bracket(head);
            let b = &head[start..];
            let (after_frac, digits) = digits_of(b, 19);
            let (zone, end) = parse_zone(b, after_frac);
            Some((zone, start, start + end, digits))
        }
        FormatHint::Syslog => {
            let start = skip_leading_bracket(head);
            let b = &head[start..];
            let (after_frac, digits) = digits_of(b, 15);
            Some((None, start, start + after_frac, digits))
        }
        FormatHint::Apache => {
            let start = head.iter().position(|b| *b == b'[')? + 1;
            let b = &head[start..];
            let mut end = 20;
            while b.get(end) == Some(&b' ') {
                end += 1;
            }
            let (zone, zone_end) = parse_zone(b, end);
            let end = if zone.is_some() { zone_end } else { 20 };
            Some((zone, start, start + end, 0))
        }
        FormatHint::Epoch => {
            let digits = head.iter().take_while(|b| b.is_ascii_digit()).count();
            let (after, frac) = digits_of(head, digits);
            // Milliseconds carry their three digits of fraction in the number itself.
            let frac = if digits == 13 { 3 } else { frac };
            Some((Some(0), 0, after, frac))
        }
        FormatHint::Unknown => None,
    }
}

/// The zone suffix at `at` as minutes east of UTC and the index after it: `Z`, `+02:00`,
/// `+0200`, `-03`; `(None, at)` when there is none.
fn parse_zone(b: &[u8], at: usize) -> (Option<i32>, usize) {
    match b.get(at) {
        Some(b'Z') | Some(b'z') => (Some(0), at + 1),
        Some(sign @ (b'+' | b'-')) => {
            let Some(hours) = number(b, at + 1, 2) else {
                return (None, at);
            };
            let (minutes, end) = if b.get(at + 3) == Some(&b':') {
                match number(b, at + 4, 2) {
                    Some(m) => (m, at + 6),
                    None => (0, at + 3),
                }
            } else if let Some(m) = number(b, at + 3, 2) {
                (m, at + 5)
            } else {
                (0, at + 3)
            };
            // Real zones run from -12:00 to +14:00: `+99:00` is no zone.
            if hours > 14 || minutes > 59 {
                return (None, at);
            }
            let total = (hours * 60 + minutes) as i32;
            (Some(if *sign == b'-' { -total } else { total }), end)
        }
        _ => (None, at),
    }
}

/// Largest fixed offset a time display or a source zone accepts, in minutes (14:00).
pub const MAX_OFFSET_MINUTES: i32 = 14 * 60;

/// How a stream shows the leading timestamp of its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeDisplay {
    /// As the log printed it (no conversion).
    #[default]
    Written,
    Utc,
    /// The local time zone, with the offset in force at each instant.
    Local,
    /// A fixed offset from UTC, in minutes.
    Offset(i32),
}

/// What a timestamp without a zone suffix means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceZone {
    #[default]
    Local,
    Utc,
    Offset(i32),
}

/// `+02:00`, `-03:30`, `+00:00`.
pub fn format_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let m = minutes.unsigned_abs();
    format!("{sign}{:02}:{:02}", m / 60, m % 60)
}

/// `+02:00`, `+0200`, `-3`, `+5:30`, `UTC+2` read as minutes east of UTC, within
/// `MAX_OFFSET_MINUTES`.
pub fn parse_offset(text: &str) -> Option<i32> {
    let text = text.trim();
    let text = text
        .strip_prefix("UTC")
        .or_else(|| text.strip_prefix("utc"))
        .or_else(|| text.strip_prefix("GMT"))
        .unwrap_or(text);
    let (sign, rest) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    let (hours, minutes) = match rest.split_once(':') {
        Some((h, m)) => (h, m),
        None if rest.len() == 4 => rest.split_at(2),
        None => (rest, "0"),
    };
    if hours.is_empty() || hours.len() > 2 || minutes.len() > 2 {
        return None;
    }
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    if minutes > 59 {
        return None;
    }
    let total = sign * (hours * 60 + minutes);
    (total.abs() <= MAX_OFFSET_MINUTES).then_some(total)
}

impl TimeDisplay {
    /// `written`, `utc`, `local` or `+HH:MM`, as saved in the workspace and sessions.
    pub fn to_config(self) -> String {
        match self {
            TimeDisplay::Written => "written".into(),
            TimeDisplay::Utc => "utc".into(),
            TimeDisplay::Local => "local".into(),
            TimeDisplay::Offset(m) => format_offset(m),
        }
    }

    pub fn from_config(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "written" => Some(TimeDisplay::Written),
            "utc" | "z" => Some(TimeDisplay::Utc),
            "local" => Some(TimeDisplay::Local),
            other => parse_offset(other).map(TimeDisplay::Offset),
        }
    }
}

impl SourceZone {
    /// `local`, `utc` or `+HH:MM`, as saved in the workspace and sessions.
    pub fn to_config(self) -> String {
        match self {
            SourceZone::Local => "local".into(),
            SourceZone::Utc => "utc".into(),
            SourceZone::Offset(m) => format_offset(m),
        }
    }

    pub fn from_config(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "local" => Some(SourceZone::Local),
            "utc" | "z" => Some(SourceZone::Utc),
            other => parse_offset(other).map(SourceZone::Offset),
        }
    }
}

/// The UTC instant of a reading on the local clock: the offset in force at that instant
/// (`local` gives it for a UTC instant, see `local_offset_millis`).
pub fn local_clock_to_utc(clock: i64, local: &dyn Fn(i64) -> i64) -> i64 {
    let guess = clock - local(clock);
    clock - local(guess)
}

/// The UTC instant of a printed timestamp: its own zone when the line states one (epoch
/// values are UTC), else the stream's source zone.
pub fn stamp_to_utc(
    printed: i64,
    zone_minutes: Option<i32>,
    source: SourceZone,
    local: &dyn Fn(i64) -> i64,
) -> i64 {
    match (zone_minutes, source) {
        (Some(z), _) => printed - i64::from(z) * 60_000,
        (None, SourceZone::Utc) => printed,
        (None, SourceZone::Offset(m)) => printed - i64::from(m) * 60_000,
        (None, SourceZone::Local) => local_clock_to_utc(printed, local),
    }
}

/// The inverse of `stamp_to_utc`: the printed clock of a UTC instant.
pub fn utc_to_stamp(
    utc: i64,
    zone_minutes: Option<i32>,
    source: SourceZone,
    local: &dyn Fn(i64) -> i64,
) -> i64 {
    match (zone_minutes, source) {
        (Some(z), _) => utc + i64::from(z) * 60_000,
        (None, SourceZone::Utc) => utc,
        (None, SourceZone::Offset(m)) => utc + i64::from(m) * 60_000,
        (None, SourceZone::Local) => utc + local(utc),
    }
}

/// The clock of a UTC instant in `display` (`Written` shows UTC: it is never converted).
pub fn utc_to_display(utc: i64, display: TimeDisplay, local: &dyn Fn(i64) -> i64) -> i64 {
    match display {
        TimeDisplay::Written | TimeDisplay::Utc => utc,
        TimeDisplay::Local => utc + local(utc),
        TimeDisplay::Offset(m) => utc + i64::from(m) * 60_000,
    }
}

/// The UTC instant of a reading on the `display` clock (a time typed by the user).
pub fn display_to_utc(shown: i64, display: TimeDisplay, local: &dyn Fn(i64) -> i64) -> i64 {
    match display {
        TimeDisplay::Written | TimeDisplay::Utc => shown,
        TimeDisplay::Local => local_clock_to_utc(shown, local),
        TimeDisplay::Offset(m) => shown - i64::from(m) * 60_000,
    }
}

/// `YYYY-MM-DD HH:MM:SS`, the fraction to `frac_digits` digits (at most 3), and `Z` for
/// UTC or the offset for a fixed one.
pub fn format_in_zone(shown: i64, frac_digits: u8, display: TimeDisplay) -> String {
    let mut out = format_millis(shown);
    let digits = frac_digits.min(3) as usize;
    if digits > 0 {
        let ms = format!("{:03}", shown.rem_euclid(1000));
        out.push('.');
        out.push_str(&ms[..digits]);
    }
    match display {
        TimeDisplay::Utc => out.push('Z'),
        TimeDisplay::Offset(m) => out.push_str(&format_offset(m)),
        TimeDisplay::Written | TimeDisplay::Local => {}
    }
    out
}

/// The leading timestamp of `line` as the `display` shows it: the byte range to replace
/// and its text. `None` when the display is "as written" or the line has no timestamp.
pub fn display_timestamp(
    line: &str,
    hint: FormatHint,
    source: SourceZone,
    display: TimeDisplay,
    local: &dyn Fn(i64) -> i64,
) -> Option<(usize, usize, String)> {
    display_timestamp_with(line, hint, true, source, display, local)
}

/// Epoch values converted to dates: 2000-01-01 to 2100-01-01, so a count such as
/// `1234567890 rows` far outside it stays a number.
const EPOCH_DISPLAY_RANGE: std::ops::Range<i64> = 946_684_800_000..4_102_444_800_000;

/// `display_timestamp`, converting a bare epoch value only when `epoch` says the stream
/// is an epoch-stamped one (a leading number in an ISO log is a count, not a time), and
/// only within `EPOCH_DISPLAY_RANGE`.
pub fn display_timestamp_with(
    line: &str,
    hint: FormatHint,
    epoch: bool,
    source: SourceZone,
    display: TimeDisplay,
    local: &dyn Fn(i64) -> i64,
) -> Option<(usize, usize, String)> {
    if display == TimeDisplay::Written {
        return None;
    }
    let stamp = detect_timestamp_zoned(line, hint)?;
    if stamp.format == FormatHint::Epoch && (!epoch || !EPOCH_DISPLAY_RANGE.contains(&stamp.millis))
    {
        return None;
    }
    let utc = stamp_to_utc(stamp.millis, stamp.zone_minutes, source, local);
    let shown = utc_to_display(utc, display, local);
    Some((
        stamp.start,
        stamp.end,
        format_in_zone(shown, stamp.frac_digits, display),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(line: &str) -> Option<i64> {
        detect_timestamp(line, FormatHint::Unknown).map(|(millis, _)| millis)
    }

    #[test]
    fn deltas_are_signed_and_shortened_by_size() {
        assert_eq!(format_delta(0), "+0.000");
        assert_eq!(format_delta(125), "+0.125");
        assert_eq!(format_delta(12_300), "+12.300");
        assert_eq!(format_delta(59_999), "+59.999");
        assert_eq!(format_delta(60_000), "+1:00.000");
        assert_eq!(format_delta(245_120), "+4:05.120");
        assert_eq!(format_delta(3_599_999), "+59:59.999");
        assert_eq!(format_delta(3_600_000), "+1:00:00");
        assert_eq!(format_delta(7_384_999), "+2:03:04", "seconds are truncated");
        assert_eq!(format_delta(86_399_999), "+23:59:59");
        assert_eq!(format_delta(86_400_000), "+1d 00:00");
        assert_eq!(format_delta(273_900_000), "+3d 04:05");
        // Rows above the anchor and out-of-order lines.
        assert_eq!(format_delta(-500), "-0.500");
        assert_eq!(format_delta(-245_120), "-4:05.120");
        assert_eq!(format_delta(-273_900_000), "-3d 04:05");
        assert_eq!(format_delta(i64::MIN).chars().next(), Some('-'));
        // The widest value below a day fits the 10-character column.
        assert!(format_delta(-3_599_999).len() <= 10);
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
    fn civil_dates_for_the_calendar() {
        // The epoch, a Thursday, and the way back.
        assert_eq!(date_to_days(1970, 1, 1), Some(0));
        assert_eq!(days_to_date(0), (1970, 1, 1));
        assert_eq!(weekday(0), 3);
        // 2026-09-28 is a Monday; 2100-01-01 a Friday.
        let monday = date_to_days(2026, 9, 28).unwrap();
        assert_eq!(weekday(monday), 0);
        assert_eq!(weekday(monday + 6), 6);
        assert_eq!(weekday(monday - 1), 6);
        let y2100 = date_to_days(2100, 1, 1).unwrap();
        assert_eq!(weekday(y2100), 4);
        assert_eq!(days_to_date(y2100), (2100, 1, 1));
        // Month ends and leap years.
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 13), 0);
        assert_eq!(date_to_days(2026, 2, 29), None);
        assert_eq!(
            date_to_days(2024, 2, 29).map(days_to_date),
            Some((2024, 2, 29))
        );
        assert_eq!(
            date_to_days(2026, 3, 1),
            date_to_days(2026, 2, 28).map(|d| d + 1)
        );
        assert_eq!(date_to_days(0, 1, 1), None);
        assert_eq!(date_to_days(10_000, 1, 1), None);
        // A date alone, and what is not one.
        assert!(is_bare_date(" 2026-09-18 "));
        assert!(!is_bare_date("2026-09-18 14:02"));
        assert!(!is_bare_date("14:02"));
    }

    #[test]
    fn the_local_offset_is_a_real_time_zone() {
        // Zones run from UTC-12 to UTC+14, in steps of a quarter of an hour.
        let offset = local_offset_millis(1_789_740_125_000);
        assert!(offset.abs() <= 14 * 3_600_000, "{offset}");
        assert_eq!(offset % (15 * 60_000), 0, "{offset}");
        let utc = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!((local_now_millis() - utc).abs() <= 14 * 3_600_000 + 60_000);
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
    fn leading_span_covers_each_format_with_its_zone_and_bracket() {
        fn span(line: &str) -> Option<&str> {
            leading_span(line, FormatHint::Unknown).map(|n| &line[..n])
        }
        assert_eq!(
            span("2026-09-18T14:02:05.123Z ERROR boom"),
            Some("2026-09-18T14:02:05.123Z")
        );
        assert_eq!(
            span("2026-09-18 14:02:05,123 INFO x"),
            Some("2026-09-18 14:02:05,123")
        );
        assert_eq!(
            span("2026-09-18T14:02:05+02:00 zoned"),
            Some("2026-09-18T14:02:05+02:00")
        );
        assert_eq!(
            span("2026-09-18T14:02:05.123-0300 zoned"),
            Some("2026-09-18T14:02:05.123-0300")
        );
        assert_eq!(
            span("[2026-09-18 14:02:05.123] bracketed"),
            Some("[2026-09-18 14:02:05.123]")
        );
        assert_eq!(
            span("Sep 18 14:02:05 host app[1]: started"),
            Some("Sep 18 14:02:05")
        );
        assert_eq!(
            span("[18/Sep/2026:14:02:05 +0200] \"GET /\""),
            Some("[18/Sep/2026:14:02:05 +0200]")
        );
        assert_eq!(span("1789480925 epoch seconds"), Some("1789480925"));
        assert_eq!(span("1789480925.250 fractional"), Some("1789480925.250"));
        assert_eq!(span("1789480925123 epoch millis"), Some("1789480925123"));
        // A bare clock is a prefix to drop, though not a point in time.
        assert_eq!(span("12:00:01.250 WARN retrying"), Some("12:00:01.250"));
        assert_eq!(span("[12:00:01] WARN retrying"), Some("[12:00:01]"));
        assert_eq!(detect("12:00:01.250 WARN retrying"), None);
        // The hint changes the order, not the answer.
        assert_eq!(
            leading_span("2026-09-18T14:02:05Z x", FormatHint::Epoch),
            Some("2026-09-18T14:02:05Z".len())
        );
    }

    #[test]
    fn leading_span_ignores_timestamps_that_do_not_open_the_line() {
        let none = |line: &str| leading_span(line, FormatHint::Unknown);
        assert_eq!(none("ERROR something went wrong"), None);
        assert_eq!(none("    at Foo.bar(Foo.java:10)"), None);
        assert_eq!(none(""), None);
        // The Apache bracket after the client fields of a combined-log line.
        assert_eq!(
            none("127.0.0.1 - - [18/Sep/2026:14:02:05 +0000] \"GET / HTTP/1.1\" 200"),
            None
        );
        assert_eq!(none("12345 short"), None);
        assert_eq!(none("25:61:00 not a clock"), None);
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

    #[test]
    fn relative_times() {
        let now = 1_000_000_000_000;
        assert_eq!(parse_relative("now", now), Some(now));
        assert_eq!(parse_relative(" NOW ", now), Some(now));
        assert_eq!(parse_relative("-15m", now), Some(now - 15 * 60_000));
        assert_eq!(parse_relative("-3h", now), Some(now - 3 * 3_600_000));
        assert_eq!(parse_relative("-2d", now), Some(now - 2 * 86_400_000));
        assert_eq!(parse_relative("-1w", now), Some(now - 7 * 86_400_000));
        assert_eq!(parse_relative("-90s", now), Some(now - 90_000));
        assert_eq!(
            parse_relative("-1H30M", now),
            Some(now - 3_600_000 - 30 * 60_000)
        );
        for bad in [
            "15m",
            "-",
            "-m",
            "-1.5h",
            "-3x",
            "-1h 30m",
            "-1h30",
            "",
            "nowish",
            "- 1h",
            "14:02",
            "2026-09-28",
        ] {
            assert_eq!(parse_relative(bad, now), None, "{bad}");
        }
        assert_eq!(parse_relative("-99999999999999999w", now), None);
    }
}

#[cfg(test)]
mod zone_tests {
    use super::*;

    const H: i64 = 3_600_000;

    /// Central European time: +01:00, +02:00 from 2026-03-29 01:00 UTC to 2026-10-25
    /// 01:00 UTC (the EU rule), for tests that must not depend on the machine's zone.
    fn cet(utc: i64) -> i64 {
        let spring = parse_user_time("2026-03-29 01:00:00", 0).unwrap();
        let autumn = parse_user_time("2026-10-25 01:00:00", 0).unwrap();
        if (spring..autumn).contains(&utc) {
            2 * H
        } else {
            H
        }
    }

    fn at(text: &str) -> i64 {
        parse_user_time(text, 0).unwrap()
    }

    fn zoned(line: &str) -> ZonedStamp {
        detect_timestamp_zoned(line, FormatHint::Unknown).unwrap()
    }

    #[test]
    fn zones_and_spans_of_each_format() {
        let z = zoned("2026-09-28T14:02:05.123Z INFO x");
        assert_eq!(z.millis, at("2026-09-28 14:02:05") + 123);
        assert_eq!(
            (z.zone_minutes, z.start, z.end, z.frac_digits),
            (Some(0), 0, 24, 3)
        );
        let z = zoned("[2026-09-28 14:02:05,5+02:00] x");
        assert_eq!(
            (z.zone_minutes, z.start, z.end, z.frac_digits),
            (Some(120), 1, 28, 1)
        );
        let z = zoned("2026-09-28 14:02:05-0330 x");
        assert_eq!((z.zone_minutes, z.end), (Some(-210), 24));
        let z = zoned("2026-09-28 14:02:05 x");
        assert_eq!((z.zone_minutes, z.end, z.frac_digits), (None, 19, 0));
        let z = zoned("Sep 28 14:02:05 host x");
        assert_eq!(
            (z.zone_minutes, z.start, z.end, z.format),
            (None, 0, 15, FormatHint::Syslog)
        );
        let line = "10.0.0.1 - - [28/Sep/2026:14:02:05 +0200] \"GET /\"";
        let z = zoned(line);
        assert_eq!(z.zone_minutes, Some(120));
        assert_eq!(&line[z.start..z.end], "28/Sep/2026:14:02:05 +0200");
        let z = zoned("1790604125123 x");
        assert_eq!(
            (z.millis, z.zone_minutes, z.end, z.frac_digits),
            (1_790_604_125_123, Some(0), 13, 3)
        );
        let z = zoned("1790604125.25 x");
        assert_eq!((z.millis, z.end, z.frac_digits), (1_790_604_125_250, 13, 2));
        let z = zoned("1790604125 x");
        assert_eq!((z.millis, z.frac_digits), (1_790_604_125_000, 0));
        assert!(detect_timestamp_zoned("no time here", FormatHint::Unknown).is_none());
    }

    #[test]
    fn utc_log_read_in_local_time() {
        let line = "2026-09-28T14:02:05.123Z INFO x";
        let (s, e, text) = display_timestamp(
            line,
            FormatHint::Unknown,
            SourceZone::Local,
            TimeDisplay::Local,
            &cet,
        )
        .unwrap();
        assert_eq!(&line[s..e], "2026-09-28T14:02:05.123Z");
        assert_eq!(text, "2026-09-28 16:02:05.123");
        let utc = display_timestamp(
            line,
            FormatHint::Unknown,
            SourceZone::Local,
            TimeDisplay::Utc,
            &cet,
        );
        assert_eq!(utc.unwrap().2, "2026-09-28 14:02:05.123Z");
        let fixed = display_timestamp(
            line,
            FormatHint::Unknown,
            SourceZone::Local,
            TimeDisplay::Offset(-300),
            &cet,
        );
        assert_eq!(fixed.unwrap().2, "2026-09-28 09:02:05.123-05:00");
        // As written: nothing to replace.
        assert!(display_timestamp(
            line,
            FormatHint::Unknown,
            SourceZone::Local,
            TimeDisplay::Written,
            &cet
        )
        .is_none());
    }

    #[test]
    fn epoch_milliseconds_as_a_date() {
        let (s, e, text) = display_timestamp(
            "1790604125123 payload",
            FormatHint::Unknown,
            SourceZone::Local,
            TimeDisplay::Utc,
            &cet,
        )
        .unwrap();
        assert_eq!((s, e), (0, 13));
        assert_eq!(text, "2026-09-28 14:02:05.123Z");
    }

    #[test]
    fn source_zone_for_timestamps_without_one() {
        let line = "2026-09-28 14:02:05 x";
        let show = |source| {
            display_timestamp(line, FormatHint::Unknown, source, TimeDisplay::Utc, &cet)
                .unwrap()
                .2
        };
        assert_eq!(show(SourceZone::Utc), "2026-09-28 14:02:05Z");
        assert_eq!(show(SourceZone::Local), "2026-09-28 12:02:05Z");
        assert_eq!(show(SourceZone::Offset(330)), "2026-09-28 08:32:05Z");
        // A zone in the line wins over the source zone.
        let own = display_timestamp(
            "2026-09-28 14:02:05+01:00 x",
            FormatHint::Unknown,
            SourceZone::Offset(330),
            TimeDisplay::Utc,
            &cet,
        );
        assert_eq!(own.unwrap().2, "2026-09-28 13:02:05Z");
        // Syslog and Apache.
        let sys = display_timestamp(
            "Sep 28 14:02:05 h",
            FormatHint::Unknown,
            SourceZone::Utc,
            TimeDisplay::Offset(60),
            &cet,
        );
        assert!(sys.unwrap().2.ends_with("15:02:05+01:00"));
        let apache = display_timestamp(
            "[28/Sep/2026:14:02:05 +0200] x",
            FormatHint::Unknown,
            SourceZone::Local,
            TimeDisplay::Utc,
            &cet,
        );
        assert_eq!(apache.unwrap(), (1, 27, "2026-09-28 12:02:05Z".to_string()));
    }

    #[test]
    fn daylight_saving_boundaries_in_local_time() {
        // Each instant gets the offset in force at that instant.
        let show = |utc_text: &str| {
            let line = format!("{utc_text}Z x");
            display_timestamp(
                &line,
                FormatHint::Unknown,
                SourceZone::Utc,
                TimeDisplay::Local,
                &cet,
            )
            .unwrap()
            .2
        };
        assert_eq!(show("2026-03-29T00:59:59"), "2026-03-29 01:59:59");
        assert_eq!(show("2026-03-29T01:00:00"), "2026-03-29 03:00:00");
        assert_eq!(show("2026-10-25T00:59:59"), "2026-10-25 02:59:59");
        assert_eq!(show("2026-10-25T01:00:00"), "2026-10-25 02:00:00");
        // Local readings back to UTC on each side of a change.
        assert_eq!(
            local_clock_to_utc(at("2026-03-29 03:30:00"), &cet),
            at("2026-03-29 01:30:00")
        );
        assert_eq!(
            local_clock_to_utc(at("2026-03-29 00:30:00"), &cet),
            at("2026-03-28 23:30:00")
        );
        assert_eq!(
            local_clock_to_utc(at("2026-07-01 12:00:00"), &cet),
            at("2026-07-01 10:00:00")
        );
        assert_eq!(
            local_clock_to_utc(at("2026-12-01 12:00:00"), &cet),
            at("2026-12-01 11:00:00")
        );
    }

    #[test]
    fn conversions_round_trip() {
        for display in [
            TimeDisplay::Utc,
            TimeDisplay::Local,
            TimeDisplay::Offset(-570),
        ] {
            for (zone, source) in [
                (Some(0), SourceZone::Local),
                (None, SourceZone::Utc),
                (None, SourceZone::Local),
                (None, SourceZone::Offset(345)),
            ] {
                let printed = at("2026-07-14 09:15:00");
                let utc = stamp_to_utc(printed, zone, source, &cet);
                let shown = utc_to_display(utc, display, &cet);
                let back = utc_to_stamp(display_to_utc(shown, display, &cet), zone, source, &cet);
                assert_eq!(back, printed, "{display:?} {zone:?} {source:?}");
            }
        }
    }

    #[test]
    fn epoch_only_on_epoch_streams_and_in_range_and_sane_zones() {
        let show = |line: &str, epoch: bool| {
            display_timestamp_with(
                line,
                FormatHint::Unknown,
                epoch,
                SourceZone::Utc,
                TimeDisplay::Utc,
                &cet,
            )
            .map(|t| t.2)
        };
        assert_eq!(show("1234567890 rows", false), None);
        assert!(show("1790604125123 x", true).is_some());
        // Out of 2000..2100: a number, not a date.
        assert_eq!(show("0000000001 x", true), None);
        assert_eq!(show("9999999999999 x", true), None);
        // A zone out of range is no zone: the line reads in the source zone.
        let z = zoned("2026-09-28 14:02:05+99:00 x");
        assert_eq!((z.zone_minutes, z.end), (None, 19));
        let z = zoned("2026-09-28 14:02:05+05:75 x");
        assert_eq!(z.zone_minutes, None);
        assert_eq!(zoned("2026-09-28 14:02:05+14:00 x").zone_minutes, Some(840));
    }

    #[test]
    fn offsets_and_config_names() {
        assert_eq!(format_offset(120), "+02:00");
        assert_eq!(format_offset(-210), "-03:30");
        assert_eq!(format_offset(0), "+00:00");
        assert_eq!(parse_offset("+02:00"), Some(120));
        assert_eq!(parse_offset("+0530"), Some(330));
        assert_eq!(parse_offset("-3"), Some(-180));
        assert_eq!(parse_offset("UTC+14"), Some(840));
        assert_eq!(parse_offset("+14:30"), None);
        assert_eq!(parse_offset("+05:60"), None);
        assert_eq!(parse_offset("02:00"), None);
        assert_eq!(parse_offset(""), None);
        for d in [
            TimeDisplay::Written,
            TimeDisplay::Utc,
            TimeDisplay::Local,
            TimeDisplay::Offset(-720),
        ] {
            assert_eq!(TimeDisplay::from_config(&d.to_config()), Some(d));
        }
        for z in [SourceZone::Local, SourceZone::Utc, SourceZone::Offset(90)] {
            assert_eq!(SourceZone::from_config(&z.to_config()), Some(z));
        }
        assert_eq!(TimeDisplay::from_config("sideways"), None);
        assert_eq!(SourceZone::from_config("written"), None);
    }

    #[test]
    fn fraction_digits_as_printed() {
        let ms = at("2026-01-02 03:04:05") + 670;
        assert_eq!(
            format_in_zone(ms, 0, TimeDisplay::Local),
            "2026-01-02 03:04:05"
        );
        assert_eq!(
            format_in_zone(ms, 1, TimeDisplay::Utc),
            "2026-01-02 03:04:05.6Z"
        );
        assert_eq!(
            format_in_zone(ms, 6, TimeDisplay::Offset(60)),
            "2026-01-02 03:04:05.670+01:00"
        );
    }

    #[test]
    fn local_offset_is_plausible_and_follows_the_instant() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        for utc in [now, at("2026-01-15 12:00:00"), at("2026-07-15 12:00:00")] {
            let off = local_offset_millis(utc);
            assert!(off.abs() <= 14 * H, "{off}");
            assert_eq!(off % (15 * 60_000), 0, "{off}");
        }
    }
}
