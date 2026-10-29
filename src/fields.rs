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

    /// The names of the groups, in pattern order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(|(_, n)| &**n)
    }

    pub fn is_match(&self, line: &str) -> bool {
        self.regex.is_match(line)
    }
}

/// How the fields of a stream's lines are read.
#[derive(Debug)]
pub enum FieldParser {
    Json,
    Logfmt,
    Regex(RegexFields),
    Delimited(Delimited),
    /// A named log format (`log_format`): its patterns, and the fields that hold the
    /// timestamp, the level and the message.
    Format(Arc<crate::log_format::LogFormat>),
}

/// A CSV / TSV parser: the separator, and the column names read from the header row
/// (none: the columns are numbered `1`, `2`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delimited {
    pub sep: u8,
    pub names: Vec<String>,
    /// The header row itself, which is not a record.
    pub header_line: Option<String>,
}

/// The separators a delimited file may use, in the order detection tries them.
pub const SEPARATORS: [u8; 4] = *b",;\t|";

/// The name of a separator in `csv_separator=` and the selector.
pub fn separator_name(sep: u8) -> &'static str {
    match sep {
        b';' => ";",
        b'\t' => "tab",
        b'|' => "|",
        _ => ",",
    }
}

/// The separator `csv_separator=` names.
pub fn separator_from_name(name: &str) -> Option<u8> {
    match name.trim() {
        "," => Some(b','),
        ";" => Some(b';'),
        "tab" | "\\t" => Some(b'\t'),
        "|" => Some(b'|'),
        _ => None,
    }
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
    /// CSV / TSV: the separator and whether the first row names the columns.
    Delimited {
        sep: u8,
        header: bool,
    },
    /// A log format by name (`fields_parser=format:<name>`).
    Format(String),
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
            ParserChoice::Delimited { .. } => "csv",
            ParserChoice::Format(_) => "format",
        }
    }

    /// The value saved in `fields_parser=`: `name`, or `format:<name>` for a format.
    pub fn saved_name(&self) -> String {
        match self {
            ParserChoice::Format(name) => format!("{}{name}", crate::log_format::CHOICE_PREFIX),
            other => other.name().to_string(),
        }
    }

    /// Reads `fields_parser=` and, for `regex`, `fields_regex=`; unknown names are `None`.
    pub fn from_name(name: &str, regex: &str) -> Option<Self> {
        let prefix = crate::log_format::CHOICE_PREFIX;
        let trimmed = name.trim();
        let has_prefix = trimmed
            .get(..prefix.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(prefix));
        if has_prefix {
            let format = trimmed[prefix.len()..].trim();
            return (!format.is_empty()).then(|| ParserChoice::Format(format.into()));
        }
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "auto" => ParserChoice::Auto,
            "off" => ParserChoice::Off,
            "json" => ParserChoice::Json,
            "logfmt" => ParserChoice::Logfmt,
            "regex" => ParserChoice::Regex(regex.to_string()),
            "apache" => ParserChoice::Apache,
            "syslog" => ParserChoice::Syslog,
            // The separator and the header come from `csv_separator=` / `csv_header=`.
            "csv" => ParserChoice::Delimited {
                sep: b',',
                header: true,
            },
            _ => return None,
        })
    }

    /// The parser of a forced choice: `Ok(None)` for `Auto` and `Off`, `Err` for a regex
    /// that does not compile or has no named group. A delimited parser with a header reads
    /// its names from `first_line`.
    pub fn build_for(&self, first_line: Option<&str>) -> Result<Option<Arc<FieldParser>>, String> {
        if let ParserChoice::Delimited { sep, header } = self {
            return Ok(Some(Arc::new(FieldParser::Delimited(Delimited::new(
                *sep,
                header.then_some(first_line).flatten(),
            )))));
        }
        self.build()
    }

    /// `build_for` without the file: a delimited parser has numbered columns.
    pub fn build(&self) -> Result<Option<Arc<FieldParser>>, String> {
        let regex = |p: &str| RegexFields::new(p).map(|r| Some(Arc::new(FieldParser::Regex(r))));
        let format = |name: &str| match crate::log_format::library().find(name) {
            Some(f) => Ok(Some(Arc::new(FieldParser::Format(f.clone())))),
            None => Err(format!("format {name} not found")),
        };
        match self {
            ParserChoice::Auto | ParserChoice::Off => Ok(None),
            ParserChoice::Json => Ok(Some(Arc::new(FieldParser::Json))),
            ParserChoice::Logfmt => Ok(Some(Arc::new(FieldParser::Logfmt))),
            ParserChoice::Regex(p) => regex(p),
            // The presets are the built-in formats now (time and message fields).
            ParserChoice::Apache => {
                format(crate::log_format::APACHE_NAME).or_else(|_| regex(APACHE_PATTERN))
            }
            ParserChoice::Syslog => {
                format(crate::log_format::SYSLOG_NAME).or_else(|_| regex(SYSLOG_PATTERN))
            }
            ParserChoice::Format(name) => format(name),
            ParserChoice::Delimited { sep, .. } => Ok(Some(Arc::new(FieldParser::Delimited(
                Delimited::new(*sep, None),
            )))),
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
    /// A CSV quoted cell: `""` stands for one quote.
    doubled: bool,
}

/// The fields of one line, reused from line to line.
#[derive(Debug, Default)]
pub struct FieldSpans {
    spans: SmallVec<[FieldSpan; 32]>,
    /// Keys that are not a slice of the line; only UTF-8 is ever appended.
    scratch: Vec<u8>,
    /// Capture slots of the regex parsers, each with the regex it belongs to: one per
    /// pattern of a log format, so lines alternating between patterns reuse theirs.
    locs: SmallVec<[(usize, CaptureLocations); 2]>,
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
    /// A CSV quoted cell, whose `""` stand for one quote.
    pub doubled: bool,
}

impl Field<'_> {
    /// The value with its escapes decoded.
    pub fn value(&self) -> Cow<'_, str> {
        if self.doubled {
            if self.raw.contains("\"\"") {
                Cow::Owned(self.raw.replace("\"\"", "\""))
            } else {
                Cow::Borrowed(self.raw)
            }
        } else if self.quoted {
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
            doubled: span.doubled,
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
            doubled: false,
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
        FieldParser::Delimited(d) => scan_delimited(d, line, out),
        FieldParser::Format(f) => f.scan(line, out),
    }
}

/// The cells of a delimited `line` as byte ranges, RFC 4180 quoting: a cell opening with
/// `"` runs to the closing quote (`""` inside is a quote) and may hold the separator. The
/// ranges are inside the quotes; the flag says the cell was quoted. A quote left open
/// takes the rest of the line. Calls `f(start, end, quoted)` for each cell, and returns
/// whether the last quote was left open.
fn delimited_cells(b: &[u8], sep: u8, mut f: impl FnMut(usize, usize, bool) -> bool) -> bool {
    let mut pos = 0;
    loop {
        if b.get(pos) == Some(&b'"') {
            let start = pos + 1;
            let mut i = start;
            let mut closed = None;
            while let Some(q) = memchr::memchr(b'"', &b[i..]) {
                let at = i + q;
                if b.get(at + 1) == Some(&b'"') {
                    i = at + 2;
                    continue;
                }
                closed = Some(at);
                break;
            }
            let Some(end) = closed else {
                f(start, b.len(), true);
                return true;
            };
            if !f(start, end, true) {
                return false;
            }
            // Past the closing quote, to the next separator (stray text is dropped).
            match memchr::memchr(sep, &b[end + 1..]) {
                Some(n) => pos = end + 1 + n + 1,
                None => return false,
            }
        } else {
            let end = memchr::memchr(sep, &b[pos..]).map_or(b.len(), |n| pos + n);
            if !f(pos, end, false) {
                return false;
            }
            if end >= b.len() {
                return false;
            }
            pos = end + 1;
        }
    }
}

impl Delimited {
    /// The parser for `sep`, with the column names of `header` when given.
    pub fn new(sep: u8, header: Option<&str>) -> Self {
        let names = header
            .map(|h| {
                let b = h.trim_end_matches(['\r', '\n']).as_bytes();
                let mut names = Vec::new();
                delimited_cells(b, sep, |a, z, quoted| {
                    let raw = String::from_utf8_lossy(&b[a..z]);
                    let name = if quoted {
                        raw.replace("\"\"", "\"")
                    } else {
                        raw.trim().to_string()
                    };
                    names.push(name);
                    true
                });
                names
            })
            .unwrap_or_default();
        Self {
            sep,
            header_line: header.map(|h| h.trim_end_matches(['\r', '\n']).to_string()),
            names,
        }
    }
}

fn scan_delimited(d: &Delimited, line: &str, out: &mut FieldSpans) -> bool {
    let line_t = line.trim_end_matches(['\r', '\n']);
    // The header row names the columns: it is no record.
    if d.header_line.as_deref() == Some(line_t) {
        return false;
    }
    let b = line_t.as_bytes();
    let mut n = 0usize;
    let mut cells = 0usize;
    let open = delimited_cells(b, d.sep, |a, z, quoted| {
        cells += 1;
        let key = match d.names.get(n) {
            Some(name) => out.scratch_key(name.as_bytes()),
            None => {
                let mut buf = itoa_buf(n + 1);
                out.scratch_key(buf.as_bytes_mut())
            }
        };
        n += 1;
        if out.spans.len() >= MAX_LINE_FIELDS {
            out.partial = true;
            out.truncated = true;
            return false;
        }
        out.spans.push(FieldSpan {
            key: KeyAt::Scratch(key.0, key.1),
            value: (a as u32, z as u32),
            quoted,
            doubled: quoted,
        });
        true
    });
    if open {
        out.partial = true;
    }
    cells >= 2
}

/// `n` in decimal, on the stack.
fn itoa_buf(n: usize) -> ItoaBuf {
    let mut buf = ItoaBuf {
        bytes: [0; 20],
        start: 20,
    };
    let mut n = n;
    loop {
        buf.start -= 1;
        buf.bytes[buf.start] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    buf
}

struct ItoaBuf {
    bytes: [u8; 20],
    start: usize,
}

impl ItoaBuf {
    fn as_bytes_mut(&mut self) -> &[u8] {
        &self.bytes[self.start..]
    }
}

/// How many lines detection reads for a delimited file.
pub const DELIMITED_DETECT_LINES: usize = 64;

/// The separator that splits `lines` into the same number (at least two) of cells on at
/// least 90 % of them, quotes taken into account; `prefer` (from the extension) breaks a
/// tie. `None` when no separator does.
pub fn detect_separator(lines: &[&str], prefer: Option<u8>) -> Option<u8> {
    let lines = records(lines);
    if lines.is_empty() {
        return None;
    }
    let mut best: Option<(usize, u8)> = None;
    for sep in SEPARATORS {
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for line in lines.iter() {
            let mut cells = 0;
            delimited_cells(line.trim_end().as_bytes(), sep, |_, _, _| {
                cells += 1;
                true
            });
            *counts.entry(cells).or_default() += 1;
        }
        let Some((&cells, &hits)) = counts.iter().max_by_key(|(c, h)| (**h, **c)) else {
            continue;
        };
        if cells < 2 || hits * 10 < lines.len() * 9 {
            continue;
        }
        let score = hits * 1000 + cells;
        let better = match best {
            None => true,
            Some((s, b)) => score > s || (score == s && Some(sep) == prefer && Some(b) != prefer),
        };
        if better {
            best = Some((score, sep));
        }
    }
    best.map(|(_, sep)| sep)
}

/// The first `DELIMITED_DETECT_LINES` non-blank records of `lines`: a line that leaves a
/// quote open is joined with the next ones up to the line that closes it.
fn records(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = false;
    for line in lines {
        if open {
            if let Some(last) = out.last_mut() {
                last.push('\n');
                last.push_str(line);
            }
            open = !toggles_quote(line);
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if out.len() >= DELIMITED_DETECT_LINES {
            break;
        }
        out.push(line.to_string());
        open = toggles_quote(line);
    }
    out
}

/// Whether the first of `lines` names the columns: its cells are non-empty, not numbers,
/// all different, and the row is not repeated as the second one.
pub fn detect_header(lines: &[&str], sep: u8) -> bool {
    let Some(first) = lines.iter().find(|l| !l.trim().is_empty()) else {
        return false;
    };
    let d = Delimited::new(sep, Some(first));
    let distinct: std::collections::HashSet<&str> = d.names.iter().map(String::as_str).collect();
    let second = lines.iter().filter(|l| !l.trim().is_empty()).nth(1);
    d.names.len() >= 2
        && distinct.len() == d.names.len()
        && d.names
            .iter()
            .all(|n| !n.is_empty() && n.trim().parse::<f64>().is_err())
        && second.is_none_or(|s| s.trim_end() != first.trim_end())
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

pub(crate) fn scan_regex(r: &RegexFields, line: &str, out: &mut FieldSpans) -> bool {
    let id = r as *const RegexFields as usize;
    // The address alone could be reused by a later parser: the slot count must match too.
    let at = out
        .locs
        .iter()
        .position(|(owner, locs)| *owner == id && locs.len() == r.regex.captures_len());
    let mut locs = match at {
        Some(i) => out.locs.remove(i).1,
        None => {
            // Bounded: as many as a format has patterns, the oldest dropped first.
            if out.locs.len() >= crate::log_format::MAX_PATTERNS {
                out.locs.remove(0);
            }
            r.regex.capture_locations()
        }
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
    out.locs.push((id, locs));
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
    /// A log format's message field, tried before the usual message keys.
    pub message_key: Option<String>,
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
        let keys = self.message_key.as_deref().into_iter().chain(MESSAGE_KEYS);
        for key in keys {
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

/// A quoted CSV cell over several lines on one line: each line break becomes `⏎` (a
/// `\r` before it goes), other control characters spaces.
fn one_line_marked(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .chars()
        .map(|c| match c {
            '\n' => '⏎',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect()
}

/// Most lines one quoted CSV record may take: a quote left open in a broken file must not
/// swallow the rest of it.
pub const MAX_RECORD_LINES: usize = 1000;

/// Whether `line` leaves a CSV quote open or closes one left open: an odd number of
/// quotes (a `""` inside a cell counts two).
pub fn toggles_quote(line: &str) -> bool {
    memchr::memchr_iter(b'"', line.as_bytes()).count() % 2 == 1
}

/// Scans `line` into owned, decoded `RowFields` (`spans` is the reused scratch).
pub fn row_fields(parser: &FieldParser, line: &str, spans: &mut FieldSpans) -> RowFields {
    let shaped = scan(parser, line, spans);
    let fields = spans
        .iter(line)
        .map(|f| {
            let value = f.value();
            let shown = if f.doubled {
                one_line_marked(&value)
            } else {
                one_line(&value)
            };
            (f.key.to_string(), shown)
        })
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
    let message_key = match parser {
        FieldParser::Format(f) if !f.spec.message_field.is_empty() => {
            Some(f.spec.message_field.clone())
        }
        _ => None,
    };
    RowFields {
        shaped,
        partial: spans.partial,
        fields,
        outside,
        message_key,
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

/// A CSV / TSV parser for `lines` (the first of a stream), when the file says it is a
/// table: its extension is `csv`, `tsv` or `psv` and a separator splits the lines evenly,
/// or a separator does and the first row is a header of column-like names (no spaces), so
/// a plain log with a regular `|` or `,` does not turn into a table by itself.
pub fn detect_table(lines: &[&str], ext: Option<&str>) -> Option<ParserChoice> {
    let prefer = match ext {
        Some("tsv") => Some(b'\t'),
        Some("psv") => Some(b'|'),
        Some("csv") => Some(b','),
        _ => None,
    };
    let sep = detect_separator(lines, prefer)?;
    let header = detect_header(lines, sep);
    // Without the extension, only a header of column-like names makes a table: a log line
    // with a regular comma has words with spaces before it.
    let names_like = || {
        let first = lines.iter().find(|l| !l.trim().is_empty()).copied();
        Delimited::new(sep, first)
            .names
            .iter()
            .all(|n| n.len() <= 64 && !n.contains(char::is_whitespace))
    };
    let table = prefer.is_some() || (header && lines.len() >= 2 && names_like());
    table.then_some(ParserChoice::Delimited { sep, header })
}

/// The fields a line's level is read from, in this order.
pub const LEVEL_KEYS: [&str; 4] = ["level", "lvl", "severity", "log.level"];
/// The fields a line's timestamp is read from, in this order.
pub const TIME_KEYS: [&str; 5] = ["ts", "time", "timestamp", "@timestamp", "t"];

thread_local! {
    /// The field spans of the line being read for a filter, a level or a timestamp: one
    /// buffer per thread (the window's and each job's), so it costs no allocation per line.
    static SPANS: std::cell::RefCell<FieldSpans> = std::cell::RefCell::new(FieldSpans::new());
}

/// Runs `f` with this thread's field buffer (a fresh one when it is in use already).
pub fn with_spans<R>(f: impl FnOnce(&mut FieldSpans) -> R) -> R {
    SPANS.with(|cell| match cell.try_borrow_mut() {
        Ok(mut spans) => f(&mut spans),
        Err(_) => f(&mut FieldSpans::new()),
    })
}

/// The level named by the first level field of `line`, already scanned into `spans`.
pub fn level_in(spans: &FieldSpans, line: &str) -> Option<crate::log_level::LogLevel> {
    LEVEL_KEYS.iter().find_map(|key| {
        spans
            .get(line, key)
            .and_then(|f| crate::log_level::level_word(&f.value()))
    })
}

/// The timestamp of the first time field of `line` (scanned into `spans`) that the line
/// parsers recognise: ISO 8601, epoch seconds or milliseconds.
pub fn timestamp_in(
    spans: &FieldSpans,
    line: &str,
    hint: crate::timestamp::FormatHint,
) -> Option<(i64, crate::timestamp::FormatHint)> {
    TIME_KEYS.iter().find_map(|key| {
        spans
            .get(line, key)
            .and_then(|f| crate::timestamp::detect_timestamp(f.value().trim(), hint))
    })
}

/// Whether `line` continues the entry above it: with a log format that has an entry start
/// pattern, exactly when the line does not match it; otherwise the stack-trace heuristic
/// (indentation, `at `, `Caused by:`...).
pub fn is_continuation(parser: Option<&FieldParser>, line: &str) -> bool {
    match parser {
        Some(FieldParser::Format(f)) if f.entry_start.is_some() => !f.starts_entry(line),
        _ => crate::tail_engine::TailEngine::is_stacktrace_continuation(line),
    }
}

/// A line that continues an entry by its format's entry start pattern: it has no level
/// or time of its own (its time is the entry's).
fn format_continuation(parser: &FieldParser, line: &str) -> bool {
    matches!(parser, FieldParser::Format(f) if f.entry_start.as_ref().is_some_and(|s| !s.is_match(line)))
}

/// The level of `line`: from its fields when `parser` is set and a level field names a
/// level, otherwise detected from the first `window` bytes of the text.
pub fn line_level(
    parser: Option<&FieldParser>,
    line: &str,
    window: usize,
) -> crate::log_level::LogLevel {
    if let Some(parser) = parser {
        if format_continuation(parser, line) {
            return crate::log_level::LogLevel::Unknown;
        }
        let found = with_spans(|spans| {
            scan(parser, line, spans);
            match parser {
                FieldParser::Format(f) if !f.spec.level_field.is_empty() => f.level(spans, line),
                _ => level_in(spans, line),
            }
        });
        if let Some(level) = found {
            return level;
        }
    }
    crate::log_level::detect_level_in(line, window)
}

/// The timestamp of `line`: from its fields when `parser` is set and a time field holds
/// one, otherwise detected at the start of the text.
pub fn line_timestamp(
    parser: Option<&FieldParser>,
    line: &str,
    hint: crate::timestamp::FormatHint,
) -> Option<(i64, crate::timestamp::FormatHint)> {
    if let Some(parser) = parser {
        if format_continuation(parser, line) {
            return None;
        }
        let found = with_spans(|spans| {
            scan(parser, line, spans);
            match parser {
                FieldParser::Format(f) if !f.spec.timestamp_field.is_empty() => {
                    f.timestamp(spans, line).map(|t| (t, hint))
                }
                _ => timestamp_in(spans, line, hint),
            }
        });
        if found.is_some() {
            return found;
        }
    }
    crate::timestamp::detect_timestamp(line, hint)
}

/// What a bare number of `key` means on a stream read with `parser`: the `unit.<key>`
/// of its log format.
pub fn field_unit(parser: Option<&FieldParser>, key: &str) -> Option<crate::units::Unit> {
    match parser {
        Some(FieldParser::Format(f)) => f.unit_of(key),
        _ => None,
    }
}

/// The comparison of a field term.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOp {
    /// `=`: the value equals one of the alternatives.
    Eq,
    /// `!=`: the value equals none of them.
    Ne,
    /// `~=`: the value contains the text.
    Contains,
    Gt,
    Ge,
    Lt,
    Le,
}

impl FieldOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            FieldOp::Eq => "=",
            FieldOp::Ne => "!=",
            FieldOp::Contains => "~=",
            FieldOp::Gt => ">",
            FieldOp::Ge => ">=",
            FieldOp::Lt => "<",
            FieldOp::Le => "<=",
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self, FieldOp::Gt | FieldOp::Ge | FieldOp::Lt | FieldOp::Le)
    }
}

/// A filter term on a field (`status>=500`, `level=error|fatal`, `user~=bot`), read from
/// an include or exclude term of a stream that has a parser. A line without the field
/// fails it, on either side.
#[derive(Debug, Clone)]
pub struct FieldTerm {
    pub key: String,
    pub op: FieldOp,
    /// The `|` alternatives of `=` / `!=` (one value for the other operators), as typed.
    pub values: Vec<String>,
    /// Their lower-case forms, for the case-insensitive comparison.
    lower: Vec<String>,
    /// In regex mode, the alternatives (anchored for `=` / `!=`) or the `~=` text.
    regexes: Vec<Regex>,
    /// The number of a numeric comparison, in its unit's base (`500ms`, `10MiB`); `None`
    /// when the term's value is not one.
    number: Option<crate::units::Reading>,
    case_sensitive: bool,
    is_regex: bool,
}

/// `text` without its surrounding double quotes, when it has them: a quoted term is
/// literal text on a stream with a parser.
pub fn unquote(text: &str) -> Option<&str> {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .filter(|_| text.len() >= 2)
}

/// The key at the start of `text` (a letter, `_` or `@`, then letters, digits, `_`, `.`,
/// `@` and `-`; or only digits, a numbered CSV column) and its length.
fn key_len(text: &str) -> usize {
    let mut chars = text.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_alphabetic() || c == '_' || c == '@' => {}
        Some((_, c)) if c.is_ascii_digit() => {
            return text
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(text.len());
        }
        _ => return 0,
    }
    for (i, c) in chars {
        if !(c.is_alphanumeric() || matches!(c, '_' | '.' | '@' | '-')) {
            return i;
        }
    }
    text.len()
}

impl FieldTerm {
    /// Reads `text` as a field term under the stream's toggles; `None` when it is not one
    /// (no operator after a key, or quoted). A regex that does not compile is `Err`.
    pub fn parse(text: &str, case_sensitive: bool, is_regex: bool) -> Option<Result<Self, ()>> {
        if unquote(text).is_some() {
            return None;
        }
        let text = text.trim_start();
        let n = key_len(text);
        if n == 0 {
            return None;
        }
        let key = &text[..n];
        let rest = text[n..].trim_start();
        let (op, value) = [
            ("!=", FieldOp::Ne),
            ("~=", FieldOp::Contains),
            (">=", FieldOp::Ge),
            ("<=", FieldOp::Le),
            ("=", FieldOp::Eq),
            (">", FieldOp::Gt),
            ("<", FieldOp::Lt),
        ]
        .iter()
        .find_map(|(sym, op)| rest.strip_prefix(sym).map(|v| (*op, v.trim())))?;
        let values: Vec<String> = match op {
            FieldOp::Eq | FieldOp::Ne => value.split('|').map(|v| v.trim().to_string()).collect(),
            _ => vec![value.to_string()],
        };
        let mut regexes = Vec::new();
        if is_regex && !op.is_numeric() {
            for v in &values {
                let pattern = if op == FieldOp::Contains {
                    v.clone()
                } else {
                    format!("^(?:{v})$")
                };
                match regex::RegexBuilder::new(&pattern)
                    .case_insensitive(!case_sensitive)
                    .build()
                {
                    Ok(re) => regexes.push(re),
                    Err(_) => return Some(Err(())),
                }
            }
        }
        Some(Ok(Self {
            key: key.to_string(),
            op,
            lower: values.iter().map(|v| v.to_lowercase()).collect(),
            number: op.is_numeric().then(|| crate::units::read(value)).flatten(),
            values,
            regexes,
            case_sensitive,
            is_regex,
        }))
    }

    /// Whether the field's `value` passes the term; `None` (no such field) fails.
    pub fn eval(&self, value: Option<&str>) -> bool {
        self.eval_as(value, None)
    }

    /// The unit family of a numeric comparison with a unit (`latency>=500ms`).
    pub fn unit_family(&self) -> Option<crate::units::Family> {
        self.number
            .map(|r| r.family)
            .filter(|f| *f != crate::units::Family::Plain)
    }

    /// `eval`, a bare number of the field meaning `bare` (a log format's `unit.<field>`).
    /// A bound with a unit compares values of its family in the base unit (`1.2s >= 1s`)
    /// and fails the others; a bare bound compares any number in its base unit.
    pub fn eval_as(&self, value: Option<&str>, bare: Option<crate::units::Unit>) -> bool {
        let Some(value) = value else {
            return false;
        };
        match self.op {
            FieldOp::Eq => self.equals_any(value),
            FieldOp::Ne => !self.equals_any(value),
            FieldOp::Contains => {
                if self.is_regex {
                    self.regexes.iter().any(|re| re.is_match(value))
                } else if self.case_sensitive {
                    value.contains(self.values[0].as_str())
                } else {
                    crate::tail_engine::contains_case_insensitive(value, &self.lower[0])
                }
            }
            op => {
                let (Some(bound), Some(read)) = (self.number, crate::units::read_as(value, bare))
                else {
                    return false;
                };
                if bound.family != crate::units::Family::Plain && read.family != bound.family {
                    return false;
                }
                let (bound, v) = (bound.value, read.value);
                match op {
                    FieldOp::Gt => v > bound,
                    FieldOp::Ge => v >= bound,
                    FieldOp::Lt => v < bound,
                    _ => v <= bound,
                }
            }
        }
    }

    fn equals_any(&self, value: &str) -> bool {
        if self.is_regex {
            self.regexes.iter().any(|re| re.is_match(value))
        } else if self.case_sensitive {
            self.values.iter().any(|v| v == value)
        } else if value.is_ascii() {
            // An ASCII value lowercases to ASCII: a byte comparison, no char iterators.
            self.lower.iter().any(|l| value.eq_ignore_ascii_case(l))
        } else {
            self.lower
                .iter()
                .any(|l| value.chars().flat_map(char::to_lowercase).eq(l.chars()))
        }
    }

    /// The term as read, for the tooltip of its row: `key`, the operator and the values.
    pub fn describe(&self) -> String {
        format!(
            "{} {} {}",
            self.key,
            self.op.symbol(),
            self.values.join(" | ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(d: &Delimited, line: &str) -> Vec<(String, String)> {
        let mut spans = FieldSpans::new();
        scan(&FieldParser::Delimited(d.clone()), line, &mut spans);
        spans
            .iter(line)
            .map(|f| (f.key.to_string(), f.value().into_owned()))
            .collect()
    }

    #[test]
    fn delimited_cells_follow_rfc_4180() {
        let d = Delimited::new(b',', Some("id,customer,note"));
        assert_eq!(d.names, vec!["id", "customer", "note"]);
        assert_eq!(
            cells(&d, r#"7,"Smith, John","said ""hi"""#),
            vec![
                ("id".into(), "7".into()),
                ("customer".into(), "Smith, John".into()),
                ("note".into(), "said \"hi\"".into()),
            ]
        );
        // Empty cells, more cells than names, the header row itself.
        assert_eq!(
            cells(&d, "1,,x,extra"),
            vec![
                ("id".into(), "1".into()),
                ("customer".into(), "".into()),
                ("note".into(), "x".into()),
                ("4".into(), "extra".into()),
            ]
        );
        let mut spans = FieldSpans::new();
        assert!(!scan(
            &FieldParser::Delimited(d.clone()),
            "id,customer,note",
            &mut spans
        ));
        assert!(!scan(
            &FieldParser::Delimited(d.clone()),
            "single",
            &mut spans
        ));
        // An open quote takes the rest of the line.
        assert!(scan(
            &FieldParser::Delimited(d.clone()),
            r#"1,"open, still"#,
            &mut spans
        ));
        assert!(spans.partial);
        let numbered = Delimited::new(b'\t', None);
        assert_eq!(cells(&numbered, "a\tb")[1], ("2".into(), "b".into()));
    }

    #[test]
    fn separators_and_headers_are_detected() {
        let csv = [
            "id,customer,total",
            "1,ann,10",
            "2,\"bo, b\",12.5",
            "3,cy,7",
        ];
        assert_eq!(detect_separator(&csv, None), Some(b','));
        assert!(detect_header(&csv, b','));
        let semi = ["1;2;3", "4;5;6", "7;8;9"];
        assert_eq!(detect_separator(&semi, None), Some(b';'));
        assert!(!detect_header(&semi, b';'), "numbers are no header");
        assert_eq!(
            detect_table(&semi, Some("csv")),
            Some(ParserChoice::Delimited {
                sep: b';',
                header: false
            })
        );
        assert_eq!(detect_table(&semi, None), None, "no extension, no header");
        let tsv = ["a\tb", "1\t2"];
        assert_eq!(
            detect_table(&tsv, Some("tsv")),
            Some(ParserChoice::Delimited {
                sep: b'\t',
                header: true
            })
        );
        let log = ["2026-01-01 INFO start", "2026-01-01 WARN x, y"];
        assert_eq!(detect_table(&log, Some("log")), None);
        assert_eq!(detect_table(&["INFO a, b", "WARN c, d"], None), None);
        assert!(detect_table(&["host,status", "a,200"], None).is_some());
        assert_eq!(separator_from_name(separator_name(b'\t')), Some(b'\t'));
    }

    #[test]
    fn field_terms_parse_their_grammar() {
        let t = |s: &str| FieldTerm::parse(s, false, false).map(|r| r.unwrap());
        let a = t("http.status >= 500").unwrap();
        assert_eq!((a.key.as_str(), a.op), ("http.status", FieldOp::Ge));
        assert_eq!(a.describe(), "http.status >= 500");
        let b = t("level=error|fatal").unwrap();
        assert_eq!(b.values, vec!["error", "fatal"]);
        assert_eq!(t("@timestamp!=x").unwrap().op, FieldOp::Ne);
        assert_eq!(t("user~=bot").unwrap().op, FieldOp::Contains);
        assert_eq!(t("a<1").unwrap().op, FieldOp::Lt);
        assert_eq!(t("a<=1").unwrap().op, FieldOp::Le);
        assert_eq!(t("a>1").unwrap().op, FieldOp::Gt);
        assert!(t("\"level=error\"").is_none(), "quoted");
        assert!(t("1a=2").is_none(), "a key starts with a letter, _ or @");
        assert_eq!(t("2>=5").unwrap().key, "2", "a numbered column");
        assert!(t("plain text").is_none());
        assert!(t("a && !b").is_none());
        assert!(FieldTerm::parse("k=(", false, true).unwrap().is_err());
        assert_eq!(unquote("\"x=1\""), Some("x=1"));
        assert_eq!(unquote("\""), None);
    }

    #[test]
    fn field_terms_evaluate_values() {
        let t = |s: &str, cs: bool, re: bool| FieldTerm::parse(s, cs, re).unwrap().unwrap();
        assert!(t("level=ERROR|fatal", false, false).eval(Some("Error")));
        assert!(!t("level=ERROR", true, false).eval(Some("Error")));
        assert!(t("level!=info", false, false).eval(Some("warn")));
        assert!(!t("level!=info", false, false).eval(None), "missing fails");
        assert!(t("s>=500", false, false).eval(Some(" 502 ")));
        assert!(!t("s>=500", false, false).eval(Some("n/a")));
        assert!(!t("s>=x", false, false).eval(Some("600")));
        assert!(t("s<1.5", false, false).eval(Some("1.25")));
        assert!(t("u~=Bot", false, false).eval(Some("my-bot-7")));
        assert!(t("u=b.t", false, true).eval(Some("bot")));
        assert!(!t("u=b.t", false, true).eval(Some("robot")), "anchored");
        assert!(t("u~=b.t", false, true).eval(Some("robot")));
        // Case folding on ASCII and non-ASCII values alike.
        assert!(t("city=ZÜRICH", false, false).eval(Some("zürich")));
        assert!(t("city=zürich", false, false).eval(Some("ZÜRICH")));
        assert!(!t("city=zurich", false, false).eval(Some("zürich")));
        assert!(t("env=PROD", false, false).eval(Some("prod")));
        assert!(!t("env=PROD", false, false).eval(Some("prod2")));
    }

    #[test]
    fn field_terms_compare_values_with_units() {
        let t = |s: &str| FieldTerm::parse(s, false, false).unwrap().unwrap();
        let keep: Vec<&str> = ["900ms", "1.2s", "45ms"]
            .into_iter()
            .filter(|v| t("latency>=1s").eval(Some(v)))
            .collect();
        assert_eq!(keep, vec!["1.2s"]);
        assert!(t("latency>=500ms").eval(Some("1m30s")));
        assert!(!t("latency>=500ms").eval(Some("499ms")));
        assert!(
            !t("latency>=500ms").eval(Some("600KB")),
            "another family fails"
        );
        assert!(
            !t("latency>=500ms").eval(Some("600")),
            "a bare value has no unit"
        );
        let ms = crate::units::unit_named("ms");
        assert!(
            t("latency>=500ms").eval_as(Some("600"), ms),
            "unless the format gives one"
        );
        assert!(
            t("latency>100").eval(Some("120ms")),
            "a bare bound in the base unit"
        );
        assert!(t("size>1000B").eval(Some("1KiB")));
        assert!(!t("size>1000B").eval(Some("1KB")));
        assert!(t("size<=1MB").eval(Some("976KiB")));
        assert_eq!(
            t("latency>=1s").unit_family(),
            Some(crate::units::Family::Time)
        );
        assert_eq!(t("status>=500").unit_family(), None);
    }

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
        // A log format whose lines alternate between its two patterns.
        let two = crate::log_format::LogFormat::compile(crate::log_format::FormatSpec {
            name: "two".into(),
            patterns: vec![
                r"^A (?P<i>\d+) (?P<t>\d\d:\d\d [AP]M)".into(),
                r"^B (?P<status>\d+) (?P<t>\d\d:\d\d [AaPp][Mm]) (?P<level>\w+)".into(),
            ],
            timestamp_field: "t".into(),
            timestamp_format: "%I:%M %p".into(),
            ..Default::default()
        })
        .unwrap();
        let two = FieldParser::Format(Arc::new(two));
        let mixed: Vec<String> = (0..500)
            .map(|i| match i % 2 {
                0 => format!("A {i} 10:15 AM"),
                _ => format!("B {i} 11:30 pm warn"),
            })
            .collect();
        for (parser, lines) in [
            (&FieldParser::Json, &json),
            (&FieldParser::Logfmt, &logfmt),
            (&*regex, &apache),
            (&two, &mixed),
        ] {
            let mut spans = FieldSpans::new();
            for line in lines.iter().take(10) {
                scan(parser, line, &mut spans);
                line_timestamp(Some(parser), line, Default::default());
                line_level(Some(parser), line, 256);
            }
            let before = ALLOCATIONS.with(Cell::get);
            let mut fields = 0;
            for line in lines {
                assert!(scan(parser, line, &mut spans));
                fields += spans.len();
                assert!(spans.get(line, "i").is_some() || spans.get(line, "status").is_some());
            }
            assert!(fields > 0);
            // A format's level and time paths scan again and read its fields.
            if let FieldParser::Format(_) = parser {
                for line in lines {
                    assert!(line_timestamp(Some(parser), line, Default::default()).is_some());
                    line_level(Some(parser), line, 256);
                }
            }
            let allocations = ALLOCATIONS.with(Cell::get) - before;
            // The regex crate may still grow its lazy DFA cache now and then: amortized,
            // never one per line. The hand-written scanners allocate nothing.
            let bound = if matches!(parser, FieldParser::Regex(_) | FieldParser::Format(_)) {
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
