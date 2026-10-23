// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Syslog messages as readable log lines: RFC 5424 (`<PRI>1 TIMESTAMP HOST APP PROCID
//! MSGID [SD] MSG`) and RFC 3164 (`<PRI>Mmm dd hh:mm:ss HOST TAG[pid]: MSG`) become
//! `<ISO 8601 time> <LEVEL> <host> <app>[<pid>]: <msg>`, the severity mapped to the level
//! words FastTail detects. A message that is neither is kept as it arrived.

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The level word of a syslog severity (the low 3 bits of PRI).
pub fn level_word(severity: u8) -> &'static str {
    match severity {
        0..=2 => "FATAL",
        3 => "ERROR",
        4 => "WARN",
        5 | 6 => "INFO",
        _ => "DEBUG",
    }
}

/// `<PRI>` at the start: the severity and the rest.
fn split_pri(msg: &str) -> Option<(u8, &str)> {
    let rest = msg.strip_prefix('<')?;
    let close = rest.find('>')?;
    if close == 0 || close > 3 {
        return None;
    }
    let pri: u16 = rest[..close].parse().ok()?;
    if pri > 191 {
        return None;
    }
    Some(((pri % 8) as u8, &rest[close + 1..]))
}

/// The next space-separated field of `s` and the rest.
fn field(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix(' ').unwrap_or(s);
    if s.is_empty() {
        return None;
    }
    Some(match s.find(' ') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    })
}

/// RFC 5424 structured data at the start of `s` (`-` or `[...]...`) and the rest.
fn structured_data(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix(' ').unwrap_or(s);
    if let Some(rest) = s.strip_prefix('-') {
        return Some(("", rest));
    }
    if !s.starts_with('[') {
        return None;
    }
    let b = s.as_bytes();
    let (mut i, mut depth, mut quoted) = (0, 0usize, false);
    while i < b.len() {
        match b[i] {
            b'\\' if quoted => i += 1,
            b'"' => quoted = !quoted,
            b'[' if !quoted => depth += 1,
            b']' if !quoted => {
                depth -= 1;
                if depth == 0 && b.get(i + 1) != Some(&b'[') {
                    return Some((&s[..=i], &s[i + 1..]));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn nil(s: &str) -> Option<&str> {
    (s != "-").then_some(s)
}

fn app_part(app: Option<&str>, pid: Option<&str>) -> String {
    match (app, pid) {
        (Some(a), Some(p)) => format!("{a}[{p}]:"),
        (Some(a), None) => format!("{a}:"),
        (None, Some(p)) => format!("[{p}]:"),
        (None, None) => "-:".to_string(),
    }
}

/// `msg` rendered as a log line; `now` formats the receive time as ISO 8601 with offset
/// (used when the message has no usable timestamp) and `year` is the receive year (RFC
/// 3164 has none). `None` when `msg` is not syslog.
pub fn render(msg: &str, year: i64, now: &dyn Fn() -> String) -> Option<String> {
    let msg = msg.trim_end_matches(['\r', '\n']);
    let (severity, rest) = split_pri(msg)?;
    let level = level_word(severity);
    if let Some(rest) = rest.strip_prefix("1 ") {
        // RFC 5424.
        let (ts, rest) = field(rest)?;
        let (host, rest) = field(rest)?;
        let (app, rest) = field(rest)?;
        let (pid, rest) = field(rest)?;
        let (_msgid, rest) = field(rest)?;
        let (sd, rest) = structured_data(rest)?;
        let text = rest.strip_prefix(' ').unwrap_or(rest);
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let ts = nil(ts).map_or_else(now, str::to_string);
        let mut line = format!(
            "{ts} {level} {} {} {text}",
            nil(host).unwrap_or("-"),
            app_part(nil(app), nil(pid))
        );
        if !sd.is_empty() {
            line.push(' ');
            line.push_str(sd);
        }
        return Some(line.trim_end().to_string());
    }
    // RFC 3164: `Mmm dd hh:mm:ss HOST TAG[pid]: MSG`.
    let b = rest.as_bytes();
    let month = MONTHS.iter().position(|m| rest.starts_with(m))?;
    if b.len() < 16 || b[3] != b' ' || b[6] != b' ' || b[9] != b':' || b[12] != b':' {
        return None;
    }
    let day: u32 = rest[4..6].trim().parse().ok()?;
    let clock = &rest[7..15];
    if !clock.bytes().enumerate().all(|(i, c)| {
        if i == 2 || i == 5 {
            c == b':'
        } else {
            c.is_ascii_digit()
        }
    }) {
        return None;
    }
    let rest = &rest[15..];
    let (host, rest) = field(rest)?;
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    // TAG[pid]: or TAG: (a tag of up to 48 characters before the colon).
    let (app, pid, text) = match rest.find(": ") {
        Some(colon) if colon <= 48 && !rest[..colon].contains(' ') => {
            let tag = &rest[..colon];
            let text = &rest[colon + 2..];
            match tag.find('[') {
                Some(open) if tag.ends_with(']') => (
                    Some(&tag[..open]),
                    Some(&tag[open + 1..tag.len() - 1]),
                    text,
                ),
                _ => (Some(tag), None, text),
            }
        }
        _ => (None, None, rest),
    };
    let offset = now();
    let zone = offset.get(19..).filter(|z| !z.is_empty()).unwrap_or("");
    let ts = format!(
        "{year:04}-{:02}-{day:02}T{clock}{}",
        month + 1,
        // The receive time's offset: the sender's clock is taken as local.
        zone.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit())
    );
    Some(format!("{ts} {level} {host} {} {text}", app_part(app, pid)))
}

/// The receive time as `YYYY-MM-DDTHH:MM:SS.mmm+HH:MM` (local clock) and its year.
pub fn receive_time() -> (String, i64) {
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let offset = crate::timestamp::local_offset_millis(utc);
    let local = utc + offset;
    let days = local.div_euclid(86_400_000);
    let ms = local.rem_euclid(86_400_000);
    let (y, m, d) = crate::timestamp::days_to_date(days);
    (
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}{}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000,
            crate::timestamp::format_offset((offset / 60_000) as i32)
        ),
        y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> String {
        "2026-10-23T10:00:00.000+02:00".to_string()
    }

    fn r(msg: &str) -> Option<String> {
        render(msg, 2026, &now)
    }

    #[test]
    fn rfc5424_messages() {
        assert_eq!(
            r("<165>1 2026-10-23T08:15:30.003Z web1 nginx 4242 ID47 - request served").as_deref(),
            Some("2026-10-23T08:15:30.003Z INFO web1 nginx[4242]: request served")
        );
        assert_eq!(
            r(r#"<11>1 2026-10-23T08:15:30Z db1 postgres - - [meta x="1" y="a\"]b"][origin ip="10.0.0.7"] ﻿disk full"#).as_deref(),
            Some(r#"2026-10-23T08:15:30Z ERROR db1 postgres: disk full [meta x="1" y="a\"]b"][origin ip="10.0.0.7"]"#)
        );
        // No timestamp: the receive time.
        assert_eq!(
            r("<15>1 - - app - - - hello").as_deref(),
            Some("2026-10-23T10:00:00.000+02:00 DEBUG - app: hello")
        );
    }

    #[test]
    fn rfc3164_messages() {
        assert_eq!(
            r("<34>Oct 11 22:14:15 mymachine su: 'su root' failed for lonvick on /dev/pts/8").as_deref(),
            Some("2026-10-11T22:14:15+02:00 FATAL mymachine su: 'su root' failed for lonvick on /dev/pts/8")
        );
        assert_eq!(
            r("<30>Oct  3 07:01:02 host1 systemd[1]: Started Session 4.").as_deref(),
            Some("2026-10-03T07:01:02+02:00 INFO host1 systemd[1]: Started Session 4.")
        );
        assert_eq!(
            r("<28>Oct 23 09:00:00 fw1 kernel dropped a packet").as_deref(),
            Some("2026-10-23T09:00:00+02:00 WARN fw1 -: kernel dropped a packet")
        );
    }

    #[test]
    fn other_messages_are_not_syslog() {
        // A Cisco message after its sequence number, plain text, a bad priority.
        assert_eq!(
            r("<189>123: *Mar  1 00:00:00.000: %SYS-5-CONFIG_I: x"),
            None
        );
        assert_eq!(r("plain line"), None);
        assert_eq!(r("<999>1 - - - - - - x"), None);
        assert_eq!(level_word(7), "DEBUG");
    }

    #[test]
    fn the_receive_time_has_an_offset() {
        let (ts, year) = receive_time();
        assert!(year >= 2026);
        assert_eq!(ts.len(), 29, "{ts}");
    }
}
