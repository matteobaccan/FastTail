// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Named (kept) searches: up to `MAX_NAMED` searches of one stream highlighted at once,
//! each in its own colour, walked together or one at a time (as lnav 0.15's named
//! searches).
//!
//! Each kept search runs its own background job: a one-stream `FindAllSession`, so it
//! never cancels the stream's own search and matches as it does (case-insensitive text
//! over the lines the stream shows). The hits are a snapshot of the file as indexed when
//! the job started; a reloaded stream searches again at once, a grown one at most every
//! `RERUN_EVERY` once its job is done.

use std::time::{Duration, Instant};

use crate::find_all::{FindAllSession, FindState};
use crate::tail_engine::TailEngine;

/// Most searches one stream keeps.
pub const MAX_NAMED: usize = 8;

/// Hits listed per kept search; past it they are counted.
pub const MAX_HITS: usize = 100_000;

/// Shortest wait before a grown stream's kept searches run again.
const RERUN_EVERY: Duration = Duration::from_secs(3);

/// One kept search.
pub struct NamedSearch {
    pub query: String,
    /// Its colour, 1 to 8 (the quick labels' colours).
    pub color: u8,
    session: FindAllSession,
    /// Lines of the stream and its reload generation when the job started.
    lines: usize,
    generation: u64,
    started: Instant,
}

impl NamedSearch {
    fn new(query: &str, color: u8) -> Self {
        Self {
            query: query.trim().to_string(),
            color,
            session: FindAllSession::with_limits(1, MAX_HITS),
            lines: 0,
            generation: u64::MAX,
            started: Instant::now(),
        }
    }

    /// The matching lines found, in file order.
    pub fn hits(&self) -> &[usize] {
        self.session
            .groups
            .first()
            .map_or(&[], |g| g.hits.as_slice())
    }

    /// Every match, the ones past the cap included.
    pub fn total(&self) -> usize {
        self.session.groups.first().map_or(0, |g| g.total)
    }

    /// Its job is still counting.
    pub fn running(&self) -> bool {
        self.session.is_active()
    }

    fn run(&mut self, engine: &TailEngine) {
        self.session.input = self.query.clone();
        self.session.start(&[engine]);
        self.lines = engine.total_lines();
        self.generation = engine.reload_generation;
        self.started = Instant::now();
    }
}

/// Why a search could not be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepError {
    Empty,
    Full,
    AlreadyKept,
}

/// The kept searches of one stream (`TailEngine::named`).
#[derive(Default)]
pub struct NamedSearches {
    pub list: Vec<NamedSearch>,
    /// The search the walk is limited to (`None`: all of them).
    pub focus: Option<usize>,
    /// Changed since the workspace last saved them.
    pub dirty: bool,
}

impl NamedSearches {
    /// Keeps `query` with the first colour none of the kept ones uses. Returns its colour.
    pub fn keep(&mut self, query: &str) -> Result<u8, KeepError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(KeepError::Empty);
        }
        if self
            .list
            .iter()
            .any(|n| n.query.eq_ignore_ascii_case(query))
        {
            return Err(KeepError::AlreadyKept);
        }
        if self.list.len() >= MAX_NAMED {
            return Err(KeepError::Full);
        }
        let color = (1..=MAX_NAMED as u8)
            .find(|c| !self.list.iter().any(|n| n.color == *c))
            .unwrap_or(1);
        self.list.push(NamedSearch::new(query, color));
        self.dirty = true;
        Ok(color)
    }

    /// Puts back searches saved as `(colour, query)`; their jobs start at the next poll.
    pub fn restore(&mut self, saved: &[(u8, String)]) {
        self.list = saved
            .iter()
            .filter(|(c, q)| (1..=MAX_NAMED as u8).contains(c) && !q.trim().is_empty())
            .take(MAX_NAMED)
            .map(|(c, q)| NamedSearch::new(q, *c))
            .collect();
        self.focus = None;
        self.dirty = false;
    }

    /// The searches as saved: `(colour, query)`.
    pub fn saved(&self) -> Vec<(u8, String)> {
        self.list
            .iter()
            .map(|n| (n.color, n.query.clone()))
            .collect()
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.list.len() {
            self.list.remove(i);
            self.focus = match self.focus {
                Some(f) if f == i => None,
                Some(f) if f > i => Some(f - 1),
                other => other,
            };
            self.dirty = true;
        }
    }

    /// Walks one search (`Some(i)`) or all of them (`None`, also when `i` is the one
    /// focused already).
    pub fn toggle_focus(&mut self, i: usize) {
        self.focus = if self.focus == Some(i) || i >= self.list.len() {
            None
        } else {
            Some(i)
        };
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Starts the jobs that are due and drains the running ones. Returns whether
    /// anything changed.
    pub fn poll(&mut self, engine: &TailEngine) -> bool {
        let mut changed = false;
        for n in &mut self.list {
            let reloaded = n.generation != engine.reload_generation;
            let grown = engine.total_lines() > n.lines
                && !n.running()
                && n.started.elapsed() >= RERUN_EVERY;
            if reloaded || grown {
                n.run(engine);
                changed = true;
                continue;
            }
            if n.running() {
                n.session.poll(&[engine]);
                changed = true;
            }
            if n.session
                .groups
                .first()
                .is_some_and(|g| g.state == FindState::Stale)
            {
                n.generation = u64::MAX;
            }
        }
        changed
    }

    /// The next (`forward`) or previous hit after or before `line` among the focused
    /// search or all of them, wrapping around; with its search.
    pub fn next_hit(&self, line: usize, forward: bool) -> Option<(usize, usize)> {
        let pick = |i: usize| self.focus.is_none_or(|f| f == i);
        let mut best: Option<(usize, usize)> = None;
        let mut wrap: Option<(usize, usize)> = None;
        for (i, n) in self.list.iter().enumerate().filter(|(i, _)| pick(*i)) {
            let hits = n.hits();
            if hits.is_empty() {
                continue;
            }
            let at = hits.partition_point(|&h| h <= line);
            let (next, first) = if forward {
                (hits.get(at).copied(), hits[0])
            } else {
                let before = hits.partition_point(|&h| h < line);
                (before.checked_sub(1).map(|b| hits[b]), hits[hits.len() - 1])
            };
            let better = |a: usize, b: usize| if forward { a < b } else { a > b };
            if let Some(h) = next {
                if best.is_none_or(|(b, _)| better(h, b)) {
                    best = Some((h, i));
                }
            }
            if wrap.is_none_or(|(w, _)| better(first, w)) {
                wrap = Some((first, i));
            }
        }
        best.or(wrap)
    }

    /// The colour of the first kept search line `line` is a hit of (for the gutter).
    pub fn color_of(&self, line: usize) -> Option<u8> {
        self.list
            .iter()
            .find(|n| n.hits().binary_search(&line).is_ok())
            .map(|n| n.color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn engine(body: &str) -> (TailEngine, tempfile::NamedTempFile) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f.flush().unwrap();
        (TailEngine::open(f.path()).unwrap(), f)
    }

    fn settle(n: &mut NamedSearches, e: &TailEngine) {
        let deadline = Instant::now() + Duration::from_secs(10);
        n.poll(e);
        while n.list.iter().any(NamedSearch::running) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            n.poll(e);
        }
    }

    #[test]
    fn two_kept_searches_are_walked_together_or_one_at_a_time() {
        let (e, _f) = engine("ok\ntimeout a\nok\nrefused b\ntimeout c\nok\n");
        let mut n = NamedSearches::default();
        assert_eq!(n.keep("timeout"), Ok(1));
        assert_eq!(n.keep("refused"), Ok(2));
        assert_eq!(n.keep("TIMEOUT"), Err(KeepError::AlreadyKept));
        assert_eq!(n.keep("  "), Err(KeepError::Empty));
        settle(&mut n, &e);
        assert_eq!(n.list[0].hits(), &[1, 4]);
        assert_eq!(n.list[1].total(), 1);
        // From line 1: the next hit of either is `refused` on line 3, then `timeout` on 4.
        assert_eq!(n.next_hit(1, true), Some((3, 1)));
        assert_eq!(n.next_hit(3, true), Some((4, 0)));
        // Past the last it wraps to the first.
        assert_eq!(n.next_hit(4, true), Some((1, 0)));
        assert_eq!(n.next_hit(1, false), Some((4, 0)));
        // Focused on `timeout`, `refused` is skipped.
        n.toggle_focus(0);
        assert_eq!(n.next_hit(1, true), Some((4, 0)));
        assert_eq!(n.color_of(3), Some(2));
        n.remove(0);
        assert_eq!(n.focus, None);
        assert_eq!(n.saved(), vec![(2, "refused".to_string())]);
        // The freed colour 1 goes to the next kept search.
        assert_eq!(n.keep("x"), Ok(1));
    }

    #[test]
    fn at_most_eight_searches_are_kept() {
        let mut n = NamedSearches::default();
        for i in 0..MAX_NAMED {
            n.keep(&format!("q{i}")).unwrap();
        }
        assert_eq!(n.keep("one more"), Err(KeepError::Full));
        let mut back = NamedSearches::default();
        back.restore(&n.saved());
        assert_eq!(back.saved(), n.saved());
        assert!(!back.dirty);
    }
}
