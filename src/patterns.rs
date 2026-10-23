// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Patterns of the lines a stream shows: each entry is assigned to a template whose
//! variable parts are `<*>` (`payment <*> failed after <*> ms: gateway timeout`).
//!
//! Templates are learnt with Drain (He et al., 2017): the first line of an entry, without
//! its leading timestamp and with numbers, hex values and UUIDs masked as collapse's
//! Numbers mode does (an IP address becomes `#.#.#.#`), is split on whitespace and routed
//! through a tree of depth 4 (Drain's default: the root, the token count, the first token
//! and the leaf; a first token holding a masked number routes as `<*>`). A leaf holds up to
//! `LEAF_TEMPLATES` templates; an entry joins the one of equal length sharing the most
//! tokens when they are at least `SIMILARITY` of them (the tokens that differ become
//! `<*>`), otherwise it starts a template.
//!
//! Learning and counting are two passes: `Drain::learn` builds the tree, then every entry
//! is assigned with `Drain::classify` on the finished tree, which never changes it. The
//! pattern filter classifies with the same tree, so filtering to a pattern shows exactly
//! the entries counted for it. Memory is the tree and the per-pattern counts, nothing per
//! line.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::collapse::{normalize_line, CollapseMode};
use crate::tail_engine::NO_TIMESTAMP;
use crate::timestamp::FormatHint;

/// Templates per stream at most; entries that would start another are counted as
/// "other patterns".
pub const MAX_PATTERNS: usize = 2_000;
/// Bytes of a line clustered, and tokens of it.
pub const MAX_LINE_BYTES: usize = 4_096;
pub const MAX_TOKENS: usize = 128;
/// Templates in one leaf of the tree at most.
pub const LEAF_TEMPLATES: usize = 100;
/// Share of equal tokens an entry needs to join a template.
pub const SIMILARITY: f64 = 0.5;
/// Buckets of the volume sparkline of a pattern.
pub const BUCKETS: usize = 64;

/// The text `<*>` stands for in a template.
pub const WILDCARD: &str = "<*>";

/// Buffers reused from line to line: the normalised text and its tokens.
#[derive(Debug, Default)]
pub struct Scratch {
    text: Vec<u8>,
    tokens: Vec<(u32, u32)>,
}

impl Scratch {
    /// Normalises `line` and splits it into tokens; false when nothing is left.
    fn load(&mut self, line: &str) -> bool {
        let mut end = line.len().min(MAX_LINE_BYTES);
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        self.text.clear();
        self.tokens.clear();
        normalize_line(
            &line[..end],
            CollapseMode::Numbers,
            FormatHint::default(),
            &mut self.text,
        );
        let mut start: Option<usize> = None;
        for (i, &b) in self.text.iter().enumerate() {
            let space = b.is_ascii_whitespace();
            match (start, space) {
                (None, false) => start = Some(i),
                (Some(s), true) => {
                    self.tokens.push((s as u32, i as u32));
                    start = None;
                    if self.tokens.len() == MAX_TOKENS {
                        return true;
                    }
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            self.tokens.push((s as u32, self.text.len() as u32));
        }
        !self.tokens.is_empty()
    }

    fn token(&self, i: usize) -> &str {
        let (a, b) = self.tokens[i];
        // The text is the line's (valid UTF-8) with ASCII masks: cut on whitespace only.
        std::str::from_utf8(&self.text[a as usize..b as usize]).unwrap_or("")
    }

    fn len(&self) -> usize {
        self.tokens.len()
    }

    /// The routing key of token `i`: itself, or `<*>` when it holds a masked number.
    fn key(&self, i: usize) -> &str {
        if i >= self.len() {
            return "";
        }
        let t = self.token(i);
        if t.contains('#') {
            WILDCARD
        } else {
            t
        }
    }
}

/// One template: its tokens, `None` where it is variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    tokens: Vec<Option<Box<str>>>,
}

impl Template {
    /// The template as shown: tokens joined by a space, `<*>` where variable (a masked
    /// number inside a token is shown as `<*>` too).
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, t) in self.tokens.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            match t {
                Some(t) => out.push_str(&t.replace('#', WILDCARD)),
                None => out.push_str(WILDCARD),
            }
        }
        out
    }

    /// The longest run of fixed tokens without a masked number, joined by a space: text
    /// that every entry of the pattern holds, for a search.
    pub fn fixed_words(&self) -> String {
        let mut best: &[Option<Box<str>>] = &[];
        let mut run_start = 0;
        for i in 0..=self.tokens.len() {
            let fixed = self
                .tokens
                .get(i)
                .is_some_and(|t| t.as_deref().is_some_and(|t| !t.contains('#')));
            if !fixed {
                if i - run_start > best.len() {
                    best = &self.tokens[run_start..i];
                }
                run_start = i + 1;
            }
        }
        best.iter()
            .filter_map(|t| t.as_deref())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whether every token is variable.
    pub fn is_all_wildcards(&self) -> bool {
        self.tokens
            .iter()
            .all(|t| t.as_deref().is_none_or(|t| t.chars().all(|c| c == '#')))
    }

    /// Equal fixed tokens, and variable ones, against the entry in `s`.
    fn compare(&self, s: &Scratch) -> (usize, usize) {
        let mut equal = 0;
        let mut wild = 0;
        for (i, t) in self.tokens.iter().enumerate() {
            match t {
                None => wild += 1,
                Some(t) if **t == *s.token(i) => equal += 1,
                Some(_) => {}
            }
        }
        (equal, wild)
    }
}

/// The routing tree and its templates (ids are positions in `templates`).
#[derive(Debug, Clone, Default)]
pub struct Drain {
    leaves: HashMap<u16, HashMap<Box<str>, Vec<u32>>>,
    templates: Vec<Template>,
}

impl Drain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn template(&self, id: u32) -> Option<&Template> {
        self.templates.get(id as usize)
    }

    fn leaf(&self, s: &Scratch) -> Option<&Vec<u32>> {
        self.leaves.get(&(s.len() as u16))?.get(s.key(0))
    }

    /// Learns the entry whose first line is `line`: the template it joins (generalised
    /// where they differ) or the one it starts; `None` past the caps.
    pub fn learn(&mut self, line: &str, s: &mut Scratch) -> Option<u32> {
        if !s.load(line) {
            return None;
        }
        let n = s.len();
        let mut best: Option<(u32, usize, usize)> = None;
        if let Some(ids) = self.leaf(s) {
            for &id in ids {
                let (equal, wild) = self.templates[id as usize].compare(s);
                let better = best.is_none_or(|(_, e, w)| equal > e || (equal == e && wild > w));
                if better {
                    best = Some((id, equal, wild));
                }
            }
        }
        if let Some((id, equal, _)) = best {
            if equal as f64 >= SIMILARITY * n as f64 {
                let t = &mut self.templates[id as usize];
                for (i, tok) in t.tokens.iter_mut().enumerate() {
                    if tok.as_deref().is_some_and(|tok| tok != s.token(i)) {
                        *tok = None;
                    }
                }
                return Some(id);
            }
        }
        if self.templates.len() >= MAX_PATTERNS {
            return None;
        }
        let leaf = self
            .leaves
            .entry(n as u16)
            .or_default()
            .entry(s.key(0).into())
            .or_default();
        if leaf.len() >= LEAF_TEMPLATES {
            return None;
        }
        let id = self.templates.len() as u32;
        leaf.push(id);
        self.templates.push(Template {
            tokens: (0..n).map(|i| Some(s.token(i).into())).collect(),
        });
        Some(id)
    }

    /// The template `line` belongs to on this tree, without changing it: the one sharing
    /// the most tokens, a variable one counting as shared, when they are at least
    /// `SIMILARITY` of them (the lowest id on a tie).
    pub fn classify(&self, line: &str, s: &mut Scratch) -> Option<u32> {
        if !s.load(line) {
            return None;
        }
        let n = s.len();
        let mut best: Option<(u32, usize)> = None;
        for &id in self.leaf(s)? {
            let (equal, wild) = self.templates[id as usize].compare(s);
            let score = equal + wild;
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((id, score));
            }
        }
        best.filter(|&(_, score)| score as f64 >= SIMILARITY * n as f64)
            .map(|(id, _)| id)
    }
}

/// What the sparklines are drawn over: timestamps, or line numbers on an untimed stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axis {
    pub timed: bool,
    pub start: i64,
    /// Width of one bucket (at least 1); doubled when a value lies past the last bucket.
    pub width: i64,
}

impl Axis {
    /// `BUCKETS` buckets over `[start, end]`.
    pub fn new(timed: bool, start: i64, end: i64) -> Self {
        let span = (end - start).max(0) + 1;
        Self {
            timed,
            start,
            width: ((span + BUCKETS as i64 - 1) / BUCKETS as i64).max(1),
        }
    }

    fn bucket(&self, value: i64) -> Option<usize> {
        if value < self.start {
            return None;
        }
        usize::try_from((value - self.start) / self.width)
            .ok()
            .filter(|&b| b < BUCKETS)
    }
}

/// The counts of one pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternStat {
    pub count: u64,
    pub first_line: usize,
    pub last_line: usize,
    /// `NO_TIMESTAMP` when none of its entries is timed.
    pub first_ts: i64,
    pub last_ts: i64,
    pub buckets: [u32; BUCKETS],
}

impl Default for PatternStat {
    fn default() -> Self {
        Self {
            count: 0,
            first_line: usize::MAX,
            last_line: 0,
            first_ts: NO_TIMESTAMP,
            last_ts: NO_TIMESTAMP,
            buckets: [0; BUCKETS],
        }
    }
}

impl PatternStat {
    fn merge(&mut self, o: &PatternStat) {
        if o.count == 0 {
            return;
        }
        self.count += o.count;
        self.first_line = self.first_line.min(o.first_line);
        self.last_line = self.last_line.max(o.last_line);
        if o.first_ts != NO_TIMESTAMP {
            if self.first_ts == NO_TIMESTAMP || o.first_ts < self.first_ts {
                self.first_ts = o.first_ts;
            }
            if self.last_ts == NO_TIMESTAMP || o.last_ts > self.last_ts {
                self.last_ts = o.last_ts;
            }
        }
        for (a, b) in self.buckets.iter_mut().zip(o.buckets.iter()) {
            *a += b;
        }
    }
}

/// The counts of every pattern, by id, and of the entries in none.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternCounts {
    pub axis: Axis,
    pub per: Vec<PatternStat>,
    /// Entries past the caps ("other patterns").
    pub other: u64,
    /// Entries counted, `other` included.
    pub entries: u64,
}

impl PatternCounts {
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            per: Vec::new(),
            other: 0,
            entries: 0,
        }
    }

    /// Counts the entry on line `line` (timestamp `ts`, `NO_TIMESTAMP` when untimed) for
    /// pattern `id`.
    pub fn add(&mut self, id: Option<u32>, line: usize, ts: i64) {
        self.entries += 1;
        let Some(id) = id else {
            self.other += 1;
            return;
        };
        let id = id as usize;
        if self.per.len() <= id {
            self.per.resize_with(id + 1, PatternStat::default);
        }
        let value = if self.axis.timed {
            (ts != NO_TIMESTAMP).then_some(ts)
        } else {
            Some(line as i64)
        };
        if let Some(v) = value {
            while v >= self.axis.start + self.axis.width * BUCKETS as i64 {
                self.double_width();
            }
        }
        let bucket = value.and_then(|v| self.axis.bucket(v));
        let p = &mut self.per[id];
        p.count += 1;
        p.first_line = p.first_line.min(line);
        p.last_line = p.last_line.max(line);
        if ts != NO_TIMESTAMP {
            if p.first_ts == NO_TIMESTAMP || ts < p.first_ts {
                p.first_ts = ts;
            }
            if p.last_ts == NO_TIMESTAMP || ts > p.last_ts {
                p.last_ts = ts;
            }
        }
        if let Some(b) = bucket {
            p.buckets[b] += 1;
        }
    }

    /// Buckets twice as wide: pairs are added together.
    fn double_width(&mut self) {
        self.axis.width *= 2;
        for p in &mut self.per {
            for i in 0..BUCKETS {
                p.buckets[i] = if i < BUCKETS / 2 {
                    p.buckets[2 * i] + p.buckets[2 * i + 1]
                } else {
                    0
                };
            }
        }
    }

    /// Adds counts made over the same axis start (the wider buckets win).
    pub fn merge(&mut self, mut o: PatternCounts) {
        while self.axis.width < o.axis.width {
            self.double_width();
        }
        while o.axis.width < self.axis.width {
            o.double_width();
        }
        if self.per.len() < o.per.len() {
            self.per.resize_with(o.per.len(), PatternStat::default);
        }
        for (a, b) in self.per.iter_mut().zip(o.per.iter()) {
            a.merge(b);
        }
        self.other += o.other;
        self.entries += o.entries;
    }

    /// Ids of the patterns with entries.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.per
            .iter()
            .enumerate()
            .filter(|(_, p)| p.count > 0)
            .map(|(i, _)| i as u32)
    }
}

/// The pattern filter of a stream: the entries of the `include` patterns (all when it is
/// empty) that are not in an `exclude` one, classified on `drain`.
#[derive(Debug, Clone)]
pub struct PatternFilter {
    pub drain: Arc<Drain>,
    pub include: Vec<u32>,
    pub exclude: Vec<u32>,
}

/// The Patterns tab of a stream, while it is open: learning, then counting, in the
/// background, and the appended entries learnt and counted as they come.
pub struct PatternState {
    pub(crate) job: Option<crate::scan_job::ScanJob>,
    pub(crate) job_generation: u64,
    /// The running job counts (it learnt before).
    pub counting: bool,
    pub progress: f32,
    /// The view changed (filters, reload): learn again from scratch on the next poll.
    pub(crate) restart: bool,
    /// The file could not be read.
    pub failed: bool,
    /// The tree, extended by the appended entries.
    pub(crate) drain: Drain,
    /// The tree as last published: what the counts and the pattern filter use.
    pub snapshot: Option<Arc<Drain>>,
    pub counts: PatternCounts,
    /// The lines learnt and counted: `0..covered` (visible entries only).
    pub(crate) covered: usize,
    /// The timestamp an appended line without its own inherits.
    pub(crate) last_ts: i64,
    /// Whether the last appended entry was visible (its continuation lines follow it).
    pub(crate) parent_visible: bool,
    /// Appended entries changed the tree since it was published, and when it was.
    pub(crate) dirty: bool,
    pub(crate) published: std::time::Instant,
    /// Bumped whenever the counts or the tree change, for views that cache what they draw.
    pub generation: u64,
}

impl PatternState {
    pub(crate) fn new() -> Self {
        Self {
            job: None,
            job_generation: 0,
            counting: false,
            progress: 0.0,
            restart: true,
            failed: false,
            drain: Drain::new(),
            snapshot: None,
            counts: PatternCounts::new(Axis::new(false, 0, 0)),
            covered: 0,
            last_ts: NO_TIMESTAMP,
            parent_visible: false,
            dirty: false,
            published: std::time::Instant::now(),
            generation: 0,
        }
    }

    /// The tree as it is now, appended entries included: what the tab shows.
    pub fn tree(&self) -> &Drain {
        &self.drain
    }

    /// The tree published now when appended entries changed it (the pattern filter takes
    /// the published one).
    pub(crate) fn publish(&mut self) {
        if self.dirty {
            self.snapshot = Some(Arc::new(self.drain.clone()));
            self.published = std::time::Instant::now();
            self.dirty = false;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    /// Whether a learning or counting scan is running (or waits to start).
    pub fn running(&self) -> bool {
        self.job.is_some() || self.restart
    }
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

impl PatternFilter {
    /// Whether the entry whose first line is `line` passes.
    pub fn passes(&self, line: &str) -> bool {
        let id = SCRATCH.with(|s| self.drain.classify(line, &mut s.borrow_mut()));
        match id {
            Some(id) => {
                (self.include.is_empty() || self.include.contains(&id))
                    && !self.exclude.contains(&id)
            }
            None => self.include.is_empty(),
        }
    }

    /// Patterns named by the filter.
    pub fn len(&self) -> usize {
        self.include.len() + self.exclude.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learn_all(lines: &[&str]) -> (Drain, Vec<Option<u32>>) {
        let mut d = Drain::new();
        let mut s = Scratch::default();
        let ids = lines.iter().map(|l| d.learn(l, &mut s)).collect();
        (d, ids)
    }

    #[test]
    fn variable_parts_become_wildcards() {
        let lines = [
            "2026-10-24 10:00:01 payment 4411 failed after 120 ms: gateway timeout",
            "2026-10-24 10:00:02 payment 9 failed after 3000 ms: gateway timeout",
            "2026-10-24 10:00:03 user alice logged in from 10.0.0.1",
            "2026-10-24 10:00:04 user bob logged in from 192.168.1.20",
            "2026-10-24 10:00:05 cache hit ratio 0x1f ok",
        ];
        let (d, ids) = learn_all(&lines);
        assert_eq!(ids[0], ids[1]);
        assert_eq!(ids[2], ids[3]);
        assert_ne!(ids[0], ids[2]);
        let t = |i: usize| d.template(ids[i].unwrap()).unwrap().text();
        assert_eq!(t(0), "payment <*> failed after <*> ms: gateway timeout");
        // The names differ, the masked addresses do not.
        assert_eq!(t(2), "user <*> logged in from <*>.<*>.<*>.<*>");
        assert_eq!(t(4), "cache hit ratio <*> ok");
        assert_eq!(
            d.template(ids[0].unwrap()).unwrap().fixed_words(),
            "ms: gateway timeout"
        );
    }

    #[test]
    fn different_lengths_and_dissimilar_lines_stay_apart() {
        let (d, ids) = learn_all(&[
            "connection opened",
            "connection opened to backend",
            "disk full on volume",
            "queue empty on worker",
        ]);
        assert_eq!(d.len(), 4);
        assert!(ids.iter().all(Option::is_some));
    }

    #[test]
    fn classify_finds_the_template_of_every_learnt_entry() {
        let lines: Vec<String> = (0..500)
            .map(|i| match i % 5 {
                0 => format!("GET /api/users/{i} 200 {}ms", i % 97),
                1 => format!("job {i} finished in {} s", i % 13),
                2 => format!("user u{i} logged out"),
                3 => format!("retrying request {i} attempt {}", i % 3),
                _ => format!("worker-{} heartbeat", i % 7),
            })
            .collect();
        let mut d = Drain::new();
        let mut s = Scratch::default();
        for l in &lines {
            d.learn(l, &mut s);
        }
        assert!(d.len() <= 6, "{} templates", d.len());
        for l in &lines {
            let id = d.classify(l, &mut s).expect("classified");
            // The template holds the entry: its fixed tokens are the entry's.
            let mut probe = Scratch::default();
            probe.load(l);
            let t = d.template(id).unwrap();
            assert_eq!(t.compare(&probe).0 + t.compare(&probe).1, probe.len());
        }
        // Classifying never changes the tree.
        let before = d.clone();
        d.classify("something never seen before", &mut s);
        assert_eq!(d.templates, before.templates);
    }

    #[test]
    fn a_full_leaf_sends_entries_to_other() {
        // Letters only (a digit would be masked): every line its own word triple, so two
        // shared tokens of five never reach the similarity and each starts a template.
        let word = |i: usize, salt: usize| -> String {
            (0..4)
                .map(|k| (b'a' + ((i / 26usize.pow(k) + salt) % 26) as u8) as char)
                .collect()
        };
        let mut d = Drain::new();
        let mut s = Scratch::default();
        let mut other = 0;
        for i in 0..(LEAF_TEMPLATES + 5) {
            let line = format!("k z {} {} {}", word(i, 0), word(i, 7), word(i, 13));
            if d.learn(&line, &mut s).is_none() {
                other += 1;
            }
        }
        assert_eq!(d.len(), LEAF_TEMPLATES);
        assert_eq!(other, 5);
    }

    #[test]
    fn counts_rebin_and_merge() {
        let mut c = PatternCounts::new(Axis::new(false, 0, 63));
        assert_eq!(c.axis.width, 1);
        c.add(Some(0), 0, NO_TIMESTAMP);
        c.add(Some(0), 63, NO_TIMESTAMP);
        c.add(None, 5, NO_TIMESTAMP);
        // Past the last bucket: twice as wide.
        c.add(Some(1), 100, NO_TIMESTAMP);
        assert_eq!(c.axis.width, 2);
        assert_eq!(c.per[0].buckets[0], 1);
        assert_eq!(c.per[0].buckets[31], 1);
        assert_eq!(c.per[1].buckets[50], 1);
        assert_eq!((c.entries, c.other), (4, 1));
        let mut d = PatternCounts::new(Axis::new(false, 0, 63));
        d.add(Some(0), 10, NO_TIMESTAMP);
        c.merge(d);
        assert_eq!(c.per[0].count, 3);
        assert_eq!(c.per[0].buckets[5], 1);
        assert_eq!((c.per[0].first_line, c.per[0].last_line), (0, 63));
        assert_eq!(c.ids().collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn the_filter_passes_its_patterns() {
        let (d, ids) = learn_all(&["a 1 done", "a 2 done", "b failed now", "b failed later"]);
        let f = PatternFilter {
            drain: Arc::new(d),
            include: vec![ids[0].unwrap()],
            exclude: vec![],
        };
        assert!(f.passes("a 77 done"));
        assert!(!f.passes("b failed now"));
        assert!(!f.passes("never seen"));
        let f = PatternFilter {
            include: vec![],
            exclude: vec![ids[0].unwrap()],
            ..f
        };
        assert!(!f.passes("a 3 done"));
        assert!(f.passes("b failed now"));
        assert!(f.passes("never seen"));
    }
}
