// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The record outputs of print mode (`--output json|logfmt|csv|template`): each line that
//! passes the filters, the same lines `text` prints, written as a record with its file,
//! line number, level, time, text and the fields the parser reads (the window's parser:
//! detected from the first lines of the file, or `--format NAME`).

use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use crate::cli::PrintOutput;
use crate::fields::{FieldParser, FieldSpans, ParserChoice};
use crate::tail_engine::NO_TIMESTAMP;

/// How an input's fields are read.
#[derive(Debug, Clone)]
pub enum ParserMode {
    /// Text output: no field is read.
    Off,
    /// Detected from the first lines, as the window does.
    Detect,
    /// `--format NAME`.
    Forced(ParserChoice),
}

impl ParserMode {
    /// The mode for `output` and `--format NAME`; `Err` names an unknown format.
    pub fn of(output: PrintOutput, format: Option<&str>) -> Result<Self, String> {
        if output == PrintOutput::Text {
            return Ok(ParserMode::Off);
        }
        let Some(name) = format.map(str::trim).filter(|n| !n.is_empty()) else {
            return Ok(ParserMode::Detect);
        };
        let choice = match name.to_ascii_lowercase().as_str() {
            "tsv" => ParserChoice::Delimited {
                sep: b'\t',
                header: true,
            },
            "regex" | "auto" | "off" => return Err(format!("unknown format '{name}'")),
            _ => match ParserChoice::from_name(name, "") {
                Some(choice) => choice,
                None => {
                    let library = crate::log_format::library();
                    let found = library
                        .formats
                        .iter()
                        .find(|f| f.name().eq_ignore_ascii_case(name))
                        .map(|f| f.name().to_string());
                    match found {
                        Some(found) => ParserChoice::Format(found),
                        None => return Err(format!("unknown format '{name}'")),
                    }
                }
            },
        };
        Ok(ParserMode::Forced(choice))
    }

    /// The parser of a file whose first lines are `head`, or `None` (then the first line
    /// read decides, for `Detect`).
    pub fn parser_for(&self, file: Option<&Path>, head: &[&str]) -> Option<Arc<FieldParser>> {
        let first = head.iter().find(|l| !l.trim().is_empty()).copied();
        let choice = match self {
            ParserMode::Off => return None,
            ParserMode::Forced(choice) => choice.clone(),
            ParserMode::Detect => detect(file, head)?,
        };
        choice.build_for(first).ok().flatten()
    }
}

/// The parser the window would choose for these first lines: a log format (the file
/// name's own first), JSON, logfmt, then a table.
fn detect(file: Option<&Path>, head: &[&str]) -> Option<ParserChoice> {
    if head.is_empty() {
        return None;
    }
    let file_name = file.and_then(|f| f.file_name()).and_then(|n| n.to_str());
    crate::log_format::detect(&crate::log_format::library(), file_name, head)
        .map(|f| ParserChoice::Format(f.name().to_string()))
        .or_else(|| crate::fields::detect(head.iter().copied()))
        .or_else(|| {
            let ext = file
                .and_then(|f| f.extension())
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            crate::fields::detect_table(head, ext.as_deref())
        })
}

/// The first lines of the file at `path` (at most `fields::DETECT_LINES`, from its first
/// 64 KB), for the detection.
pub fn head_lines(path: &Path) -> Vec<String> {
    use std::io::Read;
    let mut buf = vec![0u8; 64 * 1024];
    let n = crate::file_source::open_file_shared(path)
        .and_then(|mut f| f.read(&mut buf))
        .unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..n]);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    // The last line may be cut by the 64 KB.
    if n == buf.len() {
        lines.pop();
    }
    lines.truncate(crate::fields::DETECT_LINES);
    lines
}

/// One line to write as a record.
pub struct Line<'a> {
    pub file: &'a str,
    pub number: u64,
    /// The text without escape sequences (with the truncation marker when cut).
    pub text: &'a str,
    pub ts: i64,
    /// A context line (`--context`), not a match.
    pub context: bool,
    pub parser: Option<&'a FieldParser>,
}

/// A piece of a `--template`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Name(String),
}

/// `{name}` places, `{{` and `}}` braces.
fn parse_template(t: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if closed {
                    if !text.is_empty() {
                        out.push(Piece::Text(std::mem::take(&mut text)));
                    }
                    out.push(Piece::Name(name.trim().to_string()));
                } else {
                    text.push('{');
                    text.push_str(&name);
                }
            }
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    out
}

/// Writes the lines as records of one format.
pub struct RecordWriter {
    output: PrintOutput,
    template: Vec<Piece>,
    /// The CSV columns of the fields, from the first record (its parser's names).
    csv_fields: Option<Vec<String>>,
    spans: FieldSpans,
    cell: String,
    level_window: usize,
}

impl RecordWriter {
    /// `None` for text output.
    pub fn new(output: PrintOutput, template: Option<&str>, level_window: usize) -> Option<Self> {
        (output != PrintOutput::Text).then(|| Self {
            output,
            template: parse_template(template.unwrap_or("")),
            csv_fields: None,
            spans: FieldSpans::new(),
            cell: String::new(),
            level_window,
        })
    }

    /// Writes `line` as one record and its line break.
    pub fn write(&mut self, out: &mut impl Write, line: &Line<'_>) -> io::Result<()> {
        let level = crate::fields::line_level(line.parser, line.text, self.level_window).name();
        let time = (line.ts != NO_TIMESTAMP).then(|| crate::export::iso_time(line.ts));
        let mut fields: Vec<(String, String)> = Vec::new();
        if let Some(parser) = line.parser {
            if crate::fields::scan(parser, line.text, &mut self.spans) {
                for f in self.spans.iter(line.text) {
                    if !fields.iter().any(|(k, _)| k == f.key) {
                        fields.push((f.key.to_string(), f.value().into_owned()));
                    }
                }
            }
        }
        match self.output {
            PrintOutput::Text => {
                out.write_all(line.text.as_bytes())?;
            }
            PrintOutput::Json => {
                let s = |t: &str| serde_json::to_string(t).unwrap_or_default();
                write!(out, "{{\"file\":{},\"line\":{}", s(line.file), line.number)?;
                if !level.is_empty() {
                    write!(out, ",\"level\":{}", s(level))?;
                }
                if let Some(time) = &time {
                    write!(out, ",\"time\":{}", s(time))?;
                }
                write!(out, ",\"text\":{}", s(line.text))?;
                if !fields.is_empty() {
                    out.write_all(b",\"fields\":{")?;
                    for (i, (k, v)) in fields.iter().enumerate() {
                        if i > 0 {
                            out.write_all(b",")?;
                        }
                        write!(out, "{}:{}", s(k), s(v))?;
                    }
                    out.write_all(b"}")?;
                }
                if line.context {
                    out.write_all(b",\"context\":true")?;
                }
                out.write_all(b"}")?;
            }
            PrintOutput::Logfmt => {
                let mut pairs: Vec<(&str, &str)> = vec![("file", line.file)];
                let number = line.number.to_string();
                pairs.push(("line", &number));
                if let Some(time) = &time {
                    pairs.push(("time", time));
                }
                if !level.is_empty() {
                    pairs.push(("level", level));
                }
                if fields.is_empty() {
                    pairs.push(("text", line.text));
                } else {
                    for (k, v) in &fields {
                        // A key written already (`level`, `time`...) is not repeated.
                        if !pairs.iter().any(|(p, _)| p == k) {
                            pairs.push((k, v));
                        }
                    }
                }
                if line.context {
                    pairs.push(("context", "true"));
                }
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.write_all(b" ")?;
                    }
                    write!(out, "{}={}", logfmt_key(k), logfmt_value(v))?;
                }
            }
            PrintOutput::Csv => {
                let opts = crate::export::CsvOptions {
                    sep: b',',
                    bom: false,
                    protect: true,
                };
                if self.csv_fields.is_none() {
                    let names: Vec<String> = fields.iter().map(|(k, _)| k.clone()).collect();
                    let mut header: Vec<&str> = vec!["file", "line", "time", "level"];
                    header.extend(names.iter().map(String::as_str));
                    header.push("text");
                    for (i, h) in header.iter().enumerate() {
                        if i > 0 {
                            out.write_all(b",")?;
                        }
                        crate::export::write_cell(out, &mut self.cell, h, opts)?;
                    }
                    out.write_all(b"\n")?;
                    self.csv_fields = Some(names);
                }
                let number = line.number.to_string();
                let mut row: Vec<&str> =
                    vec![line.file, &number, time.as_deref().unwrap_or(""), level];
                if let Some(names) = &self.csv_fields {
                    for name in names {
                        row.push(
                            fields
                                .iter()
                                .find(|(k, _)| k == name)
                                .map_or("", |(_, v)| v.as_str()),
                        );
                    }
                }
                row.push(line.text);
                for (i, cell) in row.iter().enumerate() {
                    if i > 0 {
                        out.write_all(b",")?;
                    }
                    crate::export::write_cell(out, &mut self.cell, cell, opts)?;
                }
            }
            PrintOutput::Template => {
                for piece in &self.template {
                    match piece {
                        Piece::Text(t) => out.write_all(t.as_bytes())?,
                        Piece::Name(name) => {
                            let value: std::borrow::Cow<'_, str> = match name.as_str() {
                                "file" => line.file.into(),
                                "line" => line.number.to_string().into(),
                                "time" => time.as_deref().unwrap_or("").into(),
                                "level" => level.into(),
                                "text" => line.text.into(),
                                key => fields
                                    .iter()
                                    .find(|(k, _)| k == key)
                                    .map_or("", |(_, v)| v.as_str())
                                    .into(),
                            };
                            out.write_all(value.as_bytes())?;
                        }
                    }
                }
            }
        }
        out.write_all(b"\n")
    }
}

/// A logfmt key: the characters a key cannot hold become `_`.
fn logfmt_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_whitespace() || c == '=' || c == '"' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// A logfmt value, quoted (with `\"`, `\\`, `\n` escapes) when it holds a space, `=`, a
/// quote or a control character, or is empty.
fn logfmt_value(value: &str) -> String {
    let plain = !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c == '=' || c == '"' || c == '\\' || c.is_control());
    if plain {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(output: PrintOutput, template: Option<&str>, text: &str, context: bool) -> String {
        let parser = ParserChoice::Logfmt.build().unwrap();
        let mut w = RecordWriter::new(output, template, 96).unwrap();
        let mut out = Vec::new();
        w.write(
            &mut out,
            &Line {
                file: "app.log",
                number: 7,
                text,
                ts: crate::timestamp::date_to_days(2026, 10, 29).unwrap() * 86_400_000
                    + 3_600_000
                    + 250,
                context,
                parser: parser.as_deref(),
            },
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn templates_parse_names_and_braces() {
        assert_eq!(
            parse_template("{time} {{x}} {msg"),
            vec![Piece::Name("time".into()), Piece::Text(" {x} {msg".into())]
        );
        let line = "level=error status=503 msg=\"card declined\"";
        assert_eq!(
            record(
                PrintOutput::Template,
                Some("{line}|{status}|{missing}|{msg}"),
                line,
                false
            ),
            "7|503||card declined\n"
        );
    }

    #[test]
    fn json_has_stable_keys_and_the_fields() {
        let line = "level=error status=503 msg=\"card declined\"";
        let out = record(PrintOutput::Json, None, line, true);
        let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["file"], "app.log");
        assert_eq!(v["line"], 7);
        assert_eq!(v["level"], "ERROR");
        assert_eq!(v["time"], "2026-10-29T01:00:00.250");
        assert_eq!(v["text"], line);
        assert_eq!(v["fields"]["status"], "503");
        assert_eq!(v["context"], true);
        assert!(out.starts_with("{\"file\":"), "key order: {out}");
        let plain = record(PrintOutput::Json, None, line, false);
        assert!(!plain.contains("context"));
    }

    #[test]
    fn logfmt_quotes_what_needs_it() {
        let line = "level=error msg=\"card declined\"";
        assert_eq!(
            record(PrintOutput::Logfmt, None, line, false),
            "file=app.log line=7 time=2026-10-29T01:00:00.250 level=ERROR msg=\"card declined\"\n"
        );
        assert_eq!(logfmt_value(""), "\"\"");
        assert_eq!(logfmt_value("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn modes_of_the_command_line() {
        assert!(matches!(
            ParserMode::of(PrintOutput::Text, Some("json")),
            Ok(ParserMode::Off)
        ));
        assert!(matches!(
            ParserMode::of(PrintOutput::Json, None),
            Ok(ParserMode::Detect)
        ));
        assert!(matches!(
            ParserMode::of(PrintOutput::Csv, Some("tsv")),
            Ok(ParserMode::Forced(ParserChoice::Delimited {
                sep: b'\t',
                ..
            }))
        ));
        assert!(ParserMode::of(PrintOutput::Json, Some("nope")).is_err());
        let detect = ParserMode::Detect;
        assert!(detect
            .parser_for(None, &[r#"{"a":1}"#, r#"{"a":2}"#])
            .is_some_and(|p| matches!(*p, FieldParser::Json)));
        assert!(detect.parser_for(None, &["plain text"]).is_none());
    }
}
