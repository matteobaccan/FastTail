// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Log level detection: one fixed table of level tokens, matched as whole words in the
//! first bytes of a line, plus syslog `<n>` priorities. No allocation, no regex.

use serde::{Deserialize, Serialize};

/// Bytes of a line examined for a level token by default (the header, not the message
/// body); the `level_detect_bytes` setting and a stream's `level_bytes` change it.
pub const DEFAULT_WINDOW: usize = 96;
/// The range of the window: below 16 not even `<n>` and a short date fit, above 4 KB the
/// first token would too often be a word of the message.
pub const MIN_WINDOW: usize = 16;
pub const MAX_WINDOW: usize = 4096;

/// `bytes` as a level window: in range, else `None`.
pub fn valid_window(bytes: usize) -> Option<usize> {
    (MIN_WINDOW..=MAX_WINDOW).contains(&bytes).then_some(bytes)
}

/// Severity of a log line, ordered from the least to the most severe. `Unknown` is used
/// both for lines without a recognised token and as the "off" value of the level filter.
#[repr(u8)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum LogLevel {
    #[default]
    Unknown = 0,
    Trace = 1,
    Debug = 2,
    Info = 3,
    Warn = 4,
    Error = 5,
    Fatal = 6,
}

impl LogLevel {
    /// Every real level, least severe first.
    pub const ALL: [LogLevel; 6] = [
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
        LogLevel::Fatal,
    ];

    /// Number of values including `Unknown` (size of a per-level counter array).
    pub const COUNT: usize = 7;

    pub fn name(self) -> &'static str {
        match self {
            LogLevel::Unknown => "",
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Fatal => "FATAL",
        }
    }

    /// Three-letter tag for compact counters.
    pub fn short(self) -> &'static str {
        match self {
            LogLevel::Unknown => "",
            LogLevel::Trace => "TRC",
            LogLevel::Debug => "DBG",
            LogLevel::Info => "INF",
            LogLevel::Warn => "WRN",
            LogLevel::Error => "ERR",
            LogLevel::Fatal => "FTL",
        }
    }

    pub fn from_u8(v: u8) -> LogLevel {
        match v {
            1 => LogLevel::Trace,
            2 => LogLevel::Debug,
            3 => LogLevel::Info,
            4 => LogLevel::Warn,
            5 => LogLevel::Error,
            6 => LogLevel::Fatal,
            _ => LogLevel::Unknown,
        }
    }

    /// Parses a level name (any case); anything else is `Unknown`.
    pub fn parse(s: &str) -> LogLevel {
        token_level(s.trim().as_bytes()).unwrap_or(LogLevel::Unknown)
    }
}

/// Level a whole word names (`error`, `WARNING`, `info`...), as the text detection reads
/// it; `None` for any other word. Used for the value of a level field.
pub fn level_word(word: &str) -> Option<LogLevel> {
    token_level(word.trim().as_bytes())
}

/// Level of a token (a whole word already isolated), or `None` if it is not a level word.
fn token_level(word: &[u8]) -> Option<LogLevel> {
    // Branch on length first to eliminate array scans and limit string comparisons.
    // Length dispatch turns level matching into an O(1) lookup per scanned word.
    match word.len() {
        4 => {
            if word.eq_ignore_ascii_case(b"WARN") {
                Some(LogLevel::Warn)
            } else if word.eq_ignore_ascii_case(b"INFO") {
                Some(LogLevel::Info)
            } else {
                None
            }
        }
        5 => {
            if word.eq_ignore_ascii_case(b"ERROR") {
                Some(LogLevel::Error)
            } else if word.eq_ignore_ascii_case(b"DEBUG") {
                Some(LogLevel::Debug)
            } else if word.eq_ignore_ascii_case(b"TRACE") {
                Some(LogLevel::Trace)
            } else if word.eq_ignore_ascii_case(b"FATAL") {
                Some(LogLevel::Fatal)
            } else {
                None
            }
        }
        6 => {
            if word.eq_ignore_ascii_case(b"NOTICE") {
                Some(LogLevel::Info)
            } else if word.eq_ignore_ascii_case(b"SEVERE") {
                Some(LogLevel::Error)
            } else {
                None
            }
        }
        7 => {
            if word.eq_ignore_ascii_case(b"WARNING") {
                Some(LogLevel::Warn)
            } else if word.eq_ignore_ascii_case(b"VERBOSE") {
                Some(LogLevel::Trace)
            } else {
                None
            }
        }
        8 => {
            if word.eq_ignore_ascii_case(b"CRITICAL") {
                Some(LogLevel::Fatal)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Word characters: a level token must not be glued to letters, digits, `_` or non-ASCII
/// bytes (`ERRORS`, `INFO2`, `error_count` are not levels).
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Syslog priority `<n>` at the very start of the line: the severity is `n & 7`.
fn syslog_level(bytes: &[u8]) -> Option<LogLevel> {
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let mut n: u32 = 0;
    let mut digits = 0usize;
    for &b in &bytes[1..] {
        if b.is_ascii_digit() {
            if digits == 3 {
                return None;
            }
            n = n * 10 + (b - b'0') as u32;
            digits += 1;
        } else if b == b'>' && digits > 0 {
            return Some(match n & 7 {
                0..=2 => LogLevel::Fatal,
                3 => LogLevel::Error,
                4 => LogLevel::Warn,
                5 | 6 => LogLevel::Info,
                _ => LogLevel::Debug,
            });
        } else {
            return None;
        }
    }
    None
}

/// [`detect_level_in`] with the default window.
pub fn detect_level(line: &str) -> LogLevel {
    detect_level_in(line, DEFAULT_WINDOW)
}

/// Detects the level of a line: the first level token, as a whole word, in the first
/// `window` bytes (case-insensitive; brackets, colons, `=` or quotes around the token are
/// fine), or a syslog `<n>` priority at the start of the line.
pub fn detect_level_in(line: &str, window: usize) -> LogLevel {
    let bytes = line.as_bytes();
    if let Some(level) = syslog_level(bytes) {
        return level;
    }
    let window = &bytes[..bytes.len().min(window)];
    let mut i = 0;
    while i < window.len() {
        if !is_word_byte(window[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < window.len() && is_word_byte(window[i]) {
            i += 1;
        }
        // A word cut by the window is not a whole word.
        if i == window.len() && bytes.len() > window.len() && is_word_byte(bytes[i]) {
            break;
        }
        if let Some(level) = token_level(&window[start..i]) {
            return level;
        }
    }
    LogLevel::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_layouts() {
        let cases: [(&str, LogLevel); 16] = [
            ("2026-09-18 12:00:00 [ERROR] boom", LogLevel::Error),
            ("WARN  main - slow", LogLevel::Warn),
            ("<3>kernel: oops", LogLevel::Error),
            (
                "<165>1 2026-09-18T12:00:00Z host app - - - notice",
                LogLevel::Info,
            ),
            ("<0>panic", LogLevel::Fatal),
            ("<7>debugging", LogLevel::Debug),
            (
                "12:00:00.123 INFO  [main] c.e.App - started",
                LogLevel::Info,
            ),
            (
                "2026-09-18 12:00:00,000 WARNING:root:disk almost full",
                LogLevel::Warn,
            ),
            ("[2026-09-18 12:00:00 FTL] started", LogLevel::Unknown),
            ("fatal: not a git repository", LogLevel::Fatal),
            ("time=\"2026-09-18\" level=error msg=\"x\"", LogLevel::Error),
            ("{\"level\":\"debug\",\"msg\":\"x\"}", LogLevel::Debug),
            ("2026/09/18 12:00:00 [notice] 1#1: nginx", LogLevel::Info),
            (
                "Sep 18 12:00:00 host sshd[1]: CRITICAL failure",
                LogLevel::Fatal,
            ),
            ("V/Tag: android line", LogLevel::Unknown),
            ("TRACE  span=1", LogLevel::Trace),
        ];
        for (line, expected) in cases {
            assert_eq!(detect_level(line), expected, "line: {line}");
        }
    }

    #[test]
    fn whole_words_only_and_body_ignored() {
        assert_eq!(
            detect_level("INFO user typed \"error\" in the search box"),
            LogLevel::Info
        );
        assert_eq!(detect_level("ERRORS: 0"), LogLevel::Unknown);
        assert_eq!(detect_level("error_count=3"), LogLevel::Unknown);
        assert_eq!(detect_level("INFO2 x"), LogLevel::Unknown);
        assert_eq!(detect_level("Ínfo"), LogLevel::Unknown);
        assert_eq!(detect_level(""), LogLevel::Unknown);
        assert_eq!(
            detect_level("    at com.example.Foo(Foo.java:1)"),
            LogLevel::Unknown
        );
    }

    #[test]
    fn only_the_first_window_is_examined() {
        for window in [DEFAULT_WINDOW, MIN_WINDOW, 256, MAX_WINDOW] {
            let long = format!("{} ERROR late", "x".repeat(window));
            assert_eq!(
                detect_level_in(&long, window),
                LogLevel::Unknown,
                "{window}"
            );
            let cut = format!("{} ERROR", "x".repeat(window - 4));
            assert_eq!(
                detect_level_in(&cut, window),
                LogLevel::Unknown,
                "token cut by the window ({window})"
            );
            let fits = format!("{} ERROR", "x".repeat(window - 6));
            assert_eq!(detect_level_in(&fits, window), LogLevel::Error, "{window}");
        }
        // The default is today's 96 bytes; a wider window finds the late token.
        let late = format!("{} ERROR boom", "x".repeat(150));
        assert_eq!(detect_level(&late), LogLevel::Unknown);
        assert_eq!(detect_level_in(&late, 256), LogLevel::Error);
        assert_eq!(valid_window(8), None);
        assert_eq!(valid_window(16), Some(16));
        assert_eq!(valid_window(4097), None);
    }

    #[test]
    fn syslog_priority_is_strict() {
        assert_eq!(detect_level("<>x"), LogLevel::Unknown);
        assert_eq!(detect_level("<1234>x"), LogLevel::Unknown);
        assert_eq!(detect_level("<a>x"), LogLevel::Unknown);
        assert_eq!(detect_level("<3 ERROR"), LogLevel::Error);
    }

    #[test]
    fn parse_and_names_round_trip() {
        for level in LogLevel::ALL {
            assert_eq!(LogLevel::parse(level.name()), level);
            assert_eq!(LogLevel::from_u8(level as u8), level);
        }
        assert_eq!(LogLevel::parse("warning"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("off"), LogLevel::Unknown);
        assert!(LogLevel::Fatal > LogLevel::Error && LogLevel::Trace > LogLevel::Unknown);
    }
}
