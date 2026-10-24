// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! What is different in a span of a stream's timeline: the entries of the span compared
//! with a baseline (the rest of the log, or the span of equal length before it), by
//! volume, level, message template and `field=value` pair.
//!
//! One pass over the file: each entry (a line that is not a stack-trace continuation,
//! timed by its own timestamp or the one above, as the histogram does) falls in the span,
//! the baseline or neither. Templates are learnt with the Drain tree of `patterns`, one
//! tree for both sides so their counts compare. A baseline of more than
//! `MAX_BASELINE_ENTRIES` is sampled every `k`-th entry and its counts scaled by `k`.
//! Items are ranked by a smoothed log-ratio: frequent in the span and over-represented
//! there, not just frequent.

use std::collections::HashMap;

use crate::fields::{FieldParser, FieldSpans};
use crate::patterns::{Drain, Scratch};
use crate::tail_engine::NO_TIMESTAMP;

/// Baseline entries counted before sampling starts.
pub const MAX_BASELINE_ENTRIES: u64 = 1_000_000;
/// Items listed per section at most.
pub const TOP_ITEMS: usize = 20;
/// Occurrences in the span an item needs to be listed.
pub const MIN_SPAN_COUNT: u64 = 3;
/// Distinct values of a field counted in the span; a field with more is skipped.
pub const MAX_FIELD_VALUES: usize = 1_000;
/// Bytes of a field value kept.
pub const MAX_VALUE_BYTES: usize = 256;
/// Levels counted (`LogLevel as u8`, `Unknown` included).
pub const LEVELS: usize = 7;

/// What the span is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Baseline {
    /// Every timed entry outside the span.
    #[default]
    Rest,
    /// The entries of the span of equal length ending where the span starts.
    Preceding,
}

/// The two sides of a comparison, in milliseconds (both ends included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sides {
    pub span: (i64, i64),
    pub baseline: Baseline,
    /// Count every `k`-th baseline entry (1: all).
    pub sample: u64,
}

impl Sides {
    /// 0 for the span, 1 for the baseline, `None` for neither.
    pub fn side_of(&self, ts: i64) -> Option<usize> {
        if ts == NO_TIMESTAMP {
            return None;
        }
        let (a, b) = self.span;
        if (a..=b).contains(&ts) {
            return Some(0);
        }
        match self.baseline {
            Baseline::Rest => Some(1),
            Baseline::Preceding => {
                let len = b - a + 1;
                (ts >= a - len && ts < a).then_some(1)
            }
        }
    }
}

/// Counts of one item on both sides (the baseline's not yet scaled).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pair {
    pub span: u64,
    pub base: u64,
}

/// The counts of a comparison.
#[derive(Debug, Clone)]
pub struct Explanation {
    pub sides: Sides,
    /// Entries counted per side (the baseline's sampled).
    pub entries: [u64; 2],
    /// Baseline entries seen, sampled or not.
    pub base_seen: u64,
    /// First and last timestamp seen per side, for the volume.
    pub time: [(i64, i64); 2],
    pub levels: [[u64; LEVELS]; 2],
    pub drain: Drain,
    /// Per template id: counts, and the first line of the span holding it.
    pub templates: HashMap<u32, (Pair, usize)>,
    /// Entries in no template (past the caps).
    pub other: Pair,
    /// Per field key: per value, counts. Keys over the cap are moved to `dropped`.
    pub fields: HashMap<Box<str>, HashMap<Box<str>, Pair>>,
    pub dropped: Vec<Box<str>>,
}

/// One ranked item.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked<T> {
    pub item: T,
    pub span: u64,
    /// The baseline count, scaled by the sampling.
    pub base: u64,
    /// The share ratio, `None` when the item is absent from the baseline (new).
    pub ratio: Option<f64>,
    pub score: f64,
}

impl Explanation {
    pub fn new(sides: Sides) -> Self {
        Self {
            sides,
            entries: [0; 2],
            base_seen: 0,
            time: [(NO_TIMESTAMP, NO_TIMESTAMP); 2],
            levels: [[0; LEVELS]; 2],
            drain: Drain::new(),
            templates: HashMap::new(),
            other: Pair::default(),
            fields: HashMap::new(),
            dropped: Vec::new(),
        }
    }

    /// Whether the baseline was sampled.
    pub fn sampled(&self) -> bool {
        self.sides.sample > 1
    }

    /// Counts the entry `line` (line number `idx`, timestamp `ts`, level `level`) when it
    /// falls in a side; `scratch` and `spans` are reused buffers.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        line: &str,
        idx: usize,
        ts: i64,
        level: u8,
        parser: Option<&FieldParser>,
        scratch: &mut Scratch,
        spans: &mut FieldSpans,
    ) {
        let Some(side) = self.sides.side_of(ts) else {
            return;
        };
        if side == 1 {
            self.base_seen += 1;
            if !(self.base_seen - 1).is_multiple_of(self.sides.sample.max(1)) {
                return;
            }
        }
        self.entries[side] += 1;
        let t = &mut self.time[side];
        if t.0 == NO_TIMESTAMP || ts < t.0 {
            t.0 = ts;
        }
        if t.1 == NO_TIMESTAMP || ts > t.1 {
            t.1 = ts;
        }
        self.levels[side][(level as usize).min(LEVELS - 1)] += 1;
        let count = |p: &mut Pair| {
            if side == 0 {
                p.span += 1;
            } else {
                p.base += 1;
            }
        };
        match self.drain.learn(line, scratch) {
            Some(id) => {
                let e = self
                    .templates
                    .entry(id)
                    .or_insert((Pair::default(), usize::MAX));
                count(&mut e.0);
                if side == 0 && e.1 == usize::MAX {
                    e.1 = idx;
                }
            }
            None => count(&mut self.other),
        }
        let Some(parser) = parser else {
            return;
        };
        crate::fields::scan(parser, line, spans);
        for f in spans.iter(line) {
            let key = f.key;
            if self.dropped.iter().any(|d| **d == *key) {
                continue;
            }
            let value = f.value();
            let mut end = value.len().min(MAX_VALUE_BYTES);
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            let value = &value[..end];
            let values = match self.fields.get_mut(key) {
                Some(v) => v,
                None => self.fields.entry(key.into()).or_default(),
            };
            if let Some(p) = values.get_mut(value) {
                count(p);
            } else if values.len() < MAX_FIELD_VALUES {
                let mut p = Pair::default();
                count(&mut p);
                values.insert(value.into(), p);
            } else if side == 0 {
                // Too many values in the span: an id or a time, not a difference.
                self.fields.remove(key);
                self.dropped.push(key.into());
            }
        }
    }

    /// The baseline count of `p`, scaled by the sampling.
    fn scaled(&self, b: u64) -> u64 {
        b * self.sides.sample.max(1)
    }

    /// Ranks `items` (counts on both sides) by `a · ln(p / q)` with `p` and `q` the
    /// smoothed shares, keeping those at least `MIN_SPAN_COUNT` times in the span and more
    /// frequent there, at most `TOP_ITEMS`.
    pub fn rank<T: Clone>(&self, items: &[(T, Pair)]) -> Vec<Ranked<T>> {
        let a_total = self.entries[0] as f64;
        let b_total = self.scaled(self.entries[1]) as f64;
        let v = items.len().max(1) as f64;
        let mut out: Vec<Ranked<T>> = items
            .iter()
            .filter(|(_, c)| c.span >= MIN_SPAN_COUNT)
            .filter_map(|(item, c)| {
                let a = c.span as f64;
                let b = self.scaled(c.base) as f64;
                let p = (a + 0.5) / (a_total + 0.5 * v);
                let q = (b + 0.5) / (b_total + 0.5 * v);
                (p > q).then(|| Ranked {
                    item: item.clone(),
                    span: c.span,
                    base: b as u64,
                    ratio: (c.base > 0).then_some(p / q),
                    score: a * (p / q).ln(),
                })
            })
            .collect();
        out.sort_by(|x, y| y.score.total_cmp(&x.score));
        out.truncate(TOP_ITEMS);
        out
    }

    /// The message templates over-represented in the span: id, counts, first line.
    pub fn messages(&self) -> Vec<(Ranked<u32>, usize)> {
        let items: Vec<(u32, Pair)> = self
            .templates
            .iter()
            .map(|(&id, (p, _))| (id, *p))
            .collect();
        self.rank(&items)
            .into_iter()
            .map(|r| {
                let first = self.templates.get(&r.item).map_or(usize::MAX, |t| t.1);
                (r, first)
            })
            .collect()
    }

    /// The `field=value` pairs over-represented in the span.
    pub fn field_pairs(&self) -> Vec<Ranked<(String, String)>> {
        let items: Vec<((String, String), Pair)> = self
            .fields
            .iter()
            .flat_map(|(k, values)| {
                values
                    .iter()
                    .map(move |(v, p)| ((k.to_string(), v.to_string()), *p))
            })
            .collect();
        self.rank(&items)
    }

    /// Entries per minute of side `side` over its time span (`span_ms` when known).
    pub fn per_minute(&self, side: usize, span_ms: i64) -> f64 {
        let n = if side == 1 {
            self.scaled(self.entries[1])
        } else {
            self.entries[0]
        } as f64;
        n * 60_000.0 / span_ms.max(1) as f64
    }
}

/// The Explain tab of a stream: the comparison asked for, its scan and its counts.
pub struct ExplainState {
    pub sides: Sides,
    /// Only the entries the stream's filters show.
    pub only_filtered: bool,
    pub(crate) job: Option<crate::scan_job::ScanJob>,
    pub(crate) job_generation: u64,
    pub progress: f32,
    /// The counts so far (filled in as the scan sends them).
    pub result: Option<Explanation>,
    /// The file could not be read.
    pub failed: bool,
    /// The file was truncated, rotated or rewritten since: the counts are of the old one.
    pub stale: bool,
    /// Bumped whenever `result` changes.
    pub generation: u64,
}

impl ExplainState {
    pub(crate) fn new(sides: Sides, only_filtered: bool) -> Self {
        Self {
            sides,
            only_filtered,
            job: None,
            job_generation: 0,
            progress: 0.0,
            result: None,
            failed: false,
            stale: false,
            generation: 0,
        }
    }

    pub fn running(&self) -> bool {
        self.job.is_some()
    }
}

/// The sampling interval for a baseline of about `entries`: every entry up to
/// `MAX_BASELINE_ENTRIES`, then one in `k`.
pub fn sample_for(entries: u64) -> u64 {
    entries.div_ceil(MAX_BASELINE_ENTRIES).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explain(lines: &[(i64, &str, u8)], sides: Sides) -> Explanation {
        let mut e = Explanation::new(sides);
        let mut s = Scratch::default();
        let mut spans = FieldSpans::new();
        for (i, (ts, line, level)) in lines.iter().enumerate() {
            e.add(line, i, *ts, *level, None, &mut s, &mut spans);
        }
        e
    }

    fn log() -> Vec<(i64, String, u8)> {
        let mut v = Vec::new();
        for s in 0..600i64 {
            v.push((s * 1000, format!("heartbeat {s} ok"), 3));
            if (300..360).contains(&s) {
                for k in 0..5 {
                    v.push((
                        s * 1000 + k,
                        format!("payment {s}{k} failed: gateway timeout"),
                        5,
                    ));
                }
            }
            if s % 100 == 0 {
                v.push((
                    s * 1000 + 7,
                    format!("payment {s} failed: gateway timeout"),
                    5,
                ));
            }
        }
        v
    }

    #[test]
    fn a_burst_ranks_first_and_a_heartbeat_is_no_difference() {
        let owned = log();
        let lines: Vec<(i64, &str, u8)> =
            owned.iter().map(|(t, l, v)| (*t, l.as_str(), *v)).collect();
        let sides = Sides {
            span: (300_000, 359_999),
            baseline: Baseline::Rest,
            sample: 1,
        };
        let e = explain(&lines, sides);
        assert_eq!(e.entries[0], 60 + 301);
        let msgs = e.messages();
        let (top, first) = &msgs[0];
        assert_eq!(
            e.drain.template(top.item).unwrap().text(),
            "payment <*> failed: gateway timeout"
        );
        // 300 in the burst, and the one at 300 s of the steady trickle.
        assert_eq!(top.span, 301);
        // Rare in the baseline, not new.
        assert!(top.ratio.is_some_and(|r| r > 10.0));
        assert!(*first >= 300);
        assert!(msgs.iter().all(|(r, _)| !e
            .drain
            .template(r.item)
            .unwrap()
            .text()
            .starts_with("heartbeat")));
        // Errors: most of the span, a sliver of the baseline.
        assert_eq!(e.levels[0][5], 301);
        // The preceding minute holds no payment at all: new there.
        let e = explain(
            &lines,
            Sides {
                baseline: Baseline::Preceding,
                ..sides
            },
        );
        assert_eq!(e.entries[1], 60);
        assert!(e.messages()[0].0.ratio.is_none());
    }

    #[test]
    fn a_sampled_baseline_scales_its_counts() {
        let owned = log();
        let lines: Vec<(i64, &str, u8)> =
            owned.iter().map(|(t, l, v)| (*t, l.as_str(), *v)).collect();
        let sides = Sides {
            span: (300_000, 359_999),
            baseline: Baseline::Rest,
            sample: 10,
        };
        let e = explain(&lines, sides);
        assert!(e.sampled());
        // 540 heartbeats and 5 payments outside the span, one in ten counted.
        assert_eq!(e.base_seen, 545);
        assert_eq!(e.entries[1], 55);
        assert!((e.per_minute(1, 545 * 1000) - 60.0 * 550.0 / 545.0).abs() < 1.0);
        assert_eq!(e.messages()[0].0.span, 301);
    }

    #[test]
    fn the_sampling_starts_past_a_million() {
        assert_eq!(sample_for(0), 1);
        assert_eq!(sample_for(1_000_000), 1);
        assert_eq!(sample_for(1_000_001), 2);
        assert_eq!(sample_for(6_500_000), 7);
    }

    #[test]
    fn untimed_lines_and_lines_outside_both_sides_are_left_out() {
        let sides = Sides {
            span: (10, 20),
            baseline: Baseline::Preceding,
            sample: 1,
        };
        assert_eq!(sides.side_of(NO_TIMESTAMP), None);
        assert_eq!(sides.side_of(15), Some(0));
        assert_eq!(sides.side_of(0), Some(1));
        assert_eq!(sides.side_of(-1), Some(1));
        assert_eq!(sides.side_of(-2), None);
        assert_eq!(sides.side_of(21), None);
    }
}
