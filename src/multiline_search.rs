// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Multi-line regex search: the visible lines, joined with `\n`, are matched by one regex,
//! so a hit may cross line ends (`BEGIN TRANSACTION.*\n(.*\n){0,4}.*deadlock`).
//!
//! Lines are fed in order to a `Scanner`, which keeps them in a buffer of about
//! `CHUNK_BYTES` and runs the regex over it. A hit ending in the last `OVERLAP_LINES`
//! lines or `OVERLAP_BYTES` bytes of a buffer that more lines may follow is held back: the
//! next buffer starts with those lines, so a hit is neither lost at a boundary nor found
//! twice. A hit longer than `MAX_HIT_LINES` lines or `MAX_HIT_BYTES` bytes is not reported,
//! only counted. Memory is bounded by the buffer, whatever the file size.

use regex::Regex;

/// Bytes gathered before the regex runs over them.
pub const CHUNK_BYTES: usize = 1 << 20;
/// Longest hit reported, in lines and in bytes.
pub const MAX_HIT_LINES: usize = 64;
pub const MAX_HIT_BYTES: usize = 64 * 1024;
/// What the next buffer keeps of the previous one.
const OVERLAP_LINES: usize = MAX_HIT_LINES;
const OVERLAP_BYTES: usize = MAX_HIT_BYTES;

/// A hit: its first and last line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub first: usize,
    pub last: usize,
}

/// The multi-line regex of `pattern`: `^` / `$` match at line starts and ends; `.` crosses
/// line ends only with `(?s)`.
pub fn build(pattern: &str, case_sensitive: bool) -> Result<Regex, String> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .multi_line(true)
        .size_limit(1 << 23)
        .build()
        .map_err(|e| e.to_string())
}

/// Matches a regex over lines fed one by one (see the module documentation).
pub struct Scanner<'r> {
    re: &'r Regex,
    text: String,
    /// The line index and byte offset in `text` of each line held.
    lines: Vec<(usize, usize)>,
    /// Hits too long to report.
    pub skipped: usize,
}

impl<'r> Scanner<'r> {
    pub fn new(re: &'r Regex) -> Self {
        Self {
            re,
            text: String::new(),
            lines: Vec::new(),
            skipped: 0,
        }
    }

    /// Adds line `idx` (its text without the line end); hits found are passed to `hit`,
    /// which returns false to stop the search. Returns false once stopped.
    pub fn push(&mut self, idx: usize, line: &str, hit: &mut impl FnMut(Hit) -> bool) -> bool {
        if !self.lines.is_empty() {
            self.text.push('\n');
        }
        self.lines.push((idx, self.text.len()));
        self.text.push_str(line);
        if self.text.len() >= CHUNK_BYTES {
            return self.scan(false, hit);
        }
        true
    }

    /// No more lines: the hits left in the buffer.
    pub fn finish(&mut self, hit: &mut impl FnMut(Hit) -> bool) -> bool {
        self.scan(true, hit)
    }

    /// Position in `lines` of the line holding byte `at` of `text`.
    fn line_at(&self, at: usize) -> usize {
        self.lines
            .partition_point(|&(_, off)| off <= at)
            .saturating_sub(1)
    }

    /// Runs the regex over the buffer. Unless `last`, hits ending in the overlap are held
    /// back and the buffer restarts where the next scan must begin.
    fn scan(&mut self, last: bool, hit: &mut impl FnMut(Hit) -> bool) -> bool {
        if self.lines.is_empty() {
            return true;
        }
        // Where the overlap starts: the later of 64 lines and 64 KB before the end.
        let by_lines = self
            .lines
            .len()
            .saturating_sub(OVERLAP_LINES)
            .min(self.lines.len() - 1);
        let by_bytes = self.line_at(self.text.len().saturating_sub(OVERLAP_BYTES));
        let overlap = self.lines[by_lines.max(by_bytes)].1;
        let mut resume: Option<usize> = None;
        let mut search_from = 0usize;
        while let Some(m) = self.re.find_at(&self.text, search_from) {
            if !last && m.end() > overlap {
                // It may grow, or a later line may start it: matched again next time.
                resume = Some(m.start());
                break;
            }
            // An empty match moves one character on, as `find_iter` does.
            search_from = if m.is_empty() {
                self.text[m.end()..]
                    .chars()
                    .next()
                    .map_or(self.text.len() + 1, |c| m.end() + c.len_utf8())
            } else {
                m.end()
            };
            if !m.is_empty() {
                // A hit ending right after a line end belongs to the line before it.
                let end_byte = m.end().saturating_sub(1).max(m.start());
                let (a, b) = (self.line_at(m.start()), self.line_at(end_byte));
                if b - a + 1 > MAX_HIT_LINES || m.len() > MAX_HIT_BYTES {
                    self.skipped += 1;
                } else if !hit(Hit {
                    first: self.lines[a].0,
                    last: self.lines[b].0,
                }) {
                    return false;
                }
            }
            if search_from > self.text.len() {
                break;
            }
        }
        if last {
            self.text.clear();
            self.lines.clear();
            return true;
        }
        // The next buffer starts at the held hit's line, or at the overlap.
        let mut keep_from = self.line_at(resume.unwrap_or(overlap).min(overlap));
        // A held hit that already spans more than the overlap can never be reported.
        if resume.is_some_and(|r| r < overlap) && self.lines[keep_from].1 + MAX_HIT_BYTES < overlap
        {
            self.skipped += 1;
            keep_from = self.line_at(overlap);
        }
        let cut = self.lines[keep_from].1;
        self.text.drain(..cut);
        self.lines.drain(..keep_from);
        for l in &mut self.lines {
            l.1 -= cut;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(pattern: &str, lines: &[String]) -> (Vec<Hit>, usize) {
        let re = build(pattern, false).unwrap();
        let mut s = Scanner::new(&re);
        let mut out = Vec::new();
        let mut f = |h: Hit| {
            out.push(h);
            true
        };
        for (i, l) in lines.iter().enumerate() {
            s.push(i, l, &mut f);
        }
        s.finish(&mut f);
        (out, s.skipped)
    }

    #[test]
    fn hits_cross_line_ends_and_anchors_work_per_line() {
        let lines: Vec<String> = [
            "BEGIN TRANSACTION 1",
            "select",
            "deadlock detected",
            "BEGIN TRANSACTION 2",
            "commit",
            "GET /a",
            "GET /b",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (h, _) = hits(r"BEGIN TRANSACTION.*\n(?:.*\n){0,4}.*deadlock", &lines);
        assert_eq!(h, vec![Hit { first: 0, last: 2 }]);
        // A request line followed directly by another one.
        let (h, _) = hits(r"^GET .*\nGET ", &lines);
        assert_eq!(h, vec![Hit { first: 5, last: 6 }]);
        // Single-line hits too, `$` at each line end.
        let (h, _) = hits(r"\d$", &lines);
        assert_eq!(h.iter().map(|h| h.first).collect::<Vec<_>>(), vec![0, 3]);
        // `.` does not cross a line end without `(?s)`.
        assert!(hits(r"select.deadlock", &lines).0.is_empty());
        assert_eq!(hits(r"(?s)select.deadlock", &lines).0.len(), 1);
    }

    #[test]
    fn chunk_boundaries_neither_lose_nor_double_hits() {
        // Long lines so the buffer is scanned many times; every 97th pair is a hit.
        let filler = "x".repeat(3000);
        let lines: Vec<String> = (0..2000)
            .map(|i| {
                if i % 97 == 0 {
                    "START".to_string()
                } else if i % 97 == 1 {
                    "END".to_string()
                } else {
                    format!("{i} {filler}")
                }
            })
            .collect();
        let (h, skipped) = hits(r"START\nEND", &lines);
        let expected: Vec<Hit> = (0..2000)
            .filter(|i| i % 97 == 0 && i + 1 < 2000)
            .map(|i| Hit {
                first: i,
                last: i + 1,
            })
            .collect();
        assert_eq!(h, expected);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn hits_longer_than_the_bound_are_counted_not_reported() {
        let mut lines: Vec<String> = vec!["A".into()];
        lines.extend((0..100).map(|i| format!("line {i}")));
        lines.push("B".into());
        lines.push("A".into());
        lines.push("B".into());
        let (h, skipped) = hits(r"(?s)A.*?B", &lines);
        assert_eq!(
            h,
            vec![Hit {
                first: 102,
                last: 103
            }]
        );
        assert_eq!(skipped, 1);
        // Stopping early.
        let re = build("line", false).unwrap();
        let mut s = Scanner::new(&re);
        let mut n = 0;
        let mut f = |_| {
            n += 1;
            n < 3
        };
        for (i, l) in lines.iter().enumerate() {
            if !s.push(i, l, &mut f) {
                break;
            }
        }
        s.finish(&mut f);
        assert_eq!(n, 3);
    }
}
