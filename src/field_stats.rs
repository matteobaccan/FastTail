// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Statistics of one field over the lines a stream shows: the most frequent values with
//! their counts per level, a numeric summary (min, max, mean, percentiles) when the values
//! are numbers, and counts per time slice for a line chart.
//!
//! The source of the values is a field of the stream's parser or the first capture group
//! of a regex, so it works on any stream. Memory is bounded whatever the file size: at
//! most `MAX_VALUES` distinct values (the rest counted together), percentiles from a
//! log-scale histogram within 1 % of the exact value, and per-slice series stored sparse.
//! Partial results of a background scan are merged with `FieldStats::merge`.

use std::borrow::Cow;
use std::collections::HashMap;

use regex::Regex;

use crate::fields::{FieldParser, FieldSpans};
use crate::tail_engine::NO_TIMESTAMP;

/// Distinct values counted one by one; past it new values go to `FieldStats::other`.
pub const MAX_VALUES: usize = 100_000;
/// Bytes of a value kept (longer ones are cut, with `…`).
pub const MAX_VALUE_BYTES: usize = 256;
/// Slices of an automatic time breakdown at most.
pub const MAX_AUTO_SLICES: usize = 240;
/// Slices of a breakdown at most, whatever width was chosen.
pub const MAX_SLICES: usize = 2_000;
/// The slice widths offered, in milliseconds: 1 s, 5 s, 15 s, 1 min, 5 min, 15 min, 1 h,
/// 6 h, 1 day.
pub const SLICE_WIDTHS: [i64; 9] = [
    1_000, 5_000, 15_000, 60_000, 300_000, 900_000, 3_600_000, 21_600_000, 86_400_000,
];
/// Levels counted per value (`LogLevel as u8`, `Unknown` included).
pub const LEVELS: usize = 7;
/// Share of the values that must read as numbers for the numeric summary.
const NUMERIC_SHARE: f64 = 0.9;
/// Relative width of a histogram bucket (1 %).
const BUCKET_GROWTH: f64 = 1.01;

/// Where the values come from.
#[derive(Debug, Clone)]
pub enum StatsSource {
    /// A field of the stream's parser.
    Field(String),
    /// The first capture group of a regex (the whole match without a group).
    Capture(Regex),
}

impl StatsSource {
    /// Calls `f` with the value of `line` (`None` when it has none), without copying it.
    /// `spans` is a scratch buffer reused per line.
    pub fn with_value<R>(
        &self,
        parser: Option<&FieldParser>,
        line: &str,
        spans: &mut FieldSpans,
        f: impl FnOnce(Option<&str>) -> R,
    ) -> R {
        match self {
            StatsSource::Field(key) => {
                let Some(parser) = parser else {
                    return f(None);
                };
                crate::fields::scan(parser, line, spans);
                match spans.get(line, key) {
                    Some(field) => f(Some(&field.value())),
                    None => f(None),
                }
            }
            StatsSource::Capture(re) => {
                let m = re
                    .captures(line)
                    .and_then(|caps| caps.get(1).or_else(|| caps.get(0)));
                f(m.map(|m| m.as_str()))
            }
        }
    }

    /// How the source reads in a title: the field name, or the regex.
    pub fn label(&self) -> String {
        match self {
            StatsSource::Field(key) => key.clone(),
            StatsSource::Capture(re) => re.as_str().to_string(),
        }
    }
}

/// The time slices of a breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slices {
    /// Start of the first slice (ms since the epoch).
    pub start: i64,
    /// Width of a slice in ms.
    pub width: i64,
    pub count: usize,
}

impl Slices {
    /// The narrowest offered width that covers `[first, last]` in `MAX_AUTO_SLICES`
    /// slices or fewer (the widest when none does).
    pub fn auto(first: i64, last: i64) -> Option<Self> {
        let span = last.checked_sub(first)?.max(0);
        let width = SLICE_WIDTHS
            .iter()
            .copied()
            .find(|w| span / w < MAX_AUTO_SLICES as i64)
            .unwrap_or(SLICE_WIDTHS[SLICE_WIDTHS.len() - 1]);
        Self::with_width(first, last, width)
    }

    /// Slices of `width` over `[first, last]`, starting at a multiple of the width;
    /// `None` when that takes more than `MAX_SLICES`.
    pub fn with_width(first: i64, last: i64, width: i64) -> Option<Self> {
        if width <= 0 || last < first || first == NO_TIMESTAMP {
            return None;
        }
        let start = first.div_euclid(width) * width;
        let count = ((last - start) / width + 1) as usize;
        (count <= MAX_SLICES).then_some(Self {
            start,
            width,
            count,
        })
    }

    /// The slice of `ts`, `None` outside the slices or without a timestamp.
    pub fn index(&self, ts: i64) -> Option<usize> {
        if ts == NO_TIMESTAMP || ts < self.start {
            return None;
        }
        let i = ((ts - self.start) / self.width) as usize;
        (i < self.count).then_some(i)
    }
}

/// The counts of one value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueStats {
    pub count: u64,
    /// Count per level (`LogLevel as u8`).
    pub levels: [u64; LEVELS],
    /// Count per slice, sparse: `(slice, count)` runs in the order met.
    series: Vec<(u32, u32)>,
}

impl ValueStats {
    fn add(&mut self, level: u8, slice: Option<usize>) {
        self.count += 1;
        self.levels[(level as usize).min(LEVELS - 1)] += 1;
        if let Some(i) = slice {
            match self.series.last_mut() {
                Some((s, n)) if *s == i as u32 && *n < u32::MAX => *n += 1,
                _ => self.series.push((i as u32, 1)),
            }
        }
    }

    fn merge(&mut self, other: ValueStats) {
        self.count += other.count;
        for (a, b) in self.levels.iter_mut().zip(other.levels) {
            *a += b;
        }
        self.series.extend(other.series);
    }

    /// The count in each of `count` slices.
    pub fn series(&self, count: usize) -> Vec<u64> {
        let mut out = vec![0u64; count];
        for &(s, n) in &self.series {
            if let Some(slot) = out.get_mut(s as usize) {
                *slot += u64::from(n);
            }
        }
        out
    }
}

/// A log-scale histogram of numbers: percentiles within 1 % of the exact value, exact
/// count, min, max and sum.
#[derive(Debug, Clone, Default)]
pub struct NumHist {
    pos: HashMap<i32, u64>,
    neg: HashMap<i32, u64>,
    zero: u64,
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
}

fn bucket(v: f64) -> i32 {
    (v.ln() / BUCKET_GROWTH.ln()).floor() as i32
}

fn bucket_value(b: i32) -> f64 {
    BUCKET_GROWTH.powf(b as f64 + 0.5)
}

impl NumHist {
    pub fn add(&mut self, v: f64) {
        if !v.is_finite() {
            return;
        }
        if self.count == 0 {
            (self.min, self.max) = (v, v);
        } else {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        self.count += 1;
        self.sum += v;
        if v > 0.0 {
            *self.pos.entry(bucket(v)).or_default() += 1;
        } else if v < 0.0 {
            *self.neg.entry(bucket(-v)).or_default() += 1;
        } else {
            self.zero += 1;
        }
    }

    pub fn merge(&mut self, other: &NumHist) {
        if other.count == 0 {
            return;
        }
        if self.count == 0 {
            (self.min, self.max) = (other.min, other.max);
        } else {
            self.min = self.min.min(other.min);
            self.max = self.max.max(other.max);
        }
        self.count += other.count;
        self.sum += other.sum;
        self.zero += other.zero;
        for (b, n) in &other.pos {
            *self.pos.entry(*b).or_default() += n;
        }
        for (b, n) in &other.neg {
            *self.neg.entry(*b).or_default() += n;
        }
    }

    pub fn mean(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum / self.count as f64)
    }

    /// The value at quantile `q` (0..=1), within 1 % and inside `[min, max]`.
    pub fn percentile(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        // The rank of the value wanted, 1-based (nearest rank).
        let rank = ((q.clamp(0.0, 1.0) * self.count as f64).ceil() as u64).max(1);
        // The ends are known exactly.
        if rank == 1 {
            return Some(self.min);
        }
        if rank >= self.count {
            return Some(self.max);
        }
        let mut seen = 0u64;
        let mut neg: Vec<(&i32, &u64)> = self.neg.iter().collect();
        neg.sort_by(|a, b| b.0.cmp(a.0));
        for (b, n) in neg {
            seen += n;
            if seen >= rank {
                return Some((-bucket_value(*b)).clamp(self.min, self.max));
            }
        }
        seen += self.zero;
        if seen >= rank {
            return Some(0.0f64.clamp(self.min, self.max));
        }
        let mut pos: Vec<(&i32, &u64)> = self.pos.iter().collect();
        pos.sort_by_key(|e| *e.0);
        for (b, n) in pos {
            seen += n;
            if seen >= rank {
                return Some(bucket_value(*b).clamp(self.min, self.max));
            }
        }
        Some(self.max)
    }
}

/// `text` as a number: a plain number, or one followed by a unit (`ms`, `s`, `us`, `%`,
/// `B`, `KB`, `MB`).
pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim();
    let digits = ["ms", "us", "µs", "KB", "MB", "kb", "mb", "s", "%", "B"]
        .iter()
        .find_map(|u| t.strip_suffix(u))
        .unwrap_or(t)
        .trim_end();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `value` cut at `MAX_VALUE_BYTES` on a character boundary, with `…`.
fn kept(value: &str) -> Cow<'_, str> {
    if value.len() <= MAX_VALUE_BYTES {
        return Cow::Borrowed(value);
    }
    let mut end = MAX_VALUE_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}…", &value[..end]))
}

/// How the time slices of a breakdown are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliceChoice {
    /// No time breakdown.
    Off,
    /// The narrowest offered width that keeps `MAX_AUTO_SLICES` or fewer.
    #[default]
    Auto,
    /// This width in ms (one of `SLICE_WIDTHS`).
    Width(i64),
}

/// The Statistics of a stream: what is counted, the counts so far and the scan that adds
/// to them (see `TailEngine::open_stats`).
pub struct StatsState {
    pub source: StatsSource,
    pub slice_choice: SliceChoice,
    pub result: FieldStats,
    /// The scan over the lines present when it started.
    pub(crate) job: Option<crate::scan_job::ScanJob>,
    pub progress: f32,
    /// The lines counted: `0..covered` (visible ones only).
    pub(crate) covered: usize,
    /// The view changed (filters, reload): count again from scratch on the next poll.
    pub(crate) restart: bool,
    /// The file could not be read.
    pub failed: bool,
    /// Bumped whenever `result` changes, for views that cache what they draw.
    pub generation: u64,
    pub(crate) job_generation: u64,
    /// Values listed in the tab (5..=1000).
    pub top_n: usize,
}

impl StatsState {
    /// Whether the counts cover the view (no scan running, nothing waiting).
    pub fn done(&self) -> bool {
        self.job.is_none() && !self.restart
    }

    /// Whether the counts reach line `total` (appended lines included).
    pub fn covered_all(&self, total: usize) -> bool {
        self.covered >= total
    }
}

/// The statistics of one source over a set of lines.
#[derive(Debug, Clone, Default)]
pub struct FieldStats {
    pub values: HashMap<Box<str>, ValueStats>,
    /// The values past `MAX_VALUES`, counted together.
    pub other: ValueStats,
    /// Whether `other` holds anything (the distinct count is a lower bound).
    pub capped: bool,
    /// Lines looked at, with or without a value.
    pub lines: u64,
    /// Lines that had a value.
    pub with_value: u64,
    /// Values that read as numbers, and the others (empty ones not counted).
    pub numeric_values: u64,
    pub text_values: u64,
    pub hist: NumHist,
    pub slices: Option<Slices>,
    /// Values per slice, and the numbers of each slice (numeric sources).
    slice_totals: Vec<u64>,
    slice_hists: Vec<NumHist>,
}

impl FieldStats {
    pub fn new(slices: Option<Slices>) -> Self {
        Self {
            slices,
            slice_totals: vec![0; slices.map_or(0, |s| s.count)],
            ..Self::default()
        }
    }

    /// One line: its value (if any), its level (`LogLevel as u8`) and its timestamp.
    pub fn add_line(&mut self, value: Option<&str>, level: u8, ts: i64) {
        self.lines += 1;
        let Some(value) = value else {
            return;
        };
        self.with_value += 1;
        let slice = self.slices.and_then(|s| s.index(ts));
        if let Some(i) = slice {
            self.slice_totals[i] += 1;
        }
        if !value.trim().is_empty() {
            match parse_number(value) {
                Some(v) => {
                    self.numeric_values += 1;
                    self.hist.add(v);
                    if let Some(i) = slice {
                        if self.slice_hists.len() < self.slice_totals.len() {
                            self.slice_hists
                                .resize_with(self.slice_totals.len(), NumHist::default);
                        }
                        self.slice_hists[i].add(v);
                    }
                }
                None => self.text_values += 1,
            }
        }
        let value = kept(value);
        if let Some(stats) = self.values.get_mut(value.as_ref()) {
            stats.add(level, slice);
        } else if self.values.len() < MAX_VALUES {
            let mut stats = ValueStats::default();
            stats.add(level, slice);
            self.values
                .insert(value.into_owned().into_boxed_str(), stats);
        } else {
            self.capped = true;
            self.other.add(level, slice);
        }
    }

    /// Adds the counts of `other`, gathered over other lines with the same slices.
    pub fn merge(&mut self, other: FieldStats) {
        self.lines += other.lines;
        self.with_value += other.with_value;
        self.numeric_values += other.numeric_values;
        self.text_values += other.text_values;
        self.hist.merge(&other.hist);
        for (a, b) in self.slice_totals.iter_mut().zip(&other.slice_totals) {
            *a += b;
        }
        if self.slice_hists.len() < other.slice_hists.len() {
            self.slice_hists
                .resize_with(other.slice_hists.len(), NumHist::default);
        }
        for (a, b) in self.slice_hists.iter_mut().zip(&other.slice_hists) {
            a.merge(b);
        }
        for (value, stats) in other.values {
            if let Some(mine) = self.values.get_mut(&value) {
                mine.merge(stats);
            } else if self.values.len() < MAX_VALUES {
                self.values.insert(value, stats);
            } else {
                self.capped = true;
                self.other.merge(stats);
            }
        }
        if other.capped {
            self.capped = true;
        }
        self.other.merge(other.other);
    }

    /// Distinct values counted (a lower bound when `capped`).
    pub fn distinct(&self) -> usize {
        self.values.len()
    }

    /// Whether the values are numbers (at least 90 % of the non-empty ones).
    pub fn is_numeric(&self) -> bool {
        let total = self.numeric_values + self.text_values;
        total > 0 && self.numeric_values as f64 >= NUMERIC_SHARE * total as f64
    }

    /// The `n` most frequent values, most frequent first (ties in value order).
    pub fn top(&self, n: usize) -> Vec<(&str, &ValueStats)> {
        let mut all: Vec<(&str, &ValueStats)> =
            self.values.iter().map(|(k, v)| (k.as_ref(), v)).collect();
        all.sort_unstable_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(b.0)));
        all.truncate(n);
        all
    }

    /// Values in each slice, all values together.
    pub fn slice_totals(&self) -> &[u64] {
        &self.slice_totals
    }

    /// The value at quantile `q` of the numbers of slice `i`.
    pub fn slice_percentile(&self, i: usize, q: f64) -> Option<f64> {
        self.slice_hists.get(i).and_then(|h| h.percentile(q))
    }

    /// The table as CSV: value, count, share and the count per level (`level_names`, in
    /// `LogLevel` order), the `n` most frequent values then the others.
    pub fn to_csv(&self, n: usize, level_names: &[&str; LEVELS]) -> String {
        let mut out = String::from("value,count,share");
        for name in level_names {
            out.push(',');
            out.push_str(&csv_cell(name));
        }
        out.push('\n');
        let total = self.with_value.max(1) as f64;
        let top = self.top(n);
        let row = |out: &mut String, value: &str, s: &ValueStats| {
            out.push_str(&csv_cell(value));
            out.push_str(&format!(
                ",{},{:.2}%",
                s.count,
                s.count as f64 * 100.0 / total
            ));
            for c in s.levels {
                out.push_str(&format!(",{c}"));
            }
            out.push('\n');
        };
        for (value, s) in &top {
            row(&mut out, value, s);
        }
        // What the rows above leave out, as one row.
        let mut rest = self.other.clone();
        if self.values.len() > top.len() {
            let shown: std::collections::HashSet<&str> = top.iter().map(|(v, _)| *v).collect();
            for (v, s) in &self.values {
                if !shown.contains(v.as_ref()) {
                    rest.merge(s.clone());
                }
            }
        }
        if rest.count > 0 {
            row(&mut out, "(other values)", &rest);
        }
        out
    }
}

fn csv_cell(text: &str) -> String {
    // A leading = + - @ would run as a formula in a spreadsheet.
    let guarded = if text.starts_with(['=', '+', '-', '@']) {
        format!("'{text}")
    } else {
        text.to_string()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// The filter term that keeps (or, as an exclude term, drops) the lines whose source
/// value is `value`: `key=value` for a field (the value escaped in regex mode, `key~=` when
/// a `|` would split it), for a capture the regex with its group replaced by the escaped
/// value in regex mode, else the value as text.
pub fn filter_term(source: &StatsSource, value: &str, regex_mode: bool) -> String {
    match source {
        StatsSource::Field(key) => {
            if regex_mode {
                format!("{key}={}", regex::escape(value))
            } else if value.contains('|') {
                format!("{key}~={value}")
            } else {
                format!("{key}={value}")
            }
        }
        StatsSource::Capture(re) if regex_mode => match first_group(re.as_str()) {
            Some((a, b)) => format!(
                "{}{}{}",
                &re.as_str()[..a],
                regex::escape(value),
                &re.as_str()[b..]
            ),
            None => regex::escape(value),
        },
        StatsSource::Capture(_) => value.to_string(),
    }
}

/// Byte range of the first capturing group of `pattern`, parentheses included.
fn first_group(pattern: &str) -> Option<(usize, usize)> {
    let b = pattern.as_bytes();
    let (mut i, mut class, mut start, mut depth) = (0usize, false, None, 0usize);
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'[' if !class => class = true,
            b']' if class => class = false,
            b'(' if !class => {
                if start.is_some() {
                    depth += 1;
                } else {
                    let rest = &pattern[i + 1..];
                    let capturing = !rest.starts_with('?')
                        || rest.starts_with("?P<")
                        || (rest.starts_with("?<")
                            && !rest.starts_with("?<=")
                            && !rest.starts_with("?<!"));
                    if capturing {
                        start = Some(i);
                    }
                }
            }
            b')' if !class => {
                if let Some(s) = start {
                    if depth == 0 {
                        return Some((s, i + 1));
                    }
                    depth -= 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_counted_with_levels_and_capped() {
        let mut s = FieldStats::new(None);
        for (v, lvl) in [("200", 3), ("500", 5), ("200", 3), ("404", 4), ("200", 5)] {
            s.add_line(Some(v), lvl, NO_TIMESTAMP);
        }
        s.add_line(None, 0, NO_TIMESTAMP);
        assert_eq!((s.lines, s.with_value, s.distinct()), (6, 5, 3));
        let top = s.top(2);
        assert_eq!(top[0].0, "200");
        assert_eq!(top[0].1.count, 3);
        assert_eq!(top[0].1.levels[3], 2);
        assert_eq!(top[0].1.levels[5], 1);
        assert_eq!(top[1].0, "404", "ties are in value order");
        assert!(s.is_numeric());

        let mut big = FieldStats::new(None);
        for i in 0..MAX_VALUES + 5 {
            big.add_line(Some(&format!("id{i}")), 0, NO_TIMESTAMP);
        }
        big.add_line(Some("id0"), 0, NO_TIMESTAMP);
        assert!(big.capped);
        assert_eq!(big.distinct(), MAX_VALUES);
        assert_eq!(big.other.count, 5);
        assert_eq!(big.values["id0"].count, 2);
        assert!(!big.is_numeric());

        let long = "x".repeat(300) + "é";
        let mut cut = FieldStats::new(None);
        cut.add_line(Some(&long), 0, NO_TIMESTAMP);
        let key = cut.values.keys().next().unwrap();
        assert!(key.ends_with('…') && key.len() <= MAX_VALUE_BYTES + 3);
    }

    #[test]
    fn numbers_and_percentiles() {
        assert_eq!(parse_number(" 250ms"), Some(250.0));
        assert_eq!(parse_number("12.5 %"), Some(12.5));
        assert_eq!(parse_number("3KB"), Some(3.0));
        assert_eq!(parse_number("-4"), Some(-4.0));
        assert_eq!(parse_number("ms"), None);
        assert_eq!(parse_number("GET"), None);
        assert_eq!(parse_number("inf"), None);

        let mut h = NumHist::default();
        let mut values: Vec<f64> = (1..=10_000).map(|i| (i as f64).powf(1.3) / 7.0).collect();
        values.extend([0.0, 0.0, -5.0, -50.0]);
        for v in &values {
            h.add(*v);
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for q in [0.5, 0.9, 0.95, 0.99] {
            let exact = values[((q * values.len() as f64).ceil() as usize).max(1) - 1];
            let got = h.percentile(q).unwrap();
            assert!(
                (got - exact).abs() <= exact.abs() * 0.011,
                "p{q}: {got} vs {exact}"
            );
        }
        assert_eq!(h.percentile(0.0), Some(-50.0));
        assert_eq!(h.percentile(1.0), Some(h.max));
        assert_eq!((h.min, h.count), (-50.0, values.len() as u64));
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        assert!((h.mean().unwrap() - mean).abs() < 1e-6);
        assert_eq!(NumHist::default().percentile(0.5), None);
    }

    #[test]
    fn slices_series_and_percentiles_per_slice() {
        let auto = Slices::auto(0, 3_600_000).unwrap();
        // 15 s would take 241 slices once aligned.
        assert_eq!(auto.width, 60_000, "an hour in at most 240 slices");
        assert!(auto.count <= MAX_AUTO_SLICES);
        assert_eq!(Slices::auto(5, 5).unwrap().count, 1);
        assert!(Slices::with_width(0, 86_400_000 * 30, 1_000).is_none());
        let s = Slices::with_width(61_000, 185_000, 60_000).unwrap();
        assert_eq!((s.start, s.count), (60_000, 3));
        assert_eq!(s.index(59_999), None);
        assert_eq!(s.index(125_000), Some(1));
        assert_eq!(s.index(NO_TIMESTAMP), None);

        let mut st = FieldStats::new(Some(s));
        for (v, ts) in [
            ("10", 61_000),
            ("20", 62_000),
            ("10", 130_000),
            ("30", 185_000),
            ("10", 70_000), // out of order, back in the first slice
            ("10", NO_TIMESTAMP),
        ] {
            st.add_line(Some(v), 3, ts);
        }
        assert_eq!(st.slice_totals(), &[3, 1, 1]);
        assert_eq!(st.values["10"].series(3), vec![2, 1, 0]);
        assert_eq!(st.values["10"].count, 4);
        assert!((st.slice_percentile(0, 0.5).unwrap() - 10.0).abs() < 0.2);
        assert_eq!(st.slice_percentile(2, 0.95), Some(30.0));
    }

    #[test]
    fn merged_parts_equal_one_pass() {
        let lines: Vec<(String, u8, i64)> = (0..5_000)
            .map(|i| (format!("{}", i % 37), (i % 7) as u8, 1_000 * i as i64))
            .collect();
        let slices = Slices::auto(0, 5_000_000);
        let mut whole = FieldStats::new(slices);
        let (mut a, mut b) = (FieldStats::new(slices), FieldStats::new(slices));
        for (i, (v, l, t)) in lines.iter().enumerate() {
            whole.add_line(Some(v), *l, *t);
            if i < 2_222 { &mut a } else { &mut b }.add_line(Some(v), *l, *t);
        }
        a.merge(b);
        assert_eq!(a.lines, whole.lines);
        assert_eq!(a.slice_totals(), whole.slice_totals());
        for (v, s) in &whole.values {
            let m = &a.values[v];
            assert_eq!((m.count, m.levels), (s.count, s.levels));
            let n = slices.unwrap().count;
            assert_eq!(m.series(n), s.series(n));
        }
        assert_eq!(a.hist.percentile(0.9), whole.hist.percentile(0.9));
    }

    #[test]
    fn sources_csv_and_filter_terms() {
        let parser = crate::fields::ParserChoice::from_name("logfmt", "")
            .unwrap()
            .build()
            .unwrap()
            .unwrap();
        let mut spans = FieldSpans::new();
        let field = StatsSource::Field("status".into());
        assert_eq!(
            field.with_value(
                Some(&parser),
                "level=info status=503 path=/x",
                &mut spans,
                |v| v.map(str::to_string)
            ),
            Some("503".to_string())
        );
        assert!(!field.with_value(None, "status=503", &mut spans, |v| v.is_some()));
        let cap = StatsSource::Capture(Regex::new(r"took (\d+)ms").unwrap());
        assert_eq!(
            cap.with_value(None, "query took 250ms", &mut spans, |v| v
                .map(str::to_string)),
            Some("250".to_string())
        );
        let whole = StatsSource::Capture(Regex::new(r"GET|POST").unwrap());
        assert_eq!(
            whole.with_value(None, "a POST b", &mut spans, |v| v.map(str::to_string)),
            Some("POST".to_string())
        );

        assert_eq!(filter_term(&field, "503", false), "status=503");
        assert_eq!(filter_term(&field, "a|b", false), "status~=a|b");
        assert_eq!(filter_term(&field, "a.b", true), r"status=a\.b");
        assert_eq!(filter_term(&cap, "250", true), r"took 250ms");
        assert_eq!(filter_term(&cap, "250", false), "250");
        let named = StatsSource::Capture(Regex::new(r"(?:x)(?P<n>[a(]+)\)").unwrap());
        assert_eq!(filter_term(&named, "a.", true), r"(?:x)a\.\)");
        assert_eq!(filter_term(&whole, "POST", true), "POST");

        let mut s = FieldStats::new(None);
        for v in ["=1+1", "a,b", "a,b", "x"] {
            s.add_line(Some(v), 5, NO_TIMESTAMP);
        }
        let names = ["?", "TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"];
        let csv = s.to_csv(1, &names);
        let rows: Vec<&str> = csv.lines().collect();
        assert_eq!(
            rows[0],
            "value,count,share,?,TRACE,DEBUG,INFO,WARN,ERROR,FATAL"
        );
        assert_eq!(rows[1], "\"a,b\",2,50.00%,0,0,0,0,0,2,0");
        assert_eq!(rows[2], "(other values),2,50.00%,0,0,0,0,0,2,0");
        assert!(s.to_csv(5, &names).contains("'=1+1,1,"));
    }
}
