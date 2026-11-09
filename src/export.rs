// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Export of a stream's lines as text, CSV or HTML: which lines (the visible ones, the
//! selected ones, the search matches), and a writer per format. The job writes a few
//! milliseconds per frame through a buffered writer, so nothing is held in memory and a
//! large export never freezes the view; it covers the lines as they were when it started.
//!
//! CSV (RFC 4180, CRLF) has the columns `line,time,level,text`, or on a stream with a
//! field parser `line`, the shown fields and `message`; a cell starting with `=`, `+`,
//! `-`, `@`, tab or carriage return gets a leading `'` (formula protection, on by
//! default). The HTML writer lives with the window (`ui::export_html`), which knows the
//! colours.

use crate::tail_engine::{RowText, TailEngine};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Which lines are exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The lines the filters let through (those inside collapsed groups included).
    Visible,
    Selected,
    Matches,
}

/// The file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Csv,
    Html,
}

impl Format {
    /// The name saved as `export_format=`.
    pub fn name(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Csv => "csv",
            Format::Html => "html",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "text" => Some(Format::Text),
            "csv" => Some(Format::Csv),
            "html" => Some(Format::Html),
            _ => None,
        }
    }

    /// The file extension.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Text => "txt",
            Format::Csv => "csv",
            Format::Html => "html",
        }
    }
}

/// Lines an HTML export holds at most (a browser still opens it).
pub const HTML_MAX_LINES: usize = 200_000;

/// The CSV options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvOptions {
    pub sep: u8,
    /// A UTF-8 byte order mark first, so Excel reads accents.
    pub bom: bool,
    /// Formula protection.
    pub protect: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            sep: b',',
            bom: true,
            protect: true,
        }
    }
}

/// `comma`, `semicolon`, `tab` as saved in `export_csv_separator=`.
pub fn separator_name(sep: u8) -> &'static str {
    match sep {
        b';' => "semicolon",
        b'\t' => "tab",
        _ => "comma",
    }
}

pub fn separator_from_name(name: &str) -> Option<u8> {
    match name.trim() {
        "comma" => Some(b','),
        "semicolon" => Some(b';'),
        "tab" => Some(b'\t'),
        _ => None,
    }
}

/// Writes the rows of an export in one format.
pub trait RowWriter {
    fn begin(&mut self, out: &mut dyn Write, engine: &TailEngine) -> std::io::Result<()>;
    /// Line `idx`, its text as the view reads it (ANSI escapes left out).
    fn row(
        &mut self,
        out: &mut dyn Write,
        engine: &TailEngine,
        idx: usize,
        row: &RowText,
    ) -> std::io::Result<()>;
    fn end(&mut self, out: &mut dyn Write, engine: &TailEngine) -> std::io::Result<()>;
}

/// The lines as they are, one per line (today's export).
#[derive(Debug, Default)]
pub struct TextWriter;

impl RowWriter for TextWriter {
    fn begin(&mut self, _: &mut dyn Write, _: &TailEngine) -> std::io::Result<()> {
        Ok(())
    }

    fn row(
        &mut self,
        out: &mut dyn Write,
        engine: &TailEngine,
        idx: usize,
        _: &RowText,
    ) -> std::io::Result<()> {
        // The raw line, escapes included, as Export always wrote it.
        if let Some(line) = engine.get_line(idx) {
            out.write_all(line.as_bytes())?;
            out.write_all(b"\n")?;
        }
        Ok(())
    }

    fn end(&mut self, _: &mut dyn Write, _: &TailEngine) -> std::io::Result<()> {
        Ok(())
    }
}

/// CSV rows: `line,time,level,text`, or with a field parser `line`, the fields and
/// `message`.
#[derive(Debug)]
pub struct CsvWriter {
    pub options: CsvOptions,
    /// The field columns, when the stream has a parser.
    columns: Option<Vec<String>>,
    cell: String,
}

impl CsvWriter {
    pub fn new(options: CsvOptions) -> Self {
        Self {
            options,
            columns: None,
            cell: String::new(),
        }
    }

    /// Writes `value` as one cell (quoted when needed, protected when asked), after the
    /// separator unless `first`.
    fn cell(&mut self, out: &mut dyn Write, value: &str, first: bool) -> std::io::Result<()> {
        if !first {
            out.write_all(&[self.options.sep])?;
        }
        write_cell(out, &mut self.cell, value, self.options)
    }
}

/// One CSV cell: formula protection, then RFC 4180 quoting (`buf` is reused).
pub fn write_cell(
    out: &mut dyn Write,
    buf: &mut String,
    value: &str,
    options: CsvOptions,
) -> std::io::Result<()> {
    buf.clear();
    if options.protect && value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        buf.push('\'');
    }
    buf.push_str(value);
    let sep = options.sep as char;
    if buf.contains([sep, '"', '\r', '\n']) {
        out.write_all(b"\"")?;
        out.write_all(buf.replace('"', "\"\"").as_bytes())?;
        out.write_all(b"\"")
    } else {
        out.write_all(buf.as_bytes())
    }
}

/// A timestamp in ISO 8601 (`2026-09-28T14:02:11.123`, the milliseconds only when
/// there are some), on the clock the log printed.
pub fn iso_time(ms: i64) -> String {
    let text = crate::timestamp::format_millis(ms).replacen(' ', "T", 1);
    match ms.rem_euclid(1000) {
        0 => text,
        milli => format!("{text}.{milli:03}"),
    }
}

impl RowWriter for CsvWriter {
    fn begin(&mut self, out: &mut dyn Write, engine: &TailEngine) -> std::io::Result<()> {
        if self.options.bom {
            out.write_all("\u{feff}".as_bytes())?;
        }
        if engine.field_parser().is_some() {
            let columns = engine.field_columns();
            self.cell(out, "line", true)?;
            for c in &columns {
                self.cell(out, c, false)?;
            }
            self.cell(out, "message", false)?;
            self.columns = Some(columns);
        } else {
            for (i, c) in ["line", "time", "level", "text"].iter().enumerate() {
                self.cell(out, c, i == 0)?;
            }
        }
        out.write_all(b"\r\n")
    }

    fn row(
        &mut self,
        out: &mut dyn Write,
        engine: &TailEngine,
        idx: usize,
        row: &RowText,
    ) -> std::io::Result<()> {
        let number = engine.shown_line_number(idx).to_string();
        self.cell(out, &number, true)?;
        match (&self.columns, engine.field_parser()) {
            (Some(columns), Some(parser)) => {
                let fields = crate::fields::with_spans(|spans| {
                    crate::fields::row_fields(parser, &row.line, spans)
                });
                let columns = columns.clone();
                for c in &columns {
                    self.cell(out, fields.get(c).unwrap_or(""), false)?;
                }
                let message = if fields.shaped {
                    fields.message(&columns)
                } else {
                    row.line.clone()
                };
                self.cell(out, &message, false)?;
            }
            _ => {
                let parser = engine.field_parser().map(|p| &**p);
                let time = crate::fields::line_timestamp(parser, &row.line, Default::default())
                    .map(|(ms, _)| iso_time(ms))
                    .unwrap_or_default();
                self.cell(out, &time, false)?;
                let level = match engine.level_of(idx) {
                    crate::log_level::LogLevel::Unknown => "",
                    level => level.name(),
                };
                self.cell(out, level, false)?;
                self.cell(out, &row.line, false)?;
            }
        }
        out.write_all(b"\r\n")
    }

    fn end(&mut self, _: &mut dyn Write, _: &TailEngine) -> std::io::Result<()> {
        Ok(())
    }
}

/// The lines of an export: visible positions (read through the view), or a list.
#[derive(Debug)]
enum Rows {
    Visible(usize),
    List(Vec<usize>),
}

/// An export being written.
pub struct ExportJob {
    pub path: PathBuf,
    rows: Rows,
    next: usize,
    end: usize,
    out: Option<BufWriter<File>>,
    writer: Box<dyn RowWriter>,
    started: bool,
    /// Lines written.
    pub lines: usize,
    /// The lines past the format's cap were left out.
    pub capped: bool,
    pub error: Option<String>,
    pub done: bool,
    pub cancelled: bool,
}

impl std::fmt::Debug for ExportJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportJob")
            .field("path", &self.path)
            .field("next", &self.next)
            .field("end", &self.end)
            .field("lines", &self.lines)
            .finish()
    }
}

/// How many lines `scope` holds on `engine` now.
pub fn scope_len(engine: &TailEngine, scope: Scope) -> usize {
    match scope {
        Scope::Visible => engine.visible_lines(),
        Scope::Selected => engine.selected_lines().len(),
        Scope::Matches => engine.search_matches.len(),
    }
}

impl ExportJob {
    /// A job writing `scope` of `engine` to `path` with `writer`, at most `max` lines.
    pub fn start(
        engine: &TailEngine,
        scope: Scope,
        path: PathBuf,
        writer: Box<dyn RowWriter>,
        max: Option<usize>,
    ) -> std::io::Result<Self> {
        let rows = match scope {
            Scope::Visible => Rows::Visible(engine.visible_lines()),
            Scope::Selected => Rows::List(engine.selected_lines()),
            Scope::Matches => Rows::List(engine.search_matches.clone()),
        };
        let total = match &rows {
            Rows::Visible(n) => *n,
            Rows::List(list) => list.len(),
        };
        let end = max.map_or(total, |m| total.min(m));
        let file = crate::paths::create_export_file(&path)?;
        Ok(Self {
            path,
            rows,
            next: 0,
            end,
            out: Some(BufWriter::with_capacity(1 << 20, file)),
            writer,
            started: false,
            lines: 0,
            capped: end < total,
            error: None,
            done: false,
            cancelled: false,
        })
    }

    pub fn progress(&self) -> f32 {
        if self.end == 0 {
            1.0
        } else {
            self.next as f32 / self.end as f32
        }
    }

    /// Writes for at most `budget`; true once finished.
    pub fn step(&mut self, engine: &TailEngine, budget: Duration) -> bool {
        if self.done {
            return true;
        }
        let started = Instant::now();
        let result = (|| -> std::io::Result<bool> {
            let out = self.out.as_mut().expect("open while running");
            if !self.started {
                self.writer.begin(out, engine)?;
                self.started = true;
            }
            while self.next < self.end {
                let idx = match &self.rows {
                    Rows::Visible(_) => engine.line_at(self.next),
                    Rows::List(list) => list.get(self.next).copied(),
                };
                self.next += 1;
                if let Some(row) = idx.and_then(|i| engine.get_row(i).map(|r| (i, r))) {
                    self.writer.row(out, engine, row.0, &row.1)?;
                    self.lines += 1;
                }
                if self.next.is_multiple_of(256) && started.elapsed() >= budget {
                    return Ok(false);
                }
            }
            self.writer.end(out, engine)?;
            out.flush()?;
            Ok(true)
        })();
        match result {
            Ok(false) => false,
            Ok(true) => {
                self.out = None;
                self.done = true;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.out = None;
                self.done = true;
                true
            }
        }
    }

    /// Stops the export; the file written so far is removed.
    pub fn cancel(&mut self) {
        if !self.done {
            self.out = None;
            let _ = std::fs::remove_file(&self.path);
            self.cancelled = true;
            self.done = true;
        }
    }

    /// Runs the job to its end at once (small exports, tests).
    pub fn run(mut self, engine: &TailEngine) -> Self {
        while !self.step(engine, Duration::from_secs(3600)) {}
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(value: &str, options: CsvOptions) -> String {
        let mut out = Vec::new();
        write_cell(&mut out, &mut String::new(), value, options).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn cells_are_quoted_and_protected() {
        let o = CsvOptions::default();
        assert_eq!(cell("plain", o), "plain");
        assert_eq!(cell("a,b", o), "\"a,b\"");
        assert_eq!(cell("say \"hi\"", o), "\"say \"\"hi\"\"\"");
        assert_eq!(cell("two\nlines", o), "\"two\nlines\"");
        assert_eq!(cell("=cmd|' /C calc'!A0", o), "'=cmd|' /C calc'!A0");
        assert_eq!(cell("-5", o), "'-5");
        assert_eq!(cell("@x", o), "'@x");
        assert_eq!(cell("+1", o), "'+1");
        let off = CsvOptions {
            protect: false,
            ..o
        };
        assert_eq!(cell("-5", off), "-5");
        let semi = CsvOptions { sep: b';', ..o };
        assert_eq!(cell("a,b", semi), "a,b");
        assert_eq!(cell("a;b", semi), "\"a;b\"");
    }

    #[test]
    fn iso_times_and_names() {
        let day = crate::timestamp::date_to_days(2026, 9, 28).unwrap() * 86_400_000;
        let at = day + 14 * 3_600_000 + 2 * 60_000 + 11_000;
        assert_eq!(iso_time(at), "2026-09-28T14:02:11");
        assert_eq!(iso_time(at + 123), "2026-09-28T14:02:11.123");
        for f in [Format::Text, Format::Csv, Format::Html] {
            assert_eq!(Format::from_name(f.name()), Some(f));
        }
        for sep in *b",;\t" {
            assert_eq!(separator_from_name(separator_name(sep)), Some(sep));
        }
    }
}

/// How Copy as excerpt wraps the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ExcerptFlavour {
    /// A Markdown fenced block under a bold header.
    #[default]
    Markdown,
    /// A Jira `{code}` block under a `*bold*` header.
    Jira,
    /// A Slack code block under a `*bold*` header.
    Slack,
}

impl ExcerptFlavour {
    pub const ALL: [ExcerptFlavour; 3] = [Self::Markdown, Self::Jira, Self::Slack];

    /// The name saved in `excerpt_format=`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Jira => "jira",
            Self::Slack => "slack",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The next one (the settings' cycle).
    pub fn next(self) -> Self {
        match self {
            Self::Markdown => Self::Jira,
            Self::Jira => Self::Slack,
            Self::Slack => Self::Markdown,
        }
    }
}

/// Most bytes an excerpt holds (the clipboard's limit for the bookmark report too).
pub const EXCERPT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// The rows `lines` (0-based, in order) of `engine` as an excerpt for a ticket: a header
/// with the stream name, the line range and the time span of the first and last timed
/// rows, then the rows in a code block of `flavour`. Rows past `EXCERPT_MAX_BYTES` are
/// left out and the header says so.
pub fn excerpt(engine: &TailEngine, lines: &[usize], flavour: ExcerptFlavour) -> Option<String> {
    let (&first, &last) = (lines.first()?, lines.last()?);
    let parser = engine.field_parser().map(|p| p.as_ref());
    let time_of = |line: usize| {
        let text = engine.get_line(line)?;
        crate::fields::line_timestamp(parser, &text, crate::timestamp::FormatHint::default())
            .map(|(ms, _)| ms)
    };
    let first_ts = lines.iter().take(50).find_map(|&l| time_of(l));
    let last_ts = lines.iter().rev().take(50).find_map(|&l| time_of(l));
    let mut body = String::new();
    let mut cut = 0usize;
    for &line in lines {
        let Some(text) = engine.get_line(line) else {
            continue;
        };
        let text = crate::ansi::strip(&text);
        if body.len() + text.len() + 1 > EXCERPT_MAX_BYTES {
            cut += 1;
            continue;
        }
        body.push_str(&text);
        body.push('\n');
    }
    let name = crate::find_all::stream_name(engine);
    let range = if first == last {
        format!("line {}", first + 1)
    } else {
        format!("lines {}-{}", first + 1, last + 1)
    };
    let mut header = format!("{name}, {range}");
    match (first_ts, last_ts) {
        (Some(a), Some(b)) if a != b => header.push_str(&format!(
            ", {} -> {}",
            crate::timestamp::format_millis(a),
            crate::timestamp::format_millis(b)
        )),
        (Some(a), _) => header.push_str(&format!(", {}", crate::timestamp::format_millis(a))),
        _ => {}
    }
    if cut > 0 {
        header.push_str(&format!(" ({cut} rows left out)"));
    }
    Some(match flavour {
        ExcerptFlavour::Markdown => format!("**{header}**\n\n```\n{body}```\n"),
        ExcerptFlavour::Jira => format!("*{header}*\n{{code}}\n{body}{{code}}\n"),
        ExcerptFlavour::Slack => format!("*{header}*\n```\n{body}```\n"),
    })
}

#[cfg(test)]
mod excerpt_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn an_excerpt_names_the_stream_the_lines_and_the_time_span() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            "2026-10-29 14:02:05 INFO a\n2026-10-29 14:02:06 WARN b\n2026-10-29 14:02:07 ERROR c\n"
        )
        .unwrap();
        f.flush().unwrap();
        let e = TailEngine::open(f.path()).unwrap();
        let name = crate::find_all::stream_name(&e);
        let md = excerpt(&e, &[0, 1, 2], ExcerptFlavour::Markdown).unwrap();
        assert_eq!(
            md,
            format!(
                "**{name}, lines 1-3, 2026-10-29 14:02:05 -> 2026-10-29 14:02:07**\n\n```\n\
                 2026-10-29 14:02:05 INFO a\n2026-10-29 14:02:06 WARN b\n\
                 2026-10-29 14:02:07 ERROR c\n```\n"
            )
        );
        let jira = excerpt(&e, &[2], ExcerptFlavour::Jira).unwrap();
        assert!(jira.starts_with(&format!(
            "*{name}, line 3, 2026-10-29 14:02:07*\n{{code}}\n"
        )));
        assert!(jira.ends_with("{code}\n"));
        assert_eq!(
            ExcerptFlavour::from_name("Slack"),
            Some(ExcerptFlavour::Slack)
        );
        assert!(excerpt(&e, &[], ExcerptFlavour::Slack).is_none());
    }
}
