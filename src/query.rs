// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The query language of the Query tab: a pipeline of stages joined by `|`, read left to
//! right over the lines a stream shows.
//!
//! ```text
//! where status>=500 | stats count() as errors, p95(duration_ms) by path | sort -errors | head 10
//! ```
//!
//! Stages: `where <filter expression>` (the syntax of `filter_expr`), `parse /regex/` (its
//! named groups become fields), `timeslice 5m` (adds `_slice`), `fields a, b`, `stats
//! f(x) [as name], … [by k1, k2]`, `sort [-]col, …`, `head N`. `where`, `parse` and
//! `timeslice` act on lines, so they come before `stats`; `fields`, `sort` and `head` work
//! on either side. Fields are the stream parser's, the `parse` captures, and the built-in
//! `line`, `ts`, `level`, `text` and `_slice`.
//!
//! A query runs over the lines once (`Executor::push`, then `finish`) with bounded memory:
//! at most `MAX_ROWS` rows or `MAX_GROUPS` groups, percentiles from `field_stats`' log
//! histogram.

use std::borrow::Cow;
use std::collections::HashMap;

use regex::Regex;

use crate::field_stats::{parse_number, NumHist};
use crate::fields::{FieldParser, FieldSpans};
use crate::filter_expr::{FilterExpr, Toggles};
use crate::tail_engine::NO_TIMESTAMP;

/// Longest query text, in bytes.
pub const MAX_QUERY_BYTES: usize = 64 * 1024;
/// Most stages of a query.
pub const MAX_STAGES: usize = 16;
/// Most rows of a result, and most groups of a `stats`.
pub const MAX_ROWS: usize = 100_000;
pub const MAX_GROUPS: usize = 100_000;
/// Distinct values one `dc` counts exactly, and all of them together.
const MAX_DISTINCT: usize = 100_000;
const MAX_DISTINCT_TOTAL: usize = 1_000_000;

/// Why a query does not run: the stage (1-based, 0 for the whole query), the byte where
/// it goes wrong, an i18n key and a detail in the query's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub stage: usize,
    pub pos: usize,
    pub key: &'static str,
    pub detail: String,
}

impl QueryError {
    fn new(stage: usize, pos: usize, key: &'static str, detail: impl Into<String>) -> Self {
        Self {
            stage,
            pos,
            key,
            detail: detail.into(),
        }
    }
}

/// An aggregate function of `stats`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Func {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    /// Distinct values.
    Dc,
    /// A percentile, 0..=100.
    Pct(f64),
    First,
    Last,
}

impl Func {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "count" => Func::Count,
            "sum" => Func::Sum,
            "avg" | "mean" => Func::Avg,
            "min" => Func::Min,
            "max" => Func::Max,
            "dc" | "distinct" => Func::Dc,
            "first" => Func::First,
            "last" => Func::Last,
            "median" => Func::Pct(50.0),
            n => {
                let p: f64 = n.strip_prefix('p')?.parse().ok()?;
                if !(0.0..=100.0).contains(&p) {
                    return None;
                }
                Func::Pct(p)
            }
        })
    }

    fn needs_arg(self) -> bool {
        self != Func::Count
    }
}

/// One aggregate of `stats`: the function, its field, the column name.
#[derive(Debug, Clone)]
pub struct Agg {
    pub func: Func,
    pub arg: Option<String>,
    pub name: String,
}

/// A stage of the pipeline.
#[derive(Debug, Clone)]
pub enum Stage {
    Where(FilterExpr),
    Parse(Regex),
    Timeslice(i64),
    Fields(Vec<String>),
    Stats {
        aggs: Vec<Agg>,
        by: Vec<String>,
    },
    /// Columns, descending when true.
    Sort(Vec<(String, bool)>),
    Head(usize),
}

/// A parsed query.
#[derive(Debug, Clone)]
pub struct Query {
    pub text: String,
    pub stages: Vec<Stage>,
    toggles: Toggles,
}

/// Splits `text` at the `|` that join stages (not inside quotes or `/…/`), with each
/// stage's byte offset.
fn split_stages(text: &str) -> Vec<(usize, &str)> {
    let b = text.as_bytes();
    let (mut out, mut start, mut i) = (Vec::new(), 0usize, 0usize);
    let (mut quote, mut slash) = (false, false);
    // A `/` opens a regex only where a token starts, as in `filter_expr` (so the slashes
    // of `path=/api/x` are text).
    let mut token_start = true;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\\' if quote || slash => i += 1,
            b'"' if !slash => quote = !quote,
            b'/' if slash => slash = false,
            b'/' if !quote && token_start => slash = true,
            b'|' if !quote && !slash => {
                out.push((start, &text[start..i]));
                start = i + 1;
            }
            _ => {}
        }
        token_start = !quote && !slash && (c.is_ascii_whitespace() || matches!(c, b'(' | b'|'));
        i += 1;
    }
    out.push((start, &text[start..]));
    out
}

/// A field name: letters, digits, `_`, `.`, `@`, `-` (not first).
fn is_name(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_alphanumeric() || f == '_' || f == '@')
        && c.all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '.' | '@' | '-'))
}

fn names(list: &str, stage: usize, at: usize) -> Result<Vec<String>, QueryError> {
    let mut out = Vec::new();
    for part in list.split(',') {
        let name = part.trim();
        if !is_name(name) {
            return Err(QueryError::new(stage, at, "query_err_name", name));
        }
        out.push(name.to_string());
    }
    Ok(out)
}

/// `5m`, `30s`, `1h`, `1d`, `500ms` in milliseconds.
pub fn parse_duration(text: &str) -> Option<i64> {
    let t = text.trim();
    let (num, unit) = t.split_at(t.find(|c: char| !c.is_ascii_digit())?);
    let n: i64 = num.parse().ok().filter(|&n| n > 0)?;
    let ms = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" | "min" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return None,
    };
    n.checked_mul(ms)
}

fn parse_stats(body: &str, stage: usize, at: usize) -> Result<Stage, QueryError> {
    // `… by k1, k2`: the last ` by ` outside parentheses.
    let (list, by) = match body.rfind(" by ") {
        Some(i) if body[..i].matches('(').count() == body[..i].matches(')').count() => {
            (&body[..i], names(&body[i + 4..], stage, at + i + 4)?)
        }
        _ => (body, Vec::new()),
    };
    let mut aggs = Vec::new();
    let mut offset = 0usize;
    for part in list.split(',') {
        let pos = at + offset;
        offset += part.len() + 1;
        let part = part.trim();
        let (call, alias) = match part.split_once(" as ") {
            Some((c, a)) => (c.trim(), Some(a.trim())),
            None => (part, None),
        };
        let open = call
            .find('(')
            .filter(|_| call.ends_with(')'))
            .ok_or_else(|| QueryError::new(stage, pos, "query_err_aggregate", call))?;
        let fname = call[..open].trim().to_ascii_lowercase();
        let func = Func::parse(&fname)
            .ok_or_else(|| QueryError::new(stage, pos, "query_err_function", &call[..open]))?;
        let arg = call[open + 1..call.len() - 1].trim();
        let arg = if arg.is_empty() {
            if func.needs_arg() {
                return Err(QueryError::new(stage, pos, "query_err_argument", call));
            }
            None
        } else {
            if !is_name(arg) {
                return Err(QueryError::new(stage, pos, "query_err_name", arg));
            }
            Some(arg.to_string())
        };
        let name = match alias {
            Some(a) if is_name(a) => a.to_string(),
            Some(a) => return Err(QueryError::new(stage, pos, "query_err_name", a)),
            None => call.replace(' ', ""),
        };
        aggs.push(Agg { func, arg, name });
    }
    Ok(Stage::Stats { aggs, by })
}

impl Query {
    /// Parses `text`. `fields`: the stream has a parser (`where` reads `key=value` as a
    /// field term); `case_sensitive` and `is_regex` are the stream's filter toggles.
    pub fn parse(
        text: &str,
        fields: bool,
        case_sensitive: bool,
        is_regex: bool,
    ) -> Result<Self, QueryError> {
        if text.len() > MAX_QUERY_BYTES {
            return Err(QueryError::new(
                0,
                MAX_QUERY_BYTES,
                "query_err_too_long",
                "",
            ));
        }
        let toggles = Toggles {
            case_sensitive,
            is_regex,
            fields,
        };
        let parts = split_stages(text);
        if parts.len() > MAX_STAGES {
            return Err(QueryError::new(
                0,
                parts[MAX_STAGES].0,
                "query_err_too_many_stages",
                "",
            ));
        }
        let mut stages = Vec::new();
        // Past a `stats` or `sort` the stages act on the table: no more line stages.
        let mut blocked = false;
        let mut stats_seen = false;
        for (i, (at, raw)) in parts.iter().enumerate() {
            let n = i + 1;
            let lead = raw.len() - raw.trim_start().len();
            let part = raw.trim();
            let at = at + lead;
            if part.is_empty() {
                if parts.len() == 1 {
                    // An empty query: every line.
                    break;
                }
                return Err(QueryError::new(n, at, "query_err_empty_stage", ""));
            }
            let (word, rest) = part.split_once(char::is_whitespace).unwrap_or((part, ""));
            let sep = part[word.len()..].chars().next().map_or(0, char::len_utf8);
            let rest_at = at + word.len() + sep + (rest.len() - rest.trim_start().len());
            let rest = rest.trim();
            let line_stage = matches!(word, "where" | "parse" | "timeslice");
            if line_stage && blocked {
                return Err(QueryError::new(n, at, "query_err_after_stats", word));
            }
            if word == "stats" && stats_seen {
                return Err(QueryError::new(n, at, "query_err_two_stats", word));
            }
            let stage = match word {
                "where" => match FilterExpr::compile(rest, toggles) {
                    Ok(Some(e)) => Stage::Where(e),
                    Ok(None) => {
                        return Err(QueryError::new(n, rest_at, "query_err_empty_stage", word))
                    }
                    Err(e) => {
                        let detail = match e.detail {
                            Some(d) => format!("{} ({d})", e.key),
                            None => e.key.to_string(),
                        };
                        return Err(QueryError::new(
                            n,
                            rest_at + e.pos,
                            "query_err_where",
                            detail,
                        ));
                    }
                },
                "parse" => {
                    let pattern = rest
                        .strip_prefix('/')
                        .and_then(|r| r.strip_suffix('/'))
                        .ok_or_else(|| QueryError::new(n, rest_at, "query_err_parse_form", rest))?
                        .replace("\\/", "/");
                    let re = Regex::new(&pattern).map_err(|e| {
                        QueryError::new(n, rest_at, "query_err_regex", e.to_string())
                    })?;
                    if re.capture_names().flatten().next().is_none() {
                        return Err(QueryError::new(n, rest_at, "query_err_no_groups", rest));
                    }
                    Stage::Parse(re)
                }
                "timeslice" => Stage::Timeslice(
                    parse_duration(rest)
                        .ok_or_else(|| QueryError::new(n, rest_at, "query_err_duration", rest))?,
                ),
                "fields" => Stage::Fields(names(rest, n, rest_at)?),
                "stats" => {
                    blocked = true;
                    stats_seen = true;
                    parse_stats(rest, n, rest_at)?
                }
                "sort" => {
                    blocked = true;
                    let mut cols = Vec::new();
                    for c in rest.split(',') {
                        let c = c.trim();
                        let (name, desc) = match c.strip_prefix('-') {
                            Some(r) => (r.trim(), true),
                            None => (c.strip_prefix('+').unwrap_or(c).trim(), false),
                        };
                        if !is_name(name) {
                            return Err(QueryError::new(n, rest_at, "query_err_name", c));
                        }
                        cols.push((name.to_string(), desc));
                    }
                    Stage::Sort(cols)
                }
                "head" => Stage::Head(
                    rest.parse::<usize>()
                        .ok()
                        .filter(|&h| h > 0)
                        .ok_or_else(|| QueryError::new(n, rest_at, "query_err_number", rest))?,
                ),
                other => return Err(QueryError::new(n, at, "query_err_stage", other)),
            };
            stages.push(stage);
        }
        Ok(Self {
            text: text.to_string(),
            stages,
            toggles,
        })
    }

    /// Whether a stage needs the lines' timestamps (`ts`, `timeslice`).
    pub fn needs_time(&self) -> bool {
        self.mentions(|n| n == "ts" || n == "_slice")
            || self.stages.iter().any(|s| matches!(s, Stage::Timeslice(_)))
    }

    /// Whether a stage names the level.
    pub fn needs_level(&self) -> bool {
        self.mentions(|n| n == "level")
    }

    fn mentions(&self, f: impl Fn(&str) -> bool) -> bool {
        self.stages.iter().any(|s| match s {
            Stage::Fields(n) => n.iter().any(|x| f(x)),
            Stage::Stats { aggs, by } => {
                by.iter().any(|x| f(x)) || aggs.iter().any(|a| a.arg.as_deref().is_some_and(&f))
            }
            Stage::Sort(c) => c.iter().any(|(x, _)| f(x)),
            _ => false,
        }) || !self
            .stages
            .iter()
            .any(|s| matches!(s, Stage::Fields(_) | Stage::Stats { .. }))
    }
}

/// A result table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// The stream line of each row (row results only, for the jump).
    pub lines: Option<Vec<usize>>,
    /// More rows than `MAX_ROWS` (or the `head`) would have followed.
    pub truncated: bool,
    /// Lines read, and values a numeric function could not read as numbers.
    pub lines_read: u64,
    pub skipped_values: u64,
}

impl Table {
    /// The table as CSV (formula-guarded cells).
    pub fn to_csv(&self) -> String {
        let cell = |t: &str| {
            let g = if t.starts_with(['=', '+', '-', '@']) && parse_number(t).is_none() {
                format!("'{t}")
            } else {
                t.to_string()
            };
            if g.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", g.replace('"', "\"\""))
            } else {
                g
            }
        };
        let mut out = self
            .columns
            .iter()
            .map(|c| cell(c))
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        for r in &self.rows {
            out.push_str(&r.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        out
    }

    /// Sorts the rows by `col` (numbers as numbers, then text).
    pub fn sort_by(&mut self, col: usize, desc: bool) {
        let mut order: Vec<usize> = (0..self.rows.len()).collect();
        order.sort_by(|&a, &b| {
            let o = cmp_values(&self.rows[a][col], &self.rows[b][col]);
            if desc {
                o.reverse()
            } else {
                o
            }
        });
        self.rows = order.iter().map(|&i| self.rows[i].clone()).collect();
        if let Some(lines) = &self.lines {
            self.lines = Some(order.iter().map(|&i| lines[i]).collect());
        }
    }
}

fn cmp_values(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_number(a), parse_number(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.cmp(b),
    }
}

/// One line as the executor reads it.
pub struct LineIn<'a> {
    pub idx: usize,
    pub text: &'a str,
    pub level: u8,
    pub ts: i64,
}

/// The state of one aggregate of one group.
#[derive(Debug, Clone)]
enum AggState {
    Count(u64),
    Sum(f64, u64),
    Min(Option<f64>),
    Max(Option<f64>),
    Dc(std::collections::HashSet<Box<str>>, bool),
    Pct(NumHist),
    First(Option<String>),
    Last(Option<String>),
}

impl AggState {
    fn new(f: Func) -> Self {
        match f {
            Func::Count => AggState::Count(0),
            Func::Sum | Func::Avg => AggState::Sum(0.0, 0),
            Func::Min => AggState::Min(None),
            Func::Max => AggState::Max(None),
            Func::Dc => AggState::Dc(Default::default(), false),
            Func::Pct(_) => AggState::Pct(NumHist::default()),
            Func::First => AggState::First(None),
            Func::Last => AggState::Last(None),
        }
    }

    /// Adds a value; returns false when a number was wanted and `v` is none. `dc_left`:
    /// the distinct values every `dc` of the query may still keep.
    fn add(&mut self, v: Option<&str>, dc_left: &mut usize) -> bool {
        let num = || v.and_then(parse_number);
        match self {
            AggState::Count(n) => {
                if v.is_some() {
                    *n += 1;
                }
            }
            AggState::Sum(s, n) => match num() {
                Some(x) => {
                    *s += x;
                    *n += 1;
                }
                None => return v.is_none(),
            },
            AggState::Min(m) => match num() {
                Some(x) => *m = Some(m.map_or(x, |m| m.min(x))),
                None => return v.is_none(),
            },
            AggState::Max(m) => match num() {
                Some(x) => *m = Some(m.map_or(x, |m| m.max(x))),
                None => return v.is_none(),
            },
            AggState::Dc(set, capped) => {
                if let Some(v) = v {
                    if set.contains(v) {
                    } else if set.len() < MAX_DISTINCT && *dc_left > 0 {
                        set.insert(v.into());
                        *dc_left -= 1;
                    } else {
                        *capped = true;
                    }
                }
            }
            AggState::Pct(h) => match num() {
                Some(x) => h.add(x),
                None => return v.is_none(),
            },
            AggState::First(f) => {
                if f.is_none() {
                    *f = v.map(str::to_string);
                }
            }
            AggState::Last(l) => {
                if let Some(v) = v {
                    match l {
                        Some(s) => {
                            s.clear();
                            s.push_str(v);
                        }
                        None => *l = Some(v.to_string()),
                    }
                }
            }
        }
        true
    }

    fn value(&self, f: Func) -> String {
        let num = |x: f64| {
            if x.fract() == 0.0 && x.abs() < 1e15 {
                format!("{x:.0}")
            } else {
                format!("{x:.3}")
            }
        };
        match (self, f) {
            (AggState::Count(n), _) => n.to_string(),
            (AggState::Sum(s, n), Func::Avg) => {
                if *n == 0 {
                    String::new()
                } else {
                    num(s / *n as f64)
                }
            }
            (AggState::Sum(_, 0), _) => String::new(),
            (AggState::Sum(s, _), _) => num(*s),
            (AggState::Min(m) | AggState::Max(m), _) => m.map(num).unwrap_or_default(),
            (AggState::Dc(set, capped), _) => {
                if *capped {
                    format!("≥{}", set.len())
                } else {
                    set.len().to_string()
                }
            }
            (AggState::Pct(h), Func::Pct(p)) => {
                h.percentile(p / 100.0).map(num).unwrap_or_default()
            }
            (AggState::Pct(_), _) => String::new(),
            (AggState::First(v) | AggState::Last(v), _) => v.clone().unwrap_or_default(),
        }
    }
}

/// The values of one line: built-ins, the parser's fields, the `parse` captures.
struct LineCtx<'a> {
    line: &'a LineIn<'a>,
    parser: Option<&'a FieldParser>,
    spans: &'a mut FieldSpans,
    scanned: bool,
    /// `parse` captures: the name (in `names`) and byte range in the line.
    captures: &'a [(usize, (usize, usize))],
    names: &'a [String],
    slice: Option<i64>,
}

impl LineCtx<'_> {
    fn get(&mut self, name: &str) -> Option<Cow<'_, str>> {
        match name {
            "line" => return Some(Cow::Owned((self.line.idx + 1).to_string())),
            "text" => return Some(Cow::Borrowed(self.line.text)),
            "level" => {
                let lvl = crate::log_level::LogLevel::ALL
                    .get((self.line.level as usize).wrapping_sub(1))
                    .map_or("", |l| l.name());
                return Some(Cow::Borrowed(lvl));
            }
            "ts" => {
                return (self.line.ts != NO_TIMESTAMP)
                    .then(|| Cow::Owned(crate::timestamp::format_millis(self.line.ts)))
            }
            "_slice" => {
                return self
                    .slice
                    .map(|s| Cow::Owned(crate::timestamp::format_millis(s)))
            }
            _ => {}
        }
        if let Some((_, (a, b))) = self
            .captures
            .iter()
            .rev()
            .find(|(n, _)| self.names[*n] == name)
        {
            return Some(Cow::Borrowed(&self.line.text[*a..*b]));
        }
        let parser = self.parser?;
        if !self.scanned {
            crate::fields::scan(parser, self.line.text, self.spans);
            self.scanned = true;
        }
        self.spans
            .get(self.line.text, name)
            .map(|f| Cow::Owned(f.value().into_owned()))
    }

    /// The value of `name` into `out` (cleared first, its capacity reused); whether the
    /// line has it.
    fn get_into(&mut self, name: &str, out: &mut String) -> bool {
        out.clear();
        if name == "line" {
            use std::fmt::Write;
            let _ = write!(out, "{}", self.line.idx + 1);
            return true;
        }
        // Parser fields are copied from the spans without a temporary when not decoded.
        if !matches!(name, "text" | "level" | "ts" | "_slice")
            && !self.captures.iter().any(|(n, _)| self.names[*n] == name)
        {
            let Some(parser) = self.parser else {
                return false;
            };
            if !self.scanned {
                crate::fields::scan(parser, self.line.text, self.spans);
                self.scanned = true;
            }
            return match self.spans.get(self.line.text, name) {
                Some(f) => {
                    out.push_str(&f.value());
                    true
                }
                None => false,
            };
        }
        match self.get(name) {
            Some(v) => {
                out.push_str(&v);
                true
            }
            None => false,
        }
    }
}

/// The capture slots of a `parse` stage and its `(group, name)` pairs.
type ParseSlots = (regex::CaptureLocations, Vec<(usize, usize)>);

/// Runs a query over lines pushed in order.
pub struct Executor<'q> {
    query: &'q Query,
    parser: Option<std::sync::Arc<FieldParser>>,
    /// The first `stats` or `sort`: the stages before it act on lines.
    split: usize,
    spans: FieldSpans,
    /// `parse` captures of the current line, the names they refer to, and per `parse`
    /// stage its capture slots and `(group, name)` pairs.
    captures: Vec<(usize, (usize, usize))>,
    names: Vec<String>,
    parse_slots: Vec<Option<ParseSlots>>,
    /// Lines that passed every line stage so far, per `head` before the split.
    heads: Vec<usize>,
    rows: Vec<Vec<String>>,
    lines: Vec<usize>,
    columns: Vec<String>,
    /// Columns added only for a `sort` that names them, dropped at the end.
    extra_columns: usize,
    /// A `sort … | head N` over rows keeps only the first N as it goes.
    top_k: Option<usize>,
    groups: HashMap<Vec<String>, Vec<AggState>>,
    group_order: Vec<Vec<String>>,
    /// Reused per line: the group key and the aggregate values.
    key_buf: Vec<String>,
    val_buf: Vec<(bool, String)>,
    dc_left: usize,
    truncated: bool,
    lines_read: u64,
    skipped_values: u64,
    error: Option<QueryError>,
    done: bool,
}

impl<'q> Executor<'q> {
    pub fn new(query: &'q Query, parser: Option<std::sync::Arc<FieldParser>>) -> Self {
        let split = query
            .stages
            .iter()
            .position(|s| matches!(s, Stage::Stats { .. } | Stage::Sort(_)))
            .unwrap_or(query.stages.len());
        // The columns of row results: the last `fields` before the split, or the line.
        let mut columns = query.stages[..split]
            .iter()
            .rev()
            .find_map(|s| match s {
                Stage::Fields(f) => Some(f.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec!["line".into(), "ts".into(), "level".into(), "text".into()]);
        // A `sort` of rows may name a field the columns lack: read it, drop it later.
        let mut extra_columns = 0;
        let mut top_k = None;
        if let Some(Stage::Sort(cols)) = query.stages.get(split) {
            for (name, _) in cols {
                if !columns.contains(name) {
                    columns.push(name.clone());
                    extra_columns += 1;
                }
            }
            top_k = match query.stages.get(split + 1) {
                Some(Stage::Head(n)) => Some(*n),
                _ => None,
            };
        }
        let mut names: Vec<String> = Vec::new();
        let parse_slots = query
            .stages
            .iter()
            .map(|s| match s {
                Stage::Parse(re) => {
                    let pairs = re
                        .capture_names()
                        .enumerate()
                        .filter_map(|(g, n)| {
                            let n = n?;
                            let id = names.iter().position(|x| x == n).unwrap_or_else(|| {
                                names.push(n.to_string());
                                names.len() - 1
                            });
                            Some((g, id))
                        })
                        .collect();
                    Some((re.capture_locations(), pairs))
                }
                _ => None,
            })
            .collect();
        Self {
            query,
            parser,
            split,
            spans: FieldSpans::new(),
            captures: Vec::new(),
            names,
            parse_slots,
            heads: vec![0; query.stages.len()],
            rows: Vec::new(),
            lines: Vec::new(),
            columns,
            extra_columns,
            top_k,
            groups: HashMap::new(),
            group_order: Vec::new(),
            key_buf: Vec::new(),
            val_buf: Vec::new(),
            dc_left: MAX_DISTINCT_TOTAL,
            truncated: false,
            lines_read: 0,
            skipped_values: 0,
            error: None,
            done: false,
        }
    }

    /// Whether more lines can change the result (false after a `head` filled up or an
    /// error).
    pub fn wants_more(&self) -> bool {
        !self.done
    }

    fn fail(&mut self, stage: usize, key: &'static str, detail: String) {
        self.error = Some(QueryError::new(stage, 0, key, detail));
        self.done = true;
    }

    /// One line, in file order.
    pub fn push(&mut self, line: &LineIn) {
        if self.done {
            return;
        }
        self.lines_read += 1;
        self.captures.clear();
        let toggles = self.query.toggles;
        let mut slice = None;
        // The line stages, up to the first `stats` or `sort`.
        for (i, stage) in self.query.stages[..self.split].iter().enumerate() {
            match stage {
                Stage::Where(expr) => {
                    let mut fields =
                        crate::scan_job::LineFields::new(self.parser.as_deref(), &mut self.spans);
                    if !expr.eval(line.text, toggles, &mut fields) {
                        return;
                    }
                }
                Stage::Parse(re) => {
                    if let Some((locs, pairs)) = self.parse_slots[i].as_mut() {
                        if re.captures_read(locs, line.text).is_some() {
                            for &(g, id) in pairs.iter() {
                                if let Some(range) = locs.get(g) {
                                    self.captures.push((id, range));
                                }
                            }
                        }
                    }
                }
                Stage::Timeslice(w) => {
                    slice = (line.ts != NO_TIMESTAMP).then(|| line.ts.div_euclid(*w) * w);
                }
                Stage::Head(n) => {
                    if self.heads[i] >= *n {
                        // No later line passes the first `n`: no need to read on.
                        self.done = true;
                        self.truncated = true;
                        return;
                    }
                    self.heads[i] += 1;
                }
                Stage::Fields(_) | Stage::Stats { .. } | Stage::Sort(_) => {}
            }
        }
        let mut ctx = LineCtx {
            line,
            parser: self.parser.as_deref(),
            spans: &mut self.spans,
            scanned: false,
            captures: &self.captures,
            names: &self.names,
            slice,
        };
        match self.query.stages.get(self.split) {
            Some(Stage::Stats { aggs, by }) => {
                // The key and the values into reused buffers: no allocation per line
                // unless the group is new.
                self.key_buf.resize_with(by.len(), String::new);
                for (k, out) in by.iter().zip(self.key_buf.iter_mut()) {
                    ctx.get_into(k, out);
                }
                self.val_buf.resize_with(aggs.len(), Default::default);
                for (a, out) in aggs.iter().zip(self.val_buf.iter_mut()) {
                    out.0 = match &a.arg {
                        Some(arg) => ctx.get_into(arg, &mut out.1),
                        None => {
                            out.1.clear();
                            true
                        }
                    };
                }
                if !self.groups.contains_key(&self.key_buf) {
                    if self.groups.len() >= MAX_GROUPS {
                        self.fail(
                            self.split + 1,
                            "query_err_too_many_groups",
                            MAX_GROUPS.to_string(),
                        );
                        return;
                    }
                    self.group_order.push(self.key_buf.clone());
                    self.groups.insert(
                        self.key_buf.clone(),
                        aggs.iter().map(|a| AggState::new(a.func)).collect(),
                    );
                }
                let mut skipped = 0;
                if let Some(states) = self.groups.get_mut(&self.key_buf) {
                    for (st, (present, v)) in states.iter_mut().zip(&self.val_buf) {
                        let v = present.then_some(v.as_str());
                        if !st.add(v, &mut self.dc_left) {
                            skipped += 1;
                        }
                    }
                }
                self.skipped_values += skipped;
            }
            split => {
                let mut row: Vec<String> = Vec::with_capacity(self.columns.len());
                for c in &self.columns {
                    let mut v = String::new();
                    ctx.get_into(c, &mut v);
                    row.push(v);
                }
                self.rows.push(row);
                self.lines.push(line.idx);
                if let Some(Stage::Sort(cols)) = split {
                    match self.top_k {
                        // `sort … | head N`: the first N kept as the lines come.
                        Some(k) if self.rows.len() >= (2 * k).max(1024) => {
                            self.keep_top(cols, k);
                        }
                        None if self.rows.len() > MAX_ROWS => {
                            self.fail(self.split + 1, "query_err_sort_rows", MAX_ROWS.to_string());
                        }
                        _ => {}
                    }
                } else if self.rows.len() >= MAX_ROWS {
                    self.truncated = true;
                    self.done = true;
                }
            }
        }
    }

    /// Sorts the rows by `cols` (stable: equal rows keep their line order) and keeps `k`.
    fn keep_top(&mut self, cols: &[(String, bool)], k: usize) {
        let mut t = Table {
            columns: std::mem::take(&mut self.columns),
            rows: std::mem::take(&mut self.rows),
            lines: Some(std::mem::take(&mut self.lines)),
            ..Table::default()
        };
        for (name, desc) in cols.iter().rev() {
            if let Some(col) = t.columns.iter().position(|c| c == name) {
                stable_sort(&mut t, col, *desc);
            }
        }
        if t.rows.len() > k {
            t.rows.truncate(k);
            self.truncated = true;
        }
        self.columns = t.columns;
        self.rows = t.rows;
        self.lines = t.lines.unwrap_or_default();
        self.lines.truncate(k);
    }

    /// The result, once every line was pushed (or `wants_more` turned false).
    pub fn finish(mut self) -> Result<Table, QueryError> {
        if let Some(e) = self.error.take() {
            return Err(e);
        }
        let mut table = match self.query.stages.get(self.split) {
            Some(Stage::Stats { aggs, by }) => {
                let mut columns = by.clone();
                columns.extend(aggs.iter().map(|a| a.name.clone()));
                let mut rows: Vec<Vec<String>> = self
                    .group_order
                    .iter()
                    .map(|key| {
                        let states = &self.groups[key];
                        let mut row = key.clone();
                        row.extend(states.iter().zip(aggs).map(|(s, a)| s.value(a.func)));
                        row
                    })
                    .collect();
                // Without `by`, one row even when no line matched (`count()` is 0).
                if by.is_empty() && rows.is_empty() {
                    rows.push(
                        aggs.iter()
                            .map(|a| AggState::new(a.func).value(a.func))
                            .collect(),
                    );
                }
                Table {
                    columns,
                    rows,
                    lines: None,
                    ..Table::default()
                }
            }
            _ => Table {
                columns: std::mem::take(&mut self.columns),
                rows: std::mem::take(&mut self.rows),
                lines: Some(std::mem::take(&mut self.lines)),
                ..Table::default()
            },
        };
        table.truncated = self.truncated;
        table.lines_read = self.lines_read;
        table.skipped_values = self.skipped_values;
        // The stages after the split act on the table.
        let start = match self.query.stages.get(self.split) {
            Some(Stage::Stats { .. }) => self.split + 1,
            _ => self.split,
        };
        let mut extra = self.extra_columns;
        for (i, stage) in self.query.stages.iter().enumerate().skip(start) {
            match stage {
                Stage::Sort(cols) => {
                    for (name, desc) in cols.iter().rev() {
                        let col =
                            table
                                .columns
                                .iter()
                                .position(|c| c == name)
                                .ok_or_else(|| {
                                    QueryError::new(i + 1, 0, "query_err_column", name.clone())
                                })?;
                        stable_sort(&mut table, col, *desc);
                    }
                }
                Stage::Head(n) => {
                    if table.rows.len() > *n {
                        table.rows.truncate(*n);
                        if let Some(l) = table.lines.as_mut() {
                            l.truncate(*n);
                        }
                        table.truncated = true;
                    }
                }
                Stage::Fields(names) => {
                    let idx: Vec<usize> = names
                        .iter()
                        .map(|n| {
                            table.columns.iter().position(|c| c == n).ok_or_else(|| {
                                QueryError::new(i + 1, 0, "query_err_column", n.clone())
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    table.columns = names.clone();
                    for r in &mut table.rows {
                        *r = idx.iter().map(|&k| std::mem::take(&mut r[k])).collect();
                    }
                    // The projection decided the columns.
                    extra = 0;
                }
                // The parser refuses these orders; kept for an `Executor` built by hand.
                Stage::Stats { .. } => {
                    return Err(QueryError::new(i + 1, 0, "query_err_two_stats", ""));
                }
                Stage::Where(_) | Stage::Parse(_) | Stage::Timeslice(_) => {
                    return Err(QueryError::new(i + 1, 0, "query_err_after_stats", ""));
                }
            }
        }
        // The columns read only for a sort.
        if extra > 0 {
            let keep = table.columns.len() - extra;
            table.columns.truncate(keep);
            for r in &mut table.rows {
                r.truncate(keep);
            }
        }
        Ok(table)
    }
}

/// A stable sort of the rows by one column.
fn stable_sort(table: &mut Table, col: usize, desc: bool) {
    let mut order: Vec<usize> = (0..table.rows.len()).collect();
    order.sort_by(|&a, &b| {
        let o = cmp_values(&table.rows[a][col], &table.rows[b][col]);
        if desc {
            o.reverse()
        } else {
            o
        }
    });
    let mut old: Vec<Option<Vec<String>>> = std::mem::take(&mut table.rows)
        .into_iter()
        .map(Some)
        .collect();
    table.rows = order
        .iter()
        .map(|&i| old[i].take().unwrap_or_default())
        .collect();
    if let Some(lines) = &table.lines {
        table.lines = Some(order.iter().map(|&i| lines[i]).collect());
    }
}

/// Runs `query` over `lines` (tests, and the job through `Executor`).
pub fn run<'a>(
    query: &Query,
    parser: Option<std::sync::Arc<FieldParser>>,
    lines: impl IntoIterator<Item = LineIn<'a>>,
) -> Result<Table, QueryError> {
    let mut ex = Executor::new(query, parser);
    for l in lines {
        if !ex.wants_more() {
            break;
        }
        ex.push(&l);
    }
    ex.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logfmt() -> std::sync::Arc<FieldParser> {
        crate::fields::ParserChoice::from_name("logfmt", "")
            .unwrap()
            .build()
            .unwrap()
            .unwrap()
    }

    fn access() -> Vec<String> {
        (0..100)
            .map(|i| {
                let status = [200, 200, 200, 404, 500][i % 5];
                format!(
                    "level={} status={status} path=/p{} ms={} user=u{}",
                    if status == 500 { "error" } else { "info" },
                    i % 3,
                    (i * 7) % 100 + 1,
                    i % 7
                )
            })
            .collect()
    }

    fn q(text: &str) -> Query {
        Query::parse(text, true, false, false).unwrap()
    }

    fn table(query: &str, lines: &[String]) -> Table {
        let ins: Vec<LineIn> = lines
            .iter()
            .enumerate()
            .map(|(i, t)| LineIn {
                idx: i,
                text: t,
                level: if t.contains("error") { 5 } else { 3 },
                ts: 60_000 * (i as i64 / 10),
            })
            .collect();
        run(&q(query), Some(logfmt()), ins).unwrap()
    }

    #[test]
    fn stats_by_with_sort_and_head() {
        let t = table(
            "where status>=400 | stats count() as n, max(ms), p50(ms) by status | sort -n",
            &access(),
        );
        assert_eq!(t.columns, ["status", "n", "max(ms)", "p50(ms)"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(&t.rows[0][..2], ["404", "20"]);
        assert_eq!(&t.rows[1][..2], ["500", "20"]);
        assert!(t.lines.is_none());

        let t = table("stats count() by path | sort path | head 2", &access());
        assert_eq!(t.rows, vec![vec!["/p0", "34"], vec!["/p1", "33"]]);
        assert!(t.truncated);

        // Without `by`: one row.
        let t = table(
            "stats count(), dc(user), sum(ms), avg(ms), first(path), last(path)",
            &access(),
        );
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0][0], "100");
        assert_eq!(t.rows[0][1], "7");
        assert_eq!(t.rows[0][4], "/p0");
        assert_eq!(t.rows[0][5], "/p0");
        let sum: usize = (0..100).map(|i| (i * 7) % 100 + 1).sum();
        assert_eq!(t.rows[0][2], sum.to_string());
    }

    #[test]
    fn row_results_fields_parse_timeslice() {
        let t = table(
            "where status=500 | fields line, path, level | head 3",
            &access(),
        );
        assert_eq!(t.columns, ["line", "path", "level"]);
        assert_eq!(t.rows[0], vec!["5", "/p1", "ERROR"]);
        assert_eq!(t.lines.as_deref(), Some(&[4usize, 9, 14][..]));
        assert!(t.truncated);

        let t = table(
            r"parse /user=u(?P<uid>\d+)/ | stats count() by uid | sort uid",
            &access(),
        );
        assert_eq!(t.rows.len(), 7);
        assert_eq!(t.rows[0][0], "0");

        let t = table("timeslice 2m | stats count() by _slice", &access());
        assert_eq!(t.rows.len(), 5);
        assert_eq!(t.rows[0][1], "20");

        // The default columns of rows.
        let t = table("head 1", &access());
        assert_eq!(t.columns, ["line", "ts", "level", "text"]);
        // A sort of rows by a field.
        let t = table("fields ms, line | sort -ms | head 1", &access());
        assert_eq!(t.rows[0][0], "100");
    }

    #[test]
    fn errors_name_the_stage_and_where() {
        let e = |text: &str| Query::parse(text, true, false, false).unwrap_err();
        assert_eq!(e("where a | bogus x").key, "query_err_stage");
        assert_eq!(e("where a | bogus x").stage, 2);
        assert_eq!(e("stats nope(x)").key, "query_err_function");
        assert_eq!(e("stats sum()").key, "query_err_argument");
        assert_eq!(e("stats count() | where a").key, "query_err_after_stats");
        assert_eq!(e("parse /(\\d+)/").key, "query_err_no_groups");
        assert_eq!(e("parse x").key, "query_err_parse_form");
        assert_eq!(e("timeslice 5x").key, "query_err_duration");
        assert_eq!(e("head 0").key, "query_err_number");
        assert_eq!(e("where (a").key, "query_err_where");
        assert_eq!(e("where a ||").key, "query_err_empty_stage");
        let long = "where a | ".repeat(17);
        assert_eq!(e(&long).key, "query_err_too_many_stages");
        // A `|` inside a regex or quotes does not split.
        assert_eq!(q(r#"where /a|b/ | where "x|y""#).stages.len(), 2);
        assert_eq!(q("").stages.len(), 0);
        // A column a later stage names must exist.
        let lines = access();
        let ins = lines.iter().enumerate().map(|(i, t)| LineIn {
            idx: i,
            text: t,
            level: 3,
            ts: 0,
        });
        let err = run(&q("stats count() by path | sort nope"), Some(logfmt()), ins).unwrap_err();
        assert_eq!((err.key, err.stage), ("query_err_column", 2));
    }

    #[test]
    fn group_and_value_bounds_and_csv() {
        let lines: Vec<String> = (0..10).map(|i| format!("id={i} v=x{i}")).collect();
        let t = table("stats sum(v), count() by id", &lines);
        assert_eq!(t.skipped_values, 10, "text values are skipped by sum");
        assert_eq!(t.rows[0][1], "");
        let csv = Table {
            columns: vec!["a".into(), "b".into()],
            rows: vec![
                vec!["=1+1".into(), "x,y".into()],
                vec!["-5".into(), "ok".into()],
            ],
            ..Table::default()
        }
        .to_csv();
        assert_eq!(csv, "a,b\n'=1+1,\"x,y\"\n-5,ok\n");
        assert_eq!(parse_duration("90s"), Some(90_000));
        assert_eq!(parse_duration("1d"), Some(86_400_000));
        assert_eq!(parse_duration("0m"), None);
    }

    #[test]
    fn review_cases() {
        // Slashes inside a word are text, a `|` after them still splits.
        let paths: Vec<String> = (0..10)
            .map(|i| {
                format!(
                    "path=/api/x/{} status={}",
                    i % 2,
                    if i < 4 { 500 } else { 200 }
                )
            })
            .collect();
        let t = table(
            "where path=/api/x/1 | stats count() by status | sort status",
            &paths,
        );
        assert_eq!(t.rows, vec![vec!["200", "3"], vec!["500", "2"]]);
        // A sort may name a field the columns lack; it is read, then dropped.
        let t = table("where status=500 | sort -ms | head 2", &access());
        assert_eq!(t.columns, ["line", "ts", "level", "text"]);
        assert_eq!(t.rows.len(), 2);
        assert!(t.rows[0][3].contains("ms=99"), "{:?}", t.rows[0]);
        // `stats` without `by` over no line: one row.
        let t = table("where status=999 | stats count(), sum(ms)", &access());
        assert_eq!(t.rows, vec![vec!["0", ""]]);
        // Orders refused before any line is read.
        let e = |text: &str| Query::parse(text, true, false, false).unwrap_err().key;
        assert_eq!(e("sort ms | where a"), "query_err_after_stats");
        assert_eq!(e("stats count() | stats count()"), "query_err_two_stats");
        // A multi-byte separator keeps the positions on the text.
        let err = Query::parse("where\u{3000}(a", true, false, false).unwrap_err();
        assert!("where\u{3000}(a".is_char_boundary(err.pos));
    }

    #[test]
    fn sort_head_keeps_the_true_top_past_the_row_cap() {
        let lines: Vec<String> = (0..MAX_ROWS + 5000).map(|i| format!("n={i}")).collect();
        let t = table("fields n | sort -n | head 3", &lines);
        let last = MAX_ROWS + 4999;
        assert_eq!(
            t.rows,
            vec![
                vec![last.to_string()],
                vec![(last - 1).to_string()],
                vec![(last - 2).to_string()]
            ]
        );
        assert!(t.truncated);
        // Without `head` a sort over too many rows says so instead of a partial answer.
        let ins = lines.iter().enumerate().map(|(i, t)| LineIn {
            idx: i,
            text: t,
            level: 0,
            ts: 0,
        });
        let err = run(&q("sort -n"), Some(logfmt()), ins).unwrap_err();
        assert_eq!(err.key, "query_err_sort_rows");
    }
}
