// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Operations of a stream: the entries it shows grouped by an id (a field of the parser,
//! or the capture of a regex, read on each entry's first line), each with its first and
//! last line, its earliest and latest timestamp, its line count and its worst level, for
//! the Operations tab's Gantt view.
//!
//! Memory follows the number of operations, not of lines: at most `MAX_OPERATIONS` are
//! tracked (about 200 bytes each), the further ids only counted. A background scan sends
//! partial maps over later lines, merged with `OpMap::merge`; appended lines are added in
//! place by the engine.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::fields::{FieldParser, FieldSpans};
use crate::tail_engine::NO_TIMESTAMP;

/// Operations tracked at most; further ids are counted, not listed.
pub const MAX_OPERATIONS: usize = 100_000;
/// Bytes of an id kept (longer ids are cut at a character boundary).
pub const MAX_ID_BYTES: usize = 128;
/// Characters of a name kept.
pub const MAX_NAME_CHARS: usize = 120;
/// Untracked ids remembered at most to count them (8 bytes each); past it the count is
/// a lower bound.
const MAX_UNTRACKED: usize = 4_000_000;

/// What names an operation, read on its first entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NameSource {
    #[default]
    None,
    /// A field of the stream's parser.
    Field(String),
    /// The first line's text after its timestamp.
    Message,
}

impl NameSource {
    /// The name of the entry `line`, cut at `MAX_NAME_CHARS`; empty without one.
    pub fn name_of(
        &self,
        parser: Option<&FieldParser>,
        line: &str,
        spans: &mut FieldSpans,
    ) -> Box<str> {
        let text: String = match self {
            NameSource::None => return Box::from(""),
            NameSource::Field(key) => {
                let Some(parser) = parser else {
                    return Box::from("");
                };
                crate::fields::scan(parser, line, spans);
                match spans.get(line, key) {
                    Some(f) => f.value().into_owned(),
                    None => return Box::from(""),
                }
            }
            NameSource::Message => {
                let start =
                    crate::timestamp::leading_span(line, crate::timestamp::FormatHint::default())
                        .unwrap_or(0);
                line[start..].trim().to_string()
            }
        };
        cut_chars(text.trim(), MAX_NAME_CHARS).into()
    }
}

/// `text` cut to `max` characters, with `…` when it was longer.
fn cut_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

/// An id as it is kept: cut at `MAX_ID_BYTES` on a character boundary.
pub fn kept_id(id: &str) -> &str {
    if id.len() <= MAX_ID_BYTES {
        return id;
    }
    let mut end = MAX_ID_BYTES;
    while !id.is_char_boundary(end) {
        end -= 1;
    }
    &id[..end]
}

/// One operation: the visible entries whose id source gave the same value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub id: Box<str>,
    /// From the name source on its first entry (empty without one).
    pub name: Box<str>,
    pub first_line: usize,
    pub last_line: usize,
    /// Earliest and latest timestamp of its lines (ms), `NO_TIMESTAMP` when untimed.
    pub start: i64,
    pub end: i64,
    /// Lines, continuation lines included.
    pub lines: u64,
    /// Entries (first lines) it has: 0 for one only seen through continuation lines.
    pub entries: u64,
    /// The worst level of its lines (`LogLevel as u8`).
    pub worst: u8,
}

impl Operation {
    fn new(id: &str, name: Box<str>, line: usize) -> Self {
        Self {
            id: Box::from(id),
            name,
            first_line: line,
            last_line: line,
            start: NO_TIMESTAMP,
            end: NO_TIMESTAMP,
            lines: 0,
            entries: 0,
            worst: 0,
        }
    }

    /// Whether it has a timestamp (and so a bar).
    pub fn timed(&self) -> bool {
        self.start != NO_TIMESTAMP
    }

    /// From its earliest to its latest timestamp, in ms.
    pub fn duration(&self) -> Option<i64> {
        self.timed().then(|| self.end - self.start)
    }

    fn add(&mut self, line: usize, ts: i64, level: u8) {
        self.lines += 1;
        self.first_line = self.first_line.min(line);
        self.last_line = self.last_line.max(line);
        self.worst = self.worst.max(level);
        if ts != NO_TIMESTAMP {
            if self.start == NO_TIMESTAMP {
                self.start = ts;
                self.end = ts;
            } else {
                self.start = self.start.min(ts);
                self.end = self.end.max(ts);
            }
        }
    }

    /// Adds `other`, the same id over other lines; the name of the earlier first entry
    /// stays.
    fn merge(&mut self, other: &Operation) {
        let earlier =
            self.entries == 0 || (other.entries > 0 && other.first_line < self.first_line);
        if earlier && !other.name.is_empty() {
            self.name = other.name.clone();
        }
        self.first_line = self.first_line.min(other.first_line);
        self.last_line = self.last_line.max(other.last_line);
        self.lines += other.lines;
        self.entries += other.entries;
        self.worst = self.worst.max(other.worst);
        if other.timed() {
            if self.timed() {
                self.start = self.start.min(other.start);
                self.end = self.end.max(other.end);
            } else {
                self.start = other.start;
                self.end = other.end;
            }
        }
    }
}

fn id_hash(id: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

/// The operations of a set of lines, in the order their ids were first seen.
#[derive(Debug, Clone)]
pub struct OpMap {
    pub ops: Vec<Operation>,
    index: HashMap<Box<str>, usize>,
    /// The ids past `MAX_OPERATIONS` (their hashes), to count them.
    untracked: HashSet<u64>,
    /// Lines of the untracked ids.
    pub untracked_lines: u64,
    /// Lines looked at (visible), with or without an id.
    pub lines: u64,
    /// Operations tracked at most (`MAX_OPERATIONS`; a scan's partial maps hold more,
    /// sent before they grow much past it).
    cap: usize,
}

impl Default for OpMap {
    fn default() -> Self {
        Self::with_cap(MAX_OPERATIONS)
    }
}

impl OpMap {
    /// A map tracking at most `cap` operations.
    pub fn with_cap(cap: usize) -> Self {
        Self {
            ops: Vec::new(),
            index: HashMap::new(),
            untracked: HashSet::new(),
            untracked_lines: 0,
            lines: 0,
            cap,
        }
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Ids seen but not tracked (a lower bound past `MAX_UNTRACKED`).
    pub fn untracked(&self) -> usize {
        self.untracked.len()
    }

    /// The operation of `id`, if tracked.
    pub fn get(&self, id: &str) -> Option<&Operation> {
        self.index.get(kept_id(id)).map(|&i| &self.ops[i])
    }

    fn untrack(&mut self, id: &str, lines: u64) {
        if self.untracked.len() < MAX_UNTRACKED {
            self.untracked.insert(id_hash(id));
        }
        self.untracked_lines += lines;
    }

    /// The first line of an entry with id `id`: `name` gives its name when the id is new.
    pub fn add_entry(
        &mut self,
        id: &str,
        name: impl FnOnce() -> Box<str>,
        line: usize,
        ts: i64,
        level: u8,
    ) {
        self.lines += 1;
        let id = kept_id(id);
        let at = match self.index.get(id) {
            Some(&at) => at,
            None if self.ops.len() < self.cap => {
                self.ops.push(Operation::new(id, name(), line));
                self.index.insert(Box::from(id), self.ops.len() - 1);
                self.ops.len() - 1
            }
            None => {
                self.untrack(id, 1);
                return;
            }
        };
        let op = &mut self.ops[at];
        op.entries += 1;
        op.add(line, ts, level);
    }

    /// A continuation line of an entry with id `id` (its first line may be in an earlier
    /// part of the scan).
    pub fn add_continuation(&mut self, id: &str, line: usize, ts: i64, level: u8) {
        self.lines += 1;
        let id = kept_id(id);
        let at = match self.index.get(id) {
            Some(&at) => at,
            None if self.ops.len() < self.cap => {
                self.ops.push(Operation::new(id, Box::from(""), line));
                self.index.insert(Box::from(id), self.ops.len() - 1);
                self.ops.len() - 1
            }
            None => {
                self.untrack(id, 1);
                return;
            }
        };
        self.ops[at].add(line, ts, level);
    }

    /// A visible line without an id (counted only).
    pub fn add_other(&mut self) {
        self.lines += 1;
    }

    /// Adds `other`, gathered over later lines.
    pub fn merge(&mut self, other: OpMap) {
        self.lines += other.lines;
        self.untracked_lines += other.untracked_lines;
        for h in other.untracked {
            if self.untracked.len() < MAX_UNTRACKED {
                self.untracked.insert(h);
            }
        }
        for op in other.ops {
            match self.index.get(op.id.as_ref()) {
                Some(&at) => self.ops[at].merge(&op),
                // Only seen through continuation lines of an untracked entry: not new.
                None if op.entries == 0 => self.untracked_lines += op.lines,
                None if self.ops.len() < self.cap => {
                    self.index.insert(op.id.clone(), self.ops.len());
                    self.ops.push(op);
                }
                None => {
                    let (id, lines) = (op.id, op.lines);
                    self.untrack(&id, lines);
                }
            }
        }
    }

    /// Earliest start and latest end of the timed operations.
    pub fn span(&self) -> Option<(i64, i64)> {
        let mut out: Option<(i64, i64)> = None;
        for op in self.ops.iter().filter(|o| o.timed()) {
            out = Some(match out {
                Some((a, b)) => (a.min(op.start), b.max(op.end)),
                None => (op.start, op.end),
            });
        }
        out
    }
}

/// The Operations of a stream: the sources, the operations so far and the scan that adds
/// to them (see `TailEngine::open_operations`).
pub struct OpsState {
    pub source: crate::field_stats::StatsSource,
    pub name: NameSource,
    pub map: OpMap,
    pub(crate) job: Option<crate::scan_job::ScanJob>,
    pub progress: f32,
    /// The lines covered: `0..covered`.
    pub(crate) covered: usize,
    /// The view or the sources changed: computed again on the next poll.
    pub(crate) restart: bool,
    /// The file could not be read.
    pub failed: bool,
    /// Bumped whenever `map` changes, for views that cache what they draw.
    pub generation: u64,
    pub(crate) job_generation: u64,
    /// The timestamp an appended line without its own inherits.
    pub(crate) last_ts: i64,
    /// The id of the entry an appended continuation line belongs to.
    pub(crate) current: Option<Box<str>>,
}

impl OpsState {
    pub(crate) fn new(source: crate::field_stats::StatsSource, name: NameSource) -> Self {
        Self {
            source,
            name,
            map: OpMap::default(),
            job: None,
            progress: 0.0,
            covered: 0,
            restart: true,
            failed: false,
            generation: 0,
            job_generation: 0,
            last_ts: NO_TIMESTAMP,
            current: None,
        }
    }

    /// Whether the operations cover the view (no scan running, nothing waiting).
    pub fn done(&self) -> bool {
        self.job.is_none() && !self.restart
    }

    /// Whether the operations reach line `total` (appended lines included).
    pub fn covered_all(&self, total: usize) -> bool {
        self.covered >= total
    }
}

/// How the table is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpSort {
    /// Earliest first (untimed ones last, by first line).
    #[default]
    Start,
    /// Longest first.
    Duration,
    /// Most lines first.
    Lines,
    /// Worst level first.
    Level,
}

impl OpSort {
    pub const ALL: [OpSort; 4] = [
        OpSort::Start,
        OpSort::Duration,
        OpSort::Lines,
        OpSort::Level,
    ];
}

/// The operations listed: matching `needle` (in id or name, without case) and lasting at
/// least `min_ms` (untimed ones only without a minimum), sorted by `sort`. Indices into
/// `map.ops`.
pub fn listed(map: &OpMap, needle: &str, min_ms: i64, sort: OpSort) -> Vec<usize> {
    let needle = needle.trim().to_lowercase();
    let mut out: Vec<usize> = (0..map.ops.len())
        .filter(|&i| {
            let op = &map.ops[i];
            (needle.is_empty()
                || op.id.to_lowercase().contains(&needle)
                || op.name.to_lowercase().contains(&needle))
                && (min_ms <= 0 || op.duration().is_some_and(|d| d >= min_ms))
        })
        .collect();
    let ops = &map.ops;
    match sort {
        OpSort::Start => out.sort_by_key(|&i| {
            let o = &ops[i];
            (!o.timed(), o.start, o.first_line)
        }),
        OpSort::Duration => out.sort_by_key(|&i| {
            (
                std::cmp::Reverse(ops[i].duration().unwrap_or(-1)),
                ops[i].first_line,
            )
        }),
        OpSort::Lines => out.sort_by_key(|&i| (std::cmp::Reverse(ops[i].lines), ops[i].first_line)),
        OpSort::Level => out.sort_by_key(|&i| (std::cmp::Reverse(ops[i].worst), ops[i].first_line)),
    }
    out
}

/// The operations `rows` as CSV: id, name, start, end, duration (ms), lines, worst level.
pub fn to_csv(map: &OpMap, rows: &[usize]) -> String {
    let cell = |s: &str| {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let time = |ms: i64| {
        if ms == NO_TIMESTAMP {
            String::new()
        } else {
            crate::timestamp::format_millis(ms)
        }
    };
    let mut out = String::from("id,name,start,end,duration_ms,lines,worst_level\n");
    for &i in rows {
        let o = &map.ops[i];
        let level = match o.worst {
            0 => "",
            l => crate::log_level::LogLevel::ALL[l as usize - 1].name(),
        };
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            cell(&o.id),
            cell(&o.name),
            time(o.start),
            time(o.end),
            o.duration().map_or_else(String::new, |d| d.to_string()),
            o.lines,
            level
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(m: &mut OpMap, id: &str, line: usize, ts: i64, level: u8) {
        m.add_entry(id, || Box::from(format!("n{line}")), line, ts, level);
    }

    #[test]
    fn entries_of_one_id_make_one_operation_with_its_span_and_worst_level() {
        let mut m = OpMap::default();
        add(&mut m, "a", 0, 1_000, 3);
        add(&mut m, "b", 1, 1_500, 3);
        add(&mut m, "a", 2, 4_000, 5);
        m.add_continuation("a", 3, 4_000, 0);
        // Out of order across threads: the earliest stays the start.
        add(&mut m, "a", 4, 500, 3);
        let a = m.get("a").unwrap();
        assert_eq!((a.start, a.end, a.duration()), (500, 4_000, Some(3_500)));
        assert_eq!((a.lines, a.entries, a.worst), (4, 3, 5));
        assert_eq!((a.first_line, a.last_line, a.name.as_ref()), (0, 4, "n0"));
        assert_eq!(m.get("b").unwrap().duration(), Some(0));
        assert_eq!(m.lines, 5);
    }

    #[test]
    fn ids_past_the_cap_are_counted_not_tracked() {
        let mut m = OpMap::default();
        for i in 0..MAX_OPERATIONS + 10 {
            add(&mut m, &format!("id{i}"), i, NO_TIMESTAMP, 0);
        }
        add(
            &mut m,
            &format!("id{}", MAX_OPERATIONS + 3),
            0,
            NO_TIMESTAMP,
            0,
        );
        assert_eq!(
            (m.len(), m.untracked(), m.untracked_lines),
            (MAX_OPERATIONS, 10, 11)
        );
    }

    #[test]
    fn merged_parts_equal_one_pass() {
        let lines: Vec<(&str, i64, u8, bool)> = vec![
            ("a", 10, 3, true),
            ("a", 10, 0, false),
            ("b", 20, 4, true),
            ("a", 30, 5, true),
            ("c", 40, 3, true),
            ("a", 45, 0, false),
        ];
        let mut whole = OpMap::default();
        let mut parts = [OpMap::default(), OpMap::default()];
        for (i, &(id, ts, lvl, first)) in lines.iter().enumerate() {
            for m in [&mut whole, &mut parts[if i < 3 { 0 } else { 1 }]] {
                if first {
                    add(m, id, i, ts, lvl);
                } else {
                    m.add_continuation(id, i, ts, lvl);
                }
            }
        }
        let mut merged = std::mem::take(&mut parts[0]);
        merged.merge(std::mem::take(&mut parts[1]));
        assert_eq!(merged.ops, whole.ops);
        assert_eq!(merged.lines, whole.lines);
    }

    #[test]
    fn listing_narrows_and_sorts() {
        let mut m = OpMap::default();
        add(&mut m, "slow", 0, 0, 3);
        add(&mut m, "slow", 1, 9_000, 3);
        add(&mut m, "fail", 2, 1_000, 5);
        add(&mut m, "fail", 3, 2_000, 3);
        add(&mut m, "untimed", 4, NO_TIMESTAMP, 0);
        let ids = |rows: Vec<usize>| -> Vec<String> {
            rows.iter().map(|&i| m.ops[i].id.to_string()).collect()
        };
        assert_eq!(
            ids(listed(&m, "", 0, OpSort::Start)),
            ["slow", "fail", "untimed"]
        );
        assert_eq!(
            ids(listed(&m, "", 0, OpSort::Duration)),
            ["slow", "fail", "untimed"]
        );
        assert_eq!(
            ids(listed(&m, "", 0, OpSort::Level)),
            ["fail", "slow", "untimed"]
        );
        assert_eq!(ids(listed(&m, "FA", 0, OpSort::Start)), ["fail"]);
        assert_eq!(ids(listed(&m, "", 5_000, OpSort::Start)), ["slow"]);
        let csv = to_csv(&m, &listed(&m, "fail", 0, OpSort::Start));
        assert!(csv.ends_with(",1000,2,ERROR\n"), "{csv}");
    }

    #[test]
    fn long_ids_and_names_are_cut() {
        let long = "é".repeat(100);
        assert!(kept_id(&long).len() <= MAX_ID_BYTES);
        assert!(long.starts_with(kept_id(&long)));
        let mut spans = FieldSpans::new();
        let name =
            NameSource::Message.name_of(None, "2026-10-28T10:00:00Z GET /orders 200", &mut spans);
        assert_eq!(name.as_ref(), "GET /orders 200");
        assert_eq!(cut_chars("abcdef", 3), "abc…");
    }
}
