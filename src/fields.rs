// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Fields of structured log lines: JSON objects, logfmt `key=value` pairs and the named
//! groups of a regular expression (with the Apache and syslog presets).
//!
//! The scanners borrow the line: `scan` fills a reused `FieldSpans` with byte ranges into
//! it (keys of nested JSON objects, regex group names and `_prefix` are written to a
//! scratch buffer of the same `FieldSpans`), so once warmed up a line costs no
//! allocation. Values are unescaped only when shown or compared (`unescape`).
//! Nothing is kept per line: the caller scans the rows it draws or filters.

use regex::{CaptureLocations, Regex};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Nested JSON objects are flattened as `a.b.c` up to this many key segments; a deeper
/// object, like an array, is one raw value.
pub const JSON_MAX_DEPTH: usize = 3;
/// Fields kept per line; the rest of a wider line is not scanned (the line is partial).
pub const MAX_LINE_FIELDS: usize = 128;
/// Detection samples at most this many non-continuation lines...
pub const DETECT_LINES: usize = 200;
/// ...and at most this many bytes of them.
pub const DETECT_BYTES: usize = 256 * 1024;
/// Share of the sampled lines, in percent, that must parse for a parser to be chosen.
pub const DETECT_PERCENT: usize = 80;
/// Pairs a logfmt line needs to count towards logfmt detection.
pub const DETECT_LOGFMT_PAIRS: usize = 3;
/// Key of the logfmt text before the first pair (a timestamp and level, typically).
pub const PREFIX_KEY: &str = "_prefix";

/// Apache / NGINX combined (and common) log format.
pub const APACHE_PATTERN: &str = r#"^(?P<host>\S+) (?P<ident>\S+) (?P<user>\S+) \[(?P<time>[^\]]+)\] "(?:(?P<method>[A-Z]+) (?P<path>\S+)(?: (?P<protocol>[^"]*))?|[^"]*)" (?P<status>\d{3}) (?P<size>\S+)(?: "(?P<referer>[^"]*)" "(?P<agent>[^"]*)")?"#;
/// BSD syslog (RFC 3164): `<pri>Mmm dd hh:mm:ss host app[pid]: message`.
pub const SYSLOG_PATTERN: &str = r"^(?:<(?P<pri>\d{1,3})>)?(?P<time>[A-Z][a-z]{2} [ \d]\d \d{2}:\d{2}:\d{2}) (?P<host>\S+) (?P<app>[^\s:\[]+)(?:\[(?P<pid>\d+)\])?: ?(?P<msg>.*)$";

/// A regular expression whose named groups are the fields of a line.
#[derive(Debug)]
pub struct RegexFields {
    regex: Regex,
    /// `(group index, name)` of the named groups, in pattern order.
    names: Vec<(usize, Box<str>)>,
}

impl RegexFields {
    /// Compiles `pattern`; a pattern without named groups is an error too, as it would
    /// give no field.
    pub fn new(pattern: &str) -> Result<Self, String> {
        let regex = Regex::new(pattern).map_err(|e| e.to_string())?;
        let names: Vec<(usize, Box<str>)> = regex
            .capture_names()
            .enumerate()
            .filter_map(|(i, n)| n.map(|n| (i, n.into())))
            .collect();
        if names.is_empty() {
            return Err("no named group (?P<name>...)".to_string());
        }
        Ok(Self { regex, names })
    }

    pub fn pattern(&self) -> &str {
        self.regex.as_str()
    }
}

/// How the fields of a stream's lines are read.
#[derive(Debug)]
pub enum FieldParser {
    Json,
    Logfmt,
    Regex(RegexFields),
}

/// The parser chosen for a stream: automatic detection, none, or one of the parsers.
/// Saved as `fields_parser=` (absent is `Auto`) and `fields_regex=` for `Regex`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ParserChoice {
    #[default]
    Auto,
    Off,
    Json,
    Logfmt,
    Regex(String),
    Apache,
    Syslog,
}

impl ParserChoice {
    /// The name saved in `fields_parser=`.
    pub fn name(&self) -> &'static str {
        match self {
            ParserChoice::Auto => "auto",
            ParserChoice::Off => "off",
            ParserChoice::Json => "json",
            ParserChoice::Logfmt => "logfmt",
            ParserChoice::Regex(_) => "regex",
            ParserChoice::Apache => "apache",
            ParserChoice::Syslog => "syslog",
        }
    }

    /// Reads `fields_parser=` and, for `regex`, `fields_regex=`; unknown names are `None`.
    pub fn from_name(name: &str, regex: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "auto" => ParserChoice::Auto,
            "off" => ParserChoice::Off,
            "json" => ParserChoice::Json,
            "logfmt" => ParserChoice::Logfmt,
            "regex" => ParserChoice::Regex(regex.to_string()),
            "apache" => ParserChoice::Apache,
            "syslog" => ParserChoice::Syslog,
            _ => return None,
        })
    }

    /// The parser of a forced choice: `Ok(None)` for `Auto` and `Off`, `Err` for a regex
    /// that does not compile or has no named group.
    pub fn build(&self) -> Result<Option<Arc<FieldParser>>, String> {
        let regex = |p: &str| RegexFields::new(p).map(|r| Some(Arc::new(FieldParser::Regex(r))));
        match self {
            ParserChoice::Auto | ParserChoice::Off => Ok(None),
            ParserChoice::Json => Ok(Some(Arc::new(FieldParser::Json))),
            ParserChoice::Logfmt => Ok(Some(Arc::new(FieldParser::Logfmt))),
            ParserChoice::Regex(p) => regex(p),
            ParserChoice::Apache => regex(APACHE_PATTERN),
            ParserChoice::Syslog => regex(SYSLOG_PATTERN),
        }
    }
}

/// Where a key is: a range of the line, or of the scratch buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAt {
    Line(u32, u32),
    Scratch(u32, u32),
}

/// One field of a line: its key and its value as a byte range of the line (for a quoted
/// value, inside the quotes and still escaped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldSpan {
    key: KeyAt,
    value: (u32, u32),
    quoted: bool,
}

/// The fields of one line, reused from line to line.
#[derive(Debug, Default)]
pub struct FieldSpans {
    spans: SmallVec<[FieldSpan; 32]>,
    /// Keys that are not a slice of the line; only UTF-8 is ever appended.
    scratch: Vec<u8>,
    /// Capture slots of the regex parser, and the regex they belong to.
    locs: Option<(usize, CaptureLocations)>,
    /// The line did not scan to its end (JSON syntax error, too many fields, unclosed
    /// quote): the fields found before are kept.
    pub partial: bool,
    /// The line was cut only because it has more than `MAX_LINE_FIELDS` fields (then
    /// `partial` is set too): it still has the parser's shape.
    pub truncated: bool,
    /// `key=` pairs of the last logfmt scan (bare keys and the prefix not counted).
    logfmt_pairs: usize,
    /// Byte range of the whole match of the last regex scan.
    matched: Option<(usize, usize)>,
}

/// A field of a scanned line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field<'a> {
    pub key: &'a str,
    /// The value as written (inside the quotes when quoted, escapes not decoded).
    pub raw: &'a str,
    /// Byte range of `raw` in the line.
    pub range: (usize, usize),
    pub quoted: bool,
}

impl Field<'_> {
    /// The value with its escapes decoded.
    pub fn value(&self) -> Cow<'_, str> {
        if self.quoted {
            unescape(self.raw)
        } else {
            Cow::Borrowed(self.raw)
        }
    }
}

impl FieldSpans {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.spans.clear();
        self.scratch.clear();
        self.partial = false;
        self.truncated = false;
        self.logfmt_pairs = 0;
        self.matched = None;
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn scratch_str(&self, from: u32, to: u32) -> &str {
        std::str::from_utf8(&self.scratch[from as usize..to as usize]).unwrap_or("")
    }

    fn field<'a>(&'a self, line: &'a str, span: &FieldSpan) -> Field<'a> {
        let key = match span.key {
            KeyAt::Line(a, b) => line.get(a as usize..b as usize).unwrap_or(""),
            KeyAt::Scratch(a, b) => self.scratch_str(a, b),
        };
        let range = (span.value.0 as usize, span.value.1 as usize);
        Field {
            key,
            raw: line.get(range.0..range.1).unwrap_or(""),
            range,
            quoted: span.quoted,
        }
    }

    /// The fields of `line`, which must be the line last scanned into `self`.
    pub fn iter<'a>(&'a self, line: &'a str) -> impl Iterator<Item = Field<'a>> + 'a {
        self.spans.iter().map(move |s| self.field(line, s))
    }

    /// Checks whether `span`'s key matches `key` without slicing the value or constructing `Field`.
    /// Slices key bytes/str directly, short-circuiting on length mismatch.
    fn span_key_matches(&self, line: &str, span: &FieldSpan, key: &str) -> bool {
        match span.key {
            KeyAt::Line(a, b) => line.get(a as usize..b as usize) == Some(key),
            KeyAt::Scratch(a, b) => {
                self.scratch.get(a as usize..b as usize) == Some(key.as_bytes())
            }
        }
    }

    /// The first field named `key`.
    pub fn get<'a>(&'a self, line: &'a str, key: &str) -> Option<Field<'a>> {
        self.spans
            .iter()
            .find(|span| self.span_key_matches(line, span, key))
            .map(|span| self.field(line, span))
    }

    fn push(&mut self, key: KeyAt, value: (usize, usize), quoted: bool) -> bool {
        if self.spans.len() >= MAX_LINE_FIELDS {
            self.partial = true;
            self.truncated = true;
            return false;
        }
        self.spans.push(FieldSpan {
            key,
            value: (value.0 as u32, value.1 as u32),
            quoted,
        });
        true
    }

    /// Appends `text` to the scratch buffer and returns its range there.
    fn scratch_key(&mut self, text: &[u8]) -> (u32, u32) {
        let from = self.scratch.len();
        self.scratch.extend_from_slice(text);
        (from as u32, self.scratch.len() as u32)
    }
}

/// Scans `line` with `parser` into `out` (cleared first). Returns true when the line has
/// the parser's shape (a JSON object, at least one logfmt pair, a regex match), even if
/// partial. Lines longer than 4 GB are not scanned.
pub fn scan(parser: &FieldParser, line: &str, out: &mut FieldSpans) -> bool {
    out.clear();
    if line.len() > u32::MAX as usize {
        return false;
    }
    match parser {
        FieldParser::Json => scan_json(line, out),
        FieldParser::Logfmt => scan_logfmt(line, out),
        FieldParser::Regex(r) => scan_regex(r, line, out),
    }
}

/// Decodes the escapes of a quoted value (JSON's `\" \\ \/ \b \f \n \r \t \uXXXX`,
/// surrogate pairs included; logfmt's `\"` and `\\` are a subset). Unknown escapes keep
/// the escaped character.
pub fn unescape(raw: &str) -> Cow<'_, str> {
    if !raw.contains('\\') {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut from = 0;
    let b = raw.as_bytes();
    while from < b.len() {
        match memchr::memchr(b'\\', &b[from..]) {
            Some(rel) => {
                let pos = from + rel;
                out.push_str(&raw[from..pos]);
                let mut chars = raw[pos..].chars();
                chars.next(); // skip '\\'
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('u') => {
                        let hex = |chars: &mut std::str::Chars| -> Option<u32> {
                            let s: String = chars.clone().take(4).collect();
                            if s.len() != 4 {
                                return None;
                            }
                            let v = u32::from_str_radix(&s, 16).ok()?;
                            for _ in 0..4 {
                                chars.next();
                            }
                            Some(v)
                        };
                        let Some(hi) = hex(&mut chars) else {
                            out.push_str("\\u");
                            from = raw.len() - chars.as_str().len();
                            continue;
                        };
                        let code = if (0xD800..0xDC00).contains(&hi) {
                            let mut look = chars.clone();
                            let lo = (look.next() == Some('\\') && look.next() == Some('u'))
                                .then(|| hex(&mut look))
                                .flatten()
                                .filter(|lo| (0xDC00..0xE000).contains(lo));
                            match lo {
                                Some(lo) => {
                                    chars = look;
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                }
                                None => 0xFFFD,
                            }
                        } else {
                            hi
                        };
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    Some(other) => out.push(other),
                    None => out.push('\\'),
                }
                from = raw.len() - chars.as_str().len();
            }
            None => {
                out.push_str(&raw[from..]);
                break;
            }
        }
    }
    Cow::Owned(out)
}

fn skip_ws(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && matches!(b[pos], b' ' | b'\t' | b'\r' | b'\n') {
        pos += 1;
    }
    pos
}

/// Byte offset of the `{` a JSON line starts with: after optional whitespace, or after a
/// leading timestamp (`2024-05-01T10:00:00Z {"level":...}`).
pub fn json_start(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let pos = skip_ws(b, 0);
    if b.get(pos) == Some(&b'{') {
        return Some(pos);
    }
    let after = crate::timestamp::leading_span(line, crate::timestamp::FormatHint::Unknown)?;
    let pos = skip_ws(b, after);
    (b.get(pos) == Some(&b'{')).then_some(pos)
}

fn scan_json(line: &str, out: &mut FieldSpans) -> bool {
    let Some(start) = json_start(line) else {
        return false;
    };
    let b = line.as_bytes();
    let mut pos = start;
    if json_object(b, &mut pos, 1, None, out).is_err() {
        out.partial = true;
    }
    true
}

/// End of the string whose content starts at `pos` (just after the opening quote): the
/// offset of the closing quote.
fn json_string_end(b: &[u8], mut pos: usize) -> Result<usize, ()> {
    loop {
        match memchr::memchr2(b'"', b'\\', &b[pos.min(b.len())..]) {
            Some(i) => {
                pos += i;
                if b[pos] == b'"' {
                    return Ok(pos);
                }
                pos += 2;
            }
            None => return Err(()),
        }
    }
}

/// Skips a nested object or array (the value at `pos`), strings included; returns its end.
fn json_skip_nested(b: &[u8], mut pos: usize) -> Result<usize, ()> {
    let mut depth = 0usize;
    while pos < b.len() {
        match b[pos] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(pos + 1);
                }
            }
            b'"' => pos = json_string_end(b, pos + 1)?,
            _ => {}
        }
        pos += 1;
    }
    Err(())
}

/// Scans the object at `pos` (on its `{`), pushing its leaves; `prefix` is the scratch
/// range of the key path of a nested object.
fn json_object(
    b: &[u8],
    pos: &mut usize,
    depth: usize,
    prefix: Option<(u32, u32)>,
    out: &mut FieldSpans,
) -> Result<(), ()> {
    if b.get(*pos) != Some(&b'{') {
        return Err(());
    }
    *pos = skip_ws(b, *pos + 1);
    if b.get(*pos) == Some(&b'}') {
        *pos += 1;
        return Ok(());
    }
    loop {
        *pos = skip_ws(b, *pos);
        if b.get(*pos) != Some(&b'"') {
            return Err(());
        }
        let key_from = *pos + 1;
        let key_to = json_string_end(b, key_from)?;
        *pos = skip_ws(b, key_to + 1);
        if b.get(*pos) != Some(&b':') {
            return Err(());
        }
        *pos = skip_ws(b, *pos + 1);
        let key = match prefix {
            None => KeyAt::Line(key_from as u32, key_to as u32),
            Some((a, z)) => {
                let from = out.scratch.len();
                out.scratch.extend_from_within(a as usize..z as usize);
                out.scratch.push(b'.');
                out.scratch.extend_from_slice(&b[key_from..key_to]);
                KeyAt::Scratch(from as u32, out.scratch.len() as u32)
            }
        };
        match b.get(*pos) {
            Some(b'{') if depth < JSON_MAX_DEPTH => {
                let path = match key {
                    KeyAt::Line(a, z) => out.scratch_key(&b[a as usize..z as usize]),
                    KeyAt::Scratch(a, z) => (a, z),
                };
                json_object(b, pos, depth + 1, Some(path), out)?;
            }
            Some(b'{') | Some(b'[') => {
                let from = *pos;
                *pos = json_skip_nested(b, *pos)?;
                if !out.push(key, (from, *pos), false) {
                    return Err(());
                }
            }
            Some(b'"') => {
                let from = *pos + 1;
                let to = json_string_end(b, from)?;
                *pos = to + 1;
                if !out.push(key, (from, to), true) {
                    return Err(());
                }
            }
            Some(_) => {
                let from = *pos;
                while *pos < b.len()
                    && !matches!(b[*pos], b',' | b'}' | b' ' | b'\t' | b'\r' | b'\n')
                {
                    *pos += 1;
                }
                if *pos == from {
                    return Err(());
                }
                if !out.push(key, (from, *pos), false) {
                    return Err(());
                }
            }
            None => return Err(()),
        }
        *pos = skip_ws(b, *pos);
        match b.get(*pos) {
            Some(b',') => *pos += 1,
            Some(b'}') => {
                *pos += 1;
                return Ok(());
            }
            _ => return Err(()),
        }
    }
}

fn is_key_byte(c: u8) -> bool {
    !matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'=' | b'"')
}

/// Length of the `key=` at `pos`, or `None` when the word there is not a pair.
fn logfmt_pair_key(b: &[u8], pos: usize) -> Option<usize> {
    let mut end = pos;
    while end < b.len() && is_key_byte(b[end]) {
        end += 1;
    }
    (end > pos && b.get(end) == Some(&b'=')).then_some(end - pos)
}

fn scan_logfmt(line: &str, out: &mut FieldSpans) -> bool {
    let b = line.as_bytes();
    if memchr::memchr(b'=', b).is_none() {
        return false;
    }
    // The first pair: text before it is the prefix.
    let mut pos = skip_ws(b, 0);
    let mut first = None;
    while pos < b.len() {
        if logfmt_pair_key(b, pos).is_some() {
            first = Some(pos);
            break;
        }
        while pos < b.len() && !matches!(b[pos], b' ' | b'\t') {
            pos += 1;
        }
        pos = skip_ws(b, pos);
    }
    let Some(first) = first else {
        return false;
    };
    let lead = skip_ws(b, 0);
    let mut prefix_end = first;
    while prefix_end > lead && matches!(b[prefix_end - 1], b' ' | b'\t') {
        prefix_end -= 1;
    }
    if prefix_end > lead {
        let (a, z) = out.scratch_key(PREFIX_KEY.as_bytes());
        out.push(KeyAt::Scratch(a, z), (lead, prefix_end), false);
    }
    let mut pos = first;
    while pos < b.len() {
        let key_from = pos;
        while pos < b.len() && is_key_byte(b[pos]) {
            pos += 1;
        }
        let key = KeyAt::Line(key_from as u32, pos as u32);
        if pos == key_from {
            // A stray `=` or quote: skip the word.
            while pos < b.len() && !matches!(b[pos], b' ' | b'\t') {
                pos += 1;
            }
            out.partial = true;
        } else if b.get(pos) == Some(&b'=') {
            pos += 1;
            out.logfmt_pairs += 1;
            if b.get(pos) == Some(&b'"') {
                let from = pos + 1;
                match json_string_end(b, from) {
                    Ok(to) => {
                        pos = to + 1;
                        if !out.push(key, (from, to), true) {
                            return true;
                        }
                    }
                    Err(()) => {
                        out.partial = true;
                        out.push(key, (from, b.len()), true);
                        return true;
                    }
                }
            } else {
                let from = pos;
                while pos < b.len() && !matches!(b[pos], b' ' | b'\t' | b'\r' | b'\n') {
                    pos += 1;
                }
                if !out.push(key, (from, pos), false) {
                    return true;
                }
            }
        } else if !out.push(key, (pos, pos), false) {
            // A bare key: an empty value.
            return true;
        }
        pos = skip_ws(b, pos);
    }
    true
}

fn scan_regex(r: &RegexFields, line: &str, out: &mut FieldSpans) -> bool {
    let id = r as *const RegexFields as usize;
    // The address alone could be reused by a later parser: the slot count must match too.
    let mut locs = match out.locs.take() {
        Some((owner, locs)) if owner == id && locs.len() == r.regex.captures_len() => locs,
        _ => r.regex.capture_locations(),
    };
    let matched = r.regex.captures_read(&mut locs, line).is_some();
    if matched {
        out.matched = locs.get(0);
        for (group, name) in &r.names {
            if let Some((a, z)) = locs.get(*group) {
                let (ka, kz) = out.scratch_key(name.as_bytes());
                out.push(KeyAt::Scratch(ka, kz), (a, z), false);
            }
        }
    }
    out.locs = Some((id, locs));
    matched
}

/// Keys whose value is a line's message: shown in the last column of the column view.
pub const MESSAGE_KEYS: [&str; 4] = ["msg", "message", "@message", "MESSAGE"];
/// Keys a stream's field catalogue lists at most ("more fields not listed" after).
pub const MAX_CATALOGUE: usize = 256;
/// Columns shown by default: the first keys of the catalogue, message keys aside.
pub const DEFAULT_COLUMNS: usize = 8;
/// Rows whose fields the column view keeps parsed.
pub const ROW_CACHE: usize = 1024;
/// Width of a column in character cells.
pub const MIN_WIDTH: u16 = 3;
pub const MAX_WIDTH: u16 = 200;
/// Largest width suggested from the values seen.
const SUGGESTED_MAX_WIDTH: usize = 40;

/// The fields of one line as the column view draws them: decoded, on one line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowFields {
    /// The line has the parser's shape (a JSON object, logfmt pairs, a regex match).
    pub shaped: bool,
    /// It did not scan to its end.
    pub partial: bool,
    pub fields: Vec<(String, String)>,
    /// Regex parsers: the text of the line outside the match.
    pub outside: String,
}

impl RowFields {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The last column when `shown` are the other columns: the message field, then the
    /// fields not shown as `key=value`, then the text outside a regex match.
    pub fn message(&self, shown: &[String]) -> String {
        let is_shown = |key: &str| shown.iter().any(|s| s == key);
        let mut out = String::new();
        let mut used = None;
        for key in MESSAGE_KEYS {
            if !is_shown(key) {
                if let Some(value) = self.get(key) {
                    out.push_str(value);
                    used = Some(key);
                    break;
                }
            }
        }
        for (key, value) in &self.fields {
            if is_shown(key) || Some(key.as_str()) == used {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(key);
            out.push('=');
            out.push_str(value);
        }
        if !self.outside.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&self.outside);
        }
        out
    }
}

/// A value on one line: line breaks and tabs become spaces.
fn one_line(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Scans `line` into owned, decoded `RowFields` (`spans` is the reused scratch).
pub fn row_fields(parser: &FieldParser, line: &str, spans: &mut FieldSpans) -> RowFields {
    let shaped = scan(parser, line, spans);
    let fields = spans
        .iter(line)
        .map(|f| (f.key.to_string(), one_line(&f.value())))
        .collect();
    let outside = match spans.matched {
        Some((a, z)) if shaped => {
            let before = line[..a].trim();
            let after = line[z..].trim();
            one_line(&format!(
                "{before}{}{after}",
                if before.is_empty() || after.is_empty() {
                    ""
                } else {
                    " "
                }
            ))
        }
        _ => String::new(),
    };
    RowFields {
        shaped,
        partial: spans.partial,
        fields,
        outside,
    }
}

/// The keys seen in a stream's lines, in the order first seen, with a width suggested
/// from the values of the sample.
#[derive(Debug, Clone, Default)]
pub struct FieldCatalogue {
    keys: Vec<String>,
    widths: HashMap<String, u16>,
    /// Keys past `MAX_CATALOGUE` were seen and not listed.
    pub more: bool,
}

impl FieldCatalogue {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn contains(&self, key: &str) -> bool {
        self.widths.contains_key(key)
    }

    /// Records `key` with a value `value_chars` long. `grow` widens the suggestion of a
    /// known key (the sample); rows drawn later only add keys, so columns do not change
    /// width while scrolling.
    pub fn note(&mut self, key: &str, value_chars: usize, grow: bool) {
        let fit = value_chars
            .max(key.chars().count())
            .clamp(MIN_WIDTH as usize, SUGGESTED_MAX_WIDTH) as u16;
        if let Some(width) = self.widths.get_mut(key) {
            if grow {
                *width = (*width).max(fit);
            }
        } else if self.keys.len() < MAX_CATALOGUE {
            self.keys.push(key.to_string());
            self.widths.insert(key.to_string(), fit);
        } else {
            self.more = true;
        }
    }

    /// Width suggested for `key`, in character cells.
    pub fn suggested_width(&self, key: &str) -> u16 {
        self.widths
            .get(key)
            .copied()
            .unwrap_or_else(|| (key.chars().count() as u16).clamp(MIN_WIDTH, 12))
    }

    /// The default columns: the first `DEFAULT_COLUMNS` keys, message keys aside.
    pub fn default_columns(&self) -> Vec<String> {
        self.keys
            .iter()
            .filter(|k| !MESSAGE_KEYS.contains(&k.as_str()))
            .take(DEFAULT_COLUMNS)
            .cloned()
            .collect()
    }
}

/// Parsed fields of the rows drawn last, at most `ROW_CACHE`, all dropped when `key`
/// (the generations of the text and of the parser) changes. The oldest row goes first.
#[derive(Debug, Default)]
pub struct RowCache {
    key: (u64, u64, u64),
    map: HashMap<usize, Arc<RowFields>>,
    order: VecDeque<usize>,
}

impl RowCache {
    pub fn get(&mut self, key: (u64, u64, u64), line: usize) -> Option<Arc<RowFields>> {
        if self.key != key {
            self.clear();
            self.key = key;
            return None;
        }
        self.map.get(&line).cloned()
    }

    pub fn put(&mut self, line: usize, fields: Arc<RowFields>) {
        if self.map.insert(line, fields).is_none() {
            self.order.push_back(line);
        }
        while self.order.len() > ROW_CACHE {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

/// Chooses a parser from the first lines of a stream (continuation lines already left
/// out): JSON when at least 80 % of the non-empty lines are JSON objects that scan to
/// their end, logfmt when at least 80 % have three or more `key=value` pairs, else none.
/// The regex presets are never detected.
pub fn detect<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<ParserChoice> {
    let mut spans = FieldSpans::new();
    let (mut total, mut json, mut logfmt) = (0usize, 0usize, 0usize);
    let mut bytes = 0usize;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        bytes += line.len();
        if scan(&FieldParser::Json, line, &mut spans)
            && (!spans.partial || spans.truncated)
            && !spans.is_empty()
        {
            json += 1;
        } else if scan(&FieldParser::Logfmt, line, &mut spans)
            && spans.logfmt_pairs >= DETECT_LOGFMT_PAIRS
        {
            logfmt += 1;
        }
        if total >= DETECT_LINES || bytes >= DETECT_BYTES {
            break;
        }
    }
    if total == 0 {
        return None;
    }
    if json * 100 >= total * DETECT_PERCENT {
        Some(ParserChoice::Json)
    } else if logfmt * 100 >= total * DETECT_PERCENT {
        Some(ParserChoice::Logfmt)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(parser: &FieldParser, line: &str) -> (Vec<(String, String)>, bool) {
        let mut spans = FieldSpans::new();
        scan(parser, line, &mut spans);
        (
            spans
                .iter(line)
                .map(|f| (f.key.to_string(), f.value().into_owned()))
                .collect(),
            spans.partial,
        )
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn json_flat_nested_arrays_and_escapes() {
        let line = r#"{"ts":"2024-05-01T10:00:00Z","level":"error","n":42,"ok":true,"x":null,"http":{"status":503,"req":{"method":"GET","h":{"a":1}}},"tags":["a","b"],"msg":"say \"hi\"\n\u00e8"}"#;
        let (got, partial) = fields(&FieldParser::Json, line);
        assert!(!partial);
        assert_eq!(
            got,
            pairs(&[
                ("ts", "2024-05-01T10:00:00Z"),
                ("level", "error"),
                ("n", "42"),
                ("ok", "true"),
                ("x", "null"),
                ("http.status", "503"),
                ("http.req.method", "GET"),
                ("http.req.h", r#"{"a":1}"#),
                ("tags", r#"["a","b"]"#),
                ("msg", "say \"hi\"\nè"),
            ])
        );
    }

    #[test]
    fn json_after_a_leading_timestamp_and_whitespace() {
        let (got, _) = fields(
            &FieldParser::Json,
            r#"2024-05-01 10:00:00 {"a": 1 , "b" : "x" }"#,
        );
        assert_eq!(got, pairs(&[("a", "1"), ("b", "x")]));
        let (got, _) = fields(&FieldParser::Json, "   {}");
        assert!(got.is_empty());
    }

    #[test]
    fn broken_json_keeps_the_fields_before_the_error() {
        let (got, partial) = fields(&FieldParser::Json, r#"{"a":1,"b":"x","c":"#);
        assert!(partial);
        assert_eq!(got, pairs(&[("a", "1"), ("b", "x")]));
        let mut spans = FieldSpans::new();
        assert!(!scan(&FieldParser::Json, "plain text", &mut spans));
        let (got, partial) = fields(&FieldParser::Json, r#"{"a":"unclosed"#);
        assert!(partial && got.is_empty());
    }

    #[test]
    fn surrogate_pairs_and_unknown_escapes() {
        assert_eq!(unescape(r"\ud83d\ude00 \q \u12"), "😀 q \\u12");
        assert_eq!(unescape(r"\ud83d!"), "\u{FFFD}!");
        assert!(matches!(unescape("plain"), Cow::Borrowed("plain")));
    }

    #[test]
    fn logfmt_quoting_bare_keys_and_prefix() {
        let line = r#"2024-05-01T10:00:00Z INFO level=info msg="user \"bob\" logged in" status=200 dry_run user= trace"#;
        let (got, partial) = fields(&FieldParser::Logfmt, line);
        assert!(!partial);
        assert_eq!(
            got,
            pairs(&[
                (PREFIX_KEY, "2024-05-01T10:00:00Z INFO"),
                ("level", "info"),
                ("msg", "user \"bob\" logged in"),
                ("status", "200"),
                ("dry_run", ""),
                ("user", ""),
                ("trace", ""),
            ])
        );
        let (got, partial) = fields(&FieldParser::Logfmt, r#"a=1 b="open"#);
        assert!(partial);
        assert_eq!(got, pairs(&[("a", "1"), ("b", "open")]));
        let mut spans = FieldSpans::new();
        assert!(!scan(&FieldParser::Logfmt, "no pairs here", &mut spans));
    }

    #[test]
    fn apache_combined_and_common() {
        let parser = ParserChoice::Apache.build().unwrap().unwrap();
        let line = r#"127.0.0.1 - frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326 "http://www.example.com/start.html" "Mozilla/4.08""#;
        let (got, _) = fields(&parser, line);
        assert_eq!(
            got,
            pairs(&[
                ("host", "127.0.0.1"),
                ("ident", "-"),
                ("user", "frank"),
                ("time", "10/Oct/2000:13:55:36 -0700"),
                ("method", "GET"),
                ("path", "/apache_pb.gif"),
                ("protocol", "HTTP/1.0"),
                ("status", "200"),
                ("size", "2326"),
                ("referer", "http://www.example.com/start.html"),
                ("agent", "Mozilla/4.08"),
            ])
        );
        let (got, _) = fields(&parser, r#"::1 - - [10/Oct/2000:13:55:36 -0700] "-" 408 -"#);
        assert_eq!(got.iter().find(|(k, _)| k == "status").unwrap().1, "408");
        assert!(!got.iter().any(|(k, _)| k == "method"));
    }

    #[test]
    fn syslog_rfc3164() {
        let parser = ParserChoice::Syslog.build().unwrap().unwrap();
        let (got, _) = fields(
            &parser,
            "<34>Oct 11 22:14:15 mymachine su[123]: 'su root' failed",
        );
        assert_eq!(
            got,
            pairs(&[
                ("pri", "34"),
                ("time", "Oct 11 22:14:15"),
                ("host", "mymachine"),
                ("app", "su"),
                ("pid", "123"),
                ("msg", "'su root' failed"),
            ])
        );
        let (got, _) = fields(&parser, "Oct  1 02:04:05 host cron: job done");
        assert_eq!(got.len(), 4);
        let mut spans = FieldSpans::new();
        assert!(!scan(&parser, "not syslog", &mut spans));
    }

    #[test]
    fn custom_regex_needs_named_groups() {
        assert!(ParserChoice::Regex("(a)(b)".into()).build().is_err());
        assert!(ParserChoice::Regex("(?P<x".into()).build().is_err());
        let parser = ParserChoice::Regex(r"^(?P<lvl>\w+): (?P<rest>.*)".into())
            .build()
            .unwrap()
            .unwrap();
        let (got, _) = fields(&parser, "WARN: disk low");
        assert_eq!(got, pairs(&[("lvl", "WARN"), ("rest", "disk low")]));
        // The capture slots are reused across parsers without mixing them up.
        let other = ParserChoice::Syslog.build().unwrap().unwrap();
        let mut spans = FieldSpans::new();
        scan(&parser, "A: b", &mut spans);
        assert!(scan(&other, "Oct 11 22:14:15 h a: m", &mut spans));
        assert_eq!(spans.get("Oct 11 22:14:15 h a: m", "app").unwrap().raw, "a");
    }

    #[test]
    fn row_fields_message_and_outside() {
        let line = r#"{"ts":"t1","level":"info","msg":"hello\nworld","user":"bob","n":1}"#;
        let mut spans = FieldSpans::new();
        let row = row_fields(&FieldParser::Json, line, &mut spans);
        assert!(row.shaped && !row.partial);
        assert_eq!(row.get("msg"), Some("hello world"));
        let shown = vec!["ts".to_string(), "level".to_string()];
        assert_eq!(row.message(&shown), "hello world user=bob n=1");
        let shown = vec!["msg".to_string(), "user".to_string()];
        assert_eq!(row.message(&shown), "ts=t1 level=info n=1");

        let parser = ParserChoice::Regex(r"(?P<lvl>[A-Z]+):".into())
            .build()
            .unwrap()
            .unwrap();
        let row = row_fields(&parser, "at start WARN: disk low", &mut spans);
        assert_eq!(row.outside, "at start disk low");
        assert_eq!(row.message(&["lvl".to_string()]), "at start disk low");
        let row = row_fields(&parser, "no match", &mut spans);
        assert!(!row.shaped && row.fields.is_empty() && row.outside.is_empty());
    }

    #[test]
    fn catalogue_order_widths_cap_and_defaults() {
        let mut cat = FieldCatalogue::default();
        cat.note("ts", 24, true);
        cat.note("msg", 80, true);
        cat.note("level", 4, true);
        cat.note("level", 7, true);
        cat.note("level", 30, false);
        assert_eq!(cat.keys(), ["ts", "msg", "level"]);
        assert_eq!(cat.suggested_width("ts"), 24);
        assert_eq!(cat.suggested_width("msg"), 40);
        assert_eq!(
            cat.suggested_width("level"),
            7,
            "only the sample grows a width"
        );
        assert_eq!(cat.suggested_width("unknown_key_long"), 12);
        assert_eq!(cat.default_columns(), ["ts", "level"]);
        for i in 0..MAX_CATALOGUE + 5 {
            cat.note(&format!("k{i}"), 1, false);
        }
        assert_eq!(cat.keys().len(), MAX_CATALOGUE);
        assert!(cat.more);
        assert_eq!(cat.default_columns().len(), DEFAULT_COLUMNS);
    }

    #[test]
    fn row_cache_evicts_the_oldest_and_resets_on_a_new_key() {
        let mut cache = RowCache::default();
        let key = (1, 1, 1);
        assert!(cache.get(key, 0).is_none());
        for line in 0..ROW_CACHE + 10 {
            cache.put(line, Arc::new(RowFields::default()));
        }
        assert_eq!(cache.len(), ROW_CACHE);
        assert!(cache.get(key, 0).is_none());
        assert!(cache.get(key, ROW_CACHE + 9).is_some());
        assert!(cache.get((1, 2, 1), ROW_CACHE + 9).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn choice_names_round_trip() {
        for choice in [
            ParserChoice::Auto,
            ParserChoice::Off,
            ParserChoice::Json,
            ParserChoice::Logfmt,
            ParserChoice::Regex("(?P<a>x)".into()),
            ParserChoice::Apache,
            ParserChoice::Syslog,
        ] {
            let regex = match &choice {
                ParserChoice::Regex(p) => p.as_str(),
                _ => "",
            };
            assert_eq!(
                ParserChoice::from_name(choice.name(), regex),
                Some(choice.clone())
            );
        }
        assert_eq!(ParserChoice::from_name("xml", ""), None);
    }

    #[test]
    fn detection_thresholds() {
        let json: Vec<String> = (0..10)
            .map(|i| format!(r#"{{"i":{i},"msg":"x"}}"#))
            .collect();
        let mut lines: Vec<&str> = json.iter().map(String::as_str).collect();
        assert_eq!(detect(lines.iter().copied()), Some(ParserChoice::Json));
        lines.extend(["plain", "plain"]); // 10 of 12 = 83 %
        assert_eq!(detect(lines.iter().copied()), Some(ParserChoice::Json));
        lines.push("plain"); // 10 of 13 = 76 %
        assert_eq!(detect(lines.iter().copied()), None);

        let logfmt = [
            "a=1 b=2 c=3",
            "INFO a=1 b=\"x y\" c=",
            "a=1 b=2 c=3 d=4",
            "a=1 b=2 c=3",
            "x=1 only=2",
        ];
        // 4 of 5 = 80 %: a line with two pairs does not count.
        assert_eq!(detect(logfmt), Some(ParserChoice::Logfmt));
        assert_eq!(detect(["a=1 b=2", "c=1 d=2"]), None);
        assert_eq!(detect(["", "  "]), None);
        // A wide object cut at the field cap is still JSON.
        let wide = format!(
            "{{{}}}",
            (0..MAX_LINE_FIELDS + 20)
                .map(|i| format!(r#""k{i}":{i}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(detect([wide.as_str(); 5]), Some(ParserChoice::Json));
        assert_eq!(detect(["2024-01-01 12:00:00 bare words only"]), None);
    }
}

/// Counts the allocations of the current thread, for the test below: the library's unit
/// tests have no other global allocator.
#[cfg(test)]
mod alloc_count {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        pub static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    struct Counting;

    fn count() {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
    }

    // SAFETY: every call is forwarded to the system allocator unchanged.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count();
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            count();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;

    #[test]
    fn no_allocation_per_line_once_warmed_up() {
        use super::*;
        let json: Vec<String> = (0..500)
            .map(|i| format!(r#"{{"i":{i},"level":"info","http":{{"req":{{"status":{i}}},"tags":[1,2]}},"msg":"a \"b\""}}"#))
            .collect();
        let logfmt: Vec<String> = (0..500)
            .map(|i| format!(r#"INFO i={i} level=warn msg="x y" bare"#))
            .collect();
        let apache: Vec<String> = (0..500)
            .map(|i| format!(r#"1.2.3.4 - - [01/May/2024:10:00:00 +0000] "GET /{i} HTTP/1.1" 200 {i} "-" "ua""#))
            .collect();
        let regex = ParserChoice::Apache.build().unwrap().unwrap();
        for (parser, lines) in [
            (&FieldParser::Json, &json),
            (&FieldParser::Logfmt, &logfmt),
            (&*regex, &apache),
        ] {
            let mut spans = FieldSpans::new();
            for line in lines.iter().take(10) {
                scan(parser, line, &mut spans);
            }
            let before = ALLOCATIONS.with(Cell::get);
            let mut fields = 0;
            for line in lines {
                assert!(scan(parser, line, &mut spans));
                fields += spans.len();
                assert!(spans.get(line, "i").is_some() || spans.get(line, "status").is_some());
            }
            assert!(fields > 0);
            let allocations = ALLOCATIONS.with(Cell::get) - before;
            // The regex crate may still grow its lazy DFA cache now and then: amortized,
            // never one per line. The hand-written scanners allocate nothing.
            let bound = if matches!(parser, FieldParser::Regex(_)) {
                lines.len() / 50
            } else {
                0
            };
            assert!(
                allocations <= bound,
                "{allocations} allocations: {parser:?}"
            );
        }
    }
}
