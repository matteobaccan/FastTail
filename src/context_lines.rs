//! Context lines around filter matches (`grep -C N` in the viewer).
//!
//! The matches stay in the engine's `filtered_lines`; the lines shown around them are
//! never filtered and never materialised. `ContextRanges` derives, in one pass over the
//! matches, the merged `[first, last]` line ranges `match - N ..= match + N` together
//! with the prefix sum of their rows, so the row mapping is a binary search over the
//! groups (O(log groups) per row) and the memory is 24 bytes per group.
//!
//! The last range is not clamped to the end of the file: the file length is applied when
//! reading, so lines appended within `N` lines after the last match become visible as
//! context without any recomputation.

/// Largest context a stream can ask for.
pub const MAX_CONTEXT_LINES: u8 = 100;

/// Above this many matches a change of `N` rebuilds the ranges on a worker thread.
pub const BACKGROUND_REBUILD_MATCHES: usize = 10_000_000;

/// One group of consecutive shown lines: `first..=last` (the last range may run past the
/// end of the file) and `row`, the visible position of `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub first: usize,
    pub last: usize,
    pub row: usize,
}

impl Range {
    fn rows(&self) -> usize {
        self.last - self.first + 1
    }
}

/// The merged ranges of the matches with `n` lines of context each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextRanges {
    n: usize,
    ranges: Vec<Range>,
    /// Rows of all the ranges, the last one unclamped.
    rows: usize,
}

impl ContextRanges {
    /// No ranges yet, `n` lines of context for the matches to come.
    pub fn new(n: usize) -> Self {
        Self {
            n,
            ranges: Vec::new(),
            rows: 0,
        }
    }

    /// The ranges of `matches` (sorted line indices) with `n` lines of context each.
    pub fn build(n: usize, matches: &[usize]) -> Self {
        let mut out = Self::new(n);
        out.extend(matches);
        out
    }

    /// `build` on a worker thread: gives up (`None`) as soon as `cancel` is set, checked
    /// every 64 Ki matches, so a build a newer `N` superseded stops early.
    pub fn build_cancellable(
        n: usize,
        matches: &[usize],
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Option<Self> {
        let mut out = Self::new(n);
        for chunk in matches.chunks(1 << 16) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            out.extend(chunk);
        }
        Some(out)
    }

    /// Lines of context per side; 0 means context lines are off.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Whether context lines are on (`n > 0`).
    pub fn is_active(&self) -> bool {
        self.n > 0
    }

    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    /// Adds matches that come after every match seen so far (a filter batch, appended
    /// lines): the first of them may merge into the last range.
    pub fn extend(&mut self, new_matches: &[usize]) {
        if self.n == 0 {
            return;
        }
        let n = self.n;
        for &m in new_matches {
            let first = m.saturating_sub(n);
            let last = m.saturating_add(n);
            match self.ranges.last_mut() {
                Some(prev) if first <= prev.last.saturating_add(1) => {
                    if last > prev.last {
                        self.rows += last - prev.last;
                        prev.last = last;
                    }
                }
                _ => {
                    let range = Range {
                        first,
                        last,
                        row: self.rows,
                    };
                    self.rows += range.rows();
                    self.ranges.push(range);
                }
            }
        }
    }

    /// Forgets every range, keeping `n`.
    pub fn clear(&mut self) {
        self.ranges = Vec::new();
        self.rows = 0;
    }

    /// The matches after `last_match` were dropped (`None`: all of them): the ranges
    /// they alone made go, and the range of `last_match` ends `n` lines after it.
    pub fn truncate_after(&mut self, last_match: Option<usize>) {
        let Some(m) = last_match else {
            self.clear();
            return;
        };
        while self.ranges.last().is_some_and(|r| r.first > m) {
            self.ranges.pop();
        }
        match self.ranges.last_mut() {
            Some(r) => {
                r.last = m.saturating_add(self.n);
                self.rows = r.row + r.rows();
            }
            None => self.rows = 0,
        }
    }

    /// Rows shown for a file of `total` lines: the last range stops at the last line.
    pub fn visible_count(&self, total: usize) -> usize {
        let Some(r) = self.ranges.last() else {
            return 0;
        };
        if r.first >= total {
            return r.row;
        }
        r.row + r.last.min(total - 1) - r.first + 1
    }

    /// The line at visible position `pos`.
    pub fn line_at(&self, pos: usize, total: usize) -> Option<usize> {
        if pos >= self.visible_count(total) {
            return None;
        }
        let i = self
            .ranges
            .partition_point(|r| r.row <= pos)
            .checked_sub(1)?;
        let r = &self.ranges[i];
        Some(r.first + (pos - r.row))
    }

    /// Index of the range holding `line`, if any.
    fn range_of(&self, line: usize) -> Option<usize> {
        let i = self
            .ranges
            .partition_point(|r| r.first <= line)
            .checked_sub(1)?;
        (line <= self.ranges[i].last).then_some(i)
    }

    /// Visible position of `line`, `None` when it is neither a match nor context.
    pub fn pos_of_line(&self, line: usize, total: usize) -> Option<usize> {
        if line >= total {
            return None;
        }
        let r = &self.ranges[self.range_of(line)?];
        Some(r.row + (line - r.first))
    }

    /// Visible position of `line`, or of the first shown line after it (the row count
    /// past the last one).
    pub fn pos_of_line_or_next(&self, line: usize, total: usize) -> usize {
        let count = self.visible_count(total);
        if let Some(pos) = self.pos_of_line(line, total) {
            return pos;
        }
        let i = self.ranges.partition_point(|r| r.first <= line);
        self.ranges.get(i).map_or(count, |r| r.row.min(count))
    }

    /// Whether `line` is shown (a match or a context line).
    pub fn contains(&self, line: usize, total: usize) -> bool {
        line < total && self.range_of(line).is_some()
    }

    /// Lines hidden before range `i`: between it and the previous range, or before the
    /// first one.
    pub fn gap_before(&self, i: usize) -> usize {
        match (
            i.checked_sub(1).and_then(|p| self.ranges.get(p)),
            self.ranges.get(i),
        ) {
            (Some(prev), Some(r)) => r.first - prev.last - 1,
            (None, Some(r)) => r.first,
            _ => 0,
        }
    }
}

/// Walks the visible positions of increasing lines over sorted ranges (a worker's scan):
/// O(1) amortised per line.
#[derive(Debug, Clone)]
pub struct RangeCursor<'a> {
    ranges: &'a [Range],
    at: usize,
}

impl<'a> RangeCursor<'a> {
    /// A cursor for lines from `start_line` on.
    pub fn new(ranges: &'a [Range], start_line: usize) -> Self {
        Self {
            ranges,
            at: ranges.partition_point(|r| r.last < start_line),
        }
    }

    /// Visible position of `line` (lines must come in increasing order), `None` when it
    /// lies outside every range.
    pub fn pos(&mut self, line: usize) -> Option<usize> {
        while self.ranges.get(self.at).is_some_and(|r| r.last < line) {
            self.at += 1;
        }
        let r = self.ranges.get(self.at)?;
        (r.first <= line).then(|| r.row + (line - r.first))
    }

    /// Whether every range lies before the lines still to come.
    pub fn done(&self) -> bool {
        self.at >= self.ranges.len()
    }
}

/// The lines a filtered view shows: every line (no filter), the matches, or the matches
/// with their context. One mapping for the engine's rows and for the collapse groups.
#[derive(Debug, Clone, Copy)]
pub struct VisibleView<'a> {
    pub filtered: bool,
    pub matches: &'a [usize],
    pub context: &'a ContextRanges,
    pub total: usize,
}

impl VisibleView<'_> {
    fn with_context(&self) -> bool {
        self.filtered && self.context.is_active()
    }

    /// Lines shown.
    pub fn len(&self) -> usize {
        if !self.filtered {
            self.total
        } else if self.with_context() {
            self.context.visible_count(self.total)
        } else {
            self.matches.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The line at visible position `pos`.
    pub fn line_at(&self, pos: usize) -> Option<usize> {
        if !self.filtered {
            (pos < self.total).then_some(pos)
        } else if self.with_context() {
            self.context.line_at(pos, self.total)
        } else {
            self.matches.get(pos).copied()
        }
    }

    /// Visible position of `line`, `None` when the view hides it.
    pub fn pos_of_line(&self, line: usize) -> Option<usize> {
        if !self.filtered {
            (line < self.total).then_some(line)
        } else if self.with_context() {
            self.context.pos_of_line(line, self.total)
        } else {
            self.matches.binary_search(&line).ok()
        }
    }

    /// Visible position of `line`, or of the first shown line after it.
    pub fn pos_of_line_or_next(&self, line: usize) -> usize {
        if !self.filtered {
            line.min(self.total)
        } else if self.with_context() {
            self.context.pos_of_line_or_next(line, self.total)
        } else {
            self.matches.partition_point(|&l| l < line)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `grep -C n` shows: every line within `n` of a match, once, in order.
    fn grep_c(n: usize, matches: &[usize], total: usize) -> Vec<usize> {
        (0..total)
            .filter(|&l| matches.iter().any(|&m| l + n >= m && l <= m + n))
            .collect()
    }

    fn shown(c: &ContextRanges, total: usize) -> Vec<usize> {
        (0..c.visible_count(total))
            .map(|p| c.line_at(p, total).unwrap())
            .collect()
    }

    #[test]
    fn touching_and_overlapping_ranges_merge() {
        // 10 and 17 touch at N = 3 (13 + 1 = 14 = 17 - 3); 30 stands alone.
        let c = ContextRanges::build(3, &[10, 17, 30]);
        assert_eq!(
            c.ranges(),
            &[
                Range {
                    first: 7,
                    last: 20,
                    row: 0
                },
                Range {
                    first: 27,
                    last: 33,
                    row: 14
                }
            ]
        );
        // Overlapping: 100 and 104 at N = 3 give 97..=107 once.
        let c = ContextRanges::build(3, &[100, 104]);
        assert_eq!(c.ranges().len(), 1);
        assert_eq!(shown(&c, 1000), (97..=107).collect::<Vec<_>>());
        assert_eq!(c.gap_before(0), 97);
    }

    #[test]
    fn first_range_clamps_at_line_zero() {
        let c = ContextRanges::build(5, &[2]);
        assert_eq!(c.ranges()[0].first, 0);
        assert_eq!(shown(&c, 100), (0..=7).collect::<Vec<_>>());
    }

    #[test]
    fn last_range_is_clipped_to_the_file_end_and_grows_with_it() {
        let c = ContextRanges::build(3, &[8]);
        assert_eq!(c.ranges()[0].last, 11, "not clamped");
        assert_eq!(shown(&c, 10), vec![5, 6, 7, 8, 9]);
        assert_eq!(c.visible_count(10), 5);
        // Two lines appended: they show as context without touching the ranges.
        assert_eq!(shown(&c, 12), vec![5, 6, 7, 8, 9, 10, 11]);
        assert_eq!(c.pos_of_line(11, 12), Some(6));
        assert_eq!(c.pos_of_line(11, 10), None);
    }

    #[test]
    fn mapping_matches_grep_c_both_ways() {
        let total = 200;
        let matches = [0, 3, 50, 51, 60, 120, 199];
        for n in 0..6 {
            let c = ContextRanges::build(n, &matches);
            if n == 0 {
                assert!(!c.is_active());
                continue;
            }
            let expected = grep_c(n, &matches, total);
            assert_eq!(shown(&c, total), expected, "n = {n}");
            for line in 0..total {
                let pos = expected.iter().position(|&l| l == line);
                assert_eq!(c.pos_of_line(line, total), pos, "n = {n}, line {line}");
                assert_eq!(c.contains(line, total), pos.is_some());
                let next = expected.partition_point(|&l| l < line);
                assert_eq!(c.pos_of_line_or_next(line, total), next, "line {line}");
            }
        }
    }

    #[test]
    fn incremental_extend_equals_a_full_build() {
        let matches: Vec<usize> = (0..5000).map(|i| i * 7 + (i % 5) * 3).collect();
        for n in [1, 2, 3, 10] {
            let full = ContextRanges::build(n, &matches);
            let mut inc = ContextRanges::new(n);
            for chunk in matches.chunks(97) {
                inc.extend(chunk);
            }
            assert_eq!(inc, full, "n = {n}");
        }
    }

    #[test]
    fn a_cancelled_build_gives_up() {
        use std::sync::atomic::AtomicBool;
        let matches: Vec<usize> = (0..200_000).map(|i| i * 9).collect();
        let go = AtomicBool::new(false);
        assert_eq!(
            ContextRanges::build_cancellable(3, &matches, &go),
            Some(ContextRanges::build(3, &matches))
        );
        let stop = AtomicBool::new(true);
        assert_eq!(ContextRanges::build_cancellable(3, &matches, &stop), None);
    }

    #[test]
    fn truncation_keeps_what_the_kept_matches_make() {
        let matches = [10, 14, 40, 44, 90];
        for keep in 0..=matches.len() {
            let mut c = ContextRanges::build(3, &matches);
            c.truncate_after(keep.checked_sub(1).map(|i| matches[i]));
            assert_eq!(c, ContextRanges::build(3, &matches[..keep]), "keep {keep}");
        }
    }

    #[test]
    fn gaps_count_the_hidden_lines() {
        // The spec's example, 0-based: matches on lines 999 and 4,999 with N = 3.
        let c = ContextRanges::build(3, &[999, 4999]);
        assert_eq!(c.gap_before(1), 3993);
        assert_eq!(c.visible_count(10_000), 14);
    }

    #[test]
    fn the_cursor_walks_positions_in_order() {
        let c = ContextRanges::build(2, &[5, 20, 22]);
        let mut cursor = RangeCursor::new(c.ranges(), 0);
        let got: Vec<Option<usize>> = (0..30).map(|l| cursor.pos(l)).collect();
        let total = 30;
        let expected: Vec<Option<usize>> = (0..30).map(|l| c.pos_of_line(l, total)).collect();
        assert_eq!(got, expected);
        assert!(cursor.done());
        let mut from = RangeCursor::new(c.ranges(), 19);
        assert_eq!(from.pos(19), c.pos_of_line(19, total));
    }

    #[test]
    fn ten_million_matches_build_quickly() {
        let matches: Vec<usize> = (0..10_000_000).map(|i| i * 3).collect();
        let start = std::time::Instant::now();
        let c = ContextRanges::build(1, &matches);
        let took = start.elapsed();
        assert_eq!(c.ranges().len(), 1);
        // The design's bound is for an optimised build; a debug build is several times
        // slower, so it only has to stay well under a second there.
        let bound = if cfg!(debug_assertions) {
            std::time::Duration::from_millis(2000)
        } else {
            std::time::Duration::from_millis(100)
        };
        assert!(took < bound, "{took:?}");
        let sparse: Vec<usize> = (0..1_000_000).map(|i| i * 100).collect();
        let c = ContextRanges::build(10, &sparse);
        assert_eq!(c.ranges().len(), 1_000_000);
        // 21 lines per match, less the 10 the first one loses before line 0.
        assert_eq!(c.visible_count(usize::MAX), 21_000_000 - 10);
    }
}
