//! Timeline histogram: lines per level over a stream's time span, in a bounded number of
//! buckets, kept up to date line by line from the engine's timestamp and level caches.
//!
//! Buckets are numbered absolutely, `ts.div_euclid(bucket_ms)`, so a line before the first
//! bucket (an out-of-order log) just extends the deque to the left. The width starts at one
//! second and doubles whenever the span would need more than `MAX_BUCKETS`: bucket `b`
//! becomes `b.div_euclid(2)`, and because the numbering is absolute every boundary stays on
//! a whole second and pairs merge without the per-line data. It never shrinks back except
//! on `reset`.
//!
//! The deque never starts or ends with an empty bucket, so two histograms holding the same
//! lines at the same width compare equal.

use std::collections::VecDeque;

use crate::log_level::LogLevel;
use crate::tail_engine::NO_TIMESTAMP;

/// Most buckets a histogram holds; above that the width doubles.
pub const MAX_BUCKETS: usize = 2_048;
/// Width of a bucket before any doubling: one second, what the time fields can express.
pub const MIN_BUCKET_MS: i64 = 1_000;

/// Lines per level (`LogLevel as u8` index) in one bucket.
pub type LevelCounts = [u32; LogLevel::COUNT];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeHistogram {
    bucket_ms: i64,
    /// Absolute number of `counts[0]`.
    first_bucket: i64,
    counts: VecDeque<LevelCounts>,
    /// Lines with no effective timestamp (the banner before the first timed entry).
    untimed: u64,
    /// Lines placed in a bucket.
    timed: u64,
}

impl Default for TimeHistogram {
    fn default() -> Self {
        Self::with_bucket_ms(MIN_BUCKET_MS)
    }
}

/// A run of adjacent buckets drawn as one column of the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// Start of the first bucket and end of the last one (exclusive), in milliseconds.
    pub start_ms: i64,
    pub end_ms: i64,
    /// Relative index of the first bucket and the number of buckets covered.
    pub first: usize,
    pub buckets: usize,
    pub counts: [u64; LogLevel::COUNT],
}

impl Column {
    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }
}

fn level_slot(level: u8) -> usize {
    (level as usize).min(LogLevel::COUNT - 1)
}

impl TimeHistogram {
    /// An empty histogram whose buckets start `bucket_ms` wide (the tests rebuild one at
    /// the width an incremental histogram reached).
    pub fn with_bucket_ms(bucket_ms: i64) -> Self {
        Self {
            bucket_ms: bucket_ms.max(MIN_BUCKET_MS),
            first_bucket: 0,
            counts: VecDeque::new(),
            untimed: 0,
            timed: 0,
        }
    }

    /// Empties the histogram and goes back to one-second buckets.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn bucket_ms(&self) -> i64 {
        self.bucket_ms
    }

    /// Absolute number of the first bucket.
    pub fn first_bucket(&self) -> i64 {
        self.first_bucket
    }

    pub fn len(&self) -> usize {
        self.counts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn untimed(&self) -> u64 {
        self.untimed
    }

    pub fn timed(&self) -> u64 {
        self.timed
    }

    /// Counts of the bucket at relative index `i`.
    pub fn bucket(&self, i: usize) -> Option<&LevelCounts> {
        self.counts.get(i)
    }

    /// Start and end (exclusive) of the whole span, in milliseconds.
    pub fn span(&self) -> Option<(i64, i64)> {
        if self.counts.is_empty() {
            return None;
        }
        let start = self.first_bucket * self.bucket_ms;
        Some((start, start + self.counts.len() as i64 * self.bucket_ms))
    }

    /// Relative index of the bucket holding `ts`, when it falls inside the span.
    pub fn bucket_of(&self, ts: i64) -> Option<usize> {
        if ts == NO_TIMESTAMP || self.counts.is_empty() {
            return None;
        }
        let rel = ts.div_euclid(self.bucket_ms) - self.first_bucket;
        (rel >= 0 && (rel as usize) < self.counts.len()).then_some(rel as usize)
    }

    /// Counts one line with effective timestamp `ts` (`NO_TIMESTAMP`: untimed) and level
    /// `level` (`LogLevel as u8`).
    pub fn add(&mut self, ts: i64, level: u8) {
        if ts == NO_TIMESTAMP {
            self.untimed += 1;
            return;
        }
        let mut b = ts.div_euclid(self.bucket_ms);
        if self.counts.is_empty() {
            self.first_bucket = b;
            self.counts.push_back([0; LogLevel::COUNT]);
        } else {
            loop {
                let last = self.first_bucket + self.counts.len() as i64 - 1;
                let lo = self.first_bucket.min(b) as i128;
                let hi = last.max(b) as i128;
                if hi - lo < MAX_BUCKETS as i128 {
                    break;
                }
                self.double();
                b = ts.div_euclid(self.bucket_ms);
            }
            while b < self.first_bucket {
                self.counts.push_front([0; LogLevel::COUNT]);
                self.first_bucket -= 1;
            }
            while b >= self.first_bucket + self.counts.len() as i64 {
                self.counts.push_back([0; LogLevel::COUNT]);
            }
        }
        let rel = (b - self.first_bucket) as usize;
        self.counts[rel][level_slot(level)] += 1;
        self.timed += 1;
    }

    /// Takes back a line counted by `add` with the same arguments. The width stays; empty
    /// buckets at either end are dropped.
    pub fn remove(&mut self, ts: i64, level: u8) {
        if ts == NO_TIMESTAMP {
            self.untimed = self.untimed.saturating_sub(1);
            return;
        }
        let Some(rel) = self.bucket_of(ts) else {
            return;
        };
        let slot = &mut self.counts[rel][level_slot(level)];
        if *slot == 0 {
            return;
        }
        *slot -= 1;
        self.timed = self.timed.saturating_sub(1);
        while self
            .counts
            .front()
            .is_some_and(|c| c.iter().all(|&n| n == 0))
        {
            self.counts.pop_front();
            self.first_bucket += 1;
        }
        while self
            .counts
            .back()
            .is_some_and(|c| c.iter().all(|&n| n == 0))
        {
            self.counts.pop_back();
        }
    }

    /// Doubles the bucket width, merging buckets in pairs by their absolute numbers.
    fn double(&mut self) {
        self.bucket_ms = self.bucket_ms.saturating_mul(2);
        let new_first = self.first_bucket.div_euclid(2);
        let mut merged: VecDeque<LevelCounts> = VecDeque::with_capacity(self.counts.len() / 2 + 1);
        for (i, c) in self.counts.iter().enumerate() {
            let rel = ((self.first_bucket + i as i64).div_euclid(2) - new_first) as usize;
            if merged.len() <= rel {
                merged.push_back([0; LogLevel::COUNT]);
            }
            for (sum, n) in merged[rel].iter_mut().zip(c) {
                *sum += n;
            }
        }
        self.first_bucket = new_first;
        self.counts = merged;
    }

    /// Groups the buckets into at most `max_columns` columns of adjacent buckets: column
    /// `c` of `k` covers buckets `c * n / k .. (c + 1) * n / k`. With fewer buckets than
    /// columns every bucket is its own column (the painter spreads it over several pixels).
    pub fn columns(&self, max_columns: usize) -> Vec<Column> {
        let n = self.counts.len();
        let k = n.min(max_columns);
        let mut out = Vec::with_capacity(k);
        for c in 0..k {
            let first = c * n / k;
            let end = (c + 1) * n / k;
            let mut counts = [0u64; LogLevel::COUNT];
            for bucket in self.counts.range(first..end) {
                for (sum, &v) in counts.iter_mut().zip(bucket) {
                    *sum += u64::from(v);
                }
            }
            let start_ms = (self.first_bucket + first as i64) * self.bucket_ms;
            out.push(Column {
                start_ms,
                end_ms: start_ms + (end - first) as i64 * self.bucket_ms,
                first,
                buckets: end - first,
                counts,
            });
        }
        out
    }
}

/// Column holding relative bucket `bucket` when `buckets` buckets are grouped into
/// `columns` columns as `TimeHistogram::columns` does.
pub fn column_of_bucket(bucket: usize, buckets: usize, columns: usize) -> usize {
    let k = buckets.min(columns);
    if k == 0 {
        return 0;
    }
    // The last `c` with `c * n / k <= bucket`, that is `c < (bucket + 1) * k / n`.
    (((bucket + 1) * k).div_ceil(buckets) - 1).min(k - 1)
}

/// What a selection from column `first` to column `last` (either order) writes into the
/// time fields: the first second of the start bucket and the last second of the end one,
/// both as `format_millis` text. Through `end_of_typed_time` the "to" text covers that
/// whole second, so the window is exactly the selected buckets.
pub fn selection_texts(columns: &[Column], first: usize, last: usize) -> Option<(String, String)> {
    let (a, b) = (first.min(last), first.max(last));
    let start = columns.get(a)?.start_ms;
    let end = columns.get(b)?.end_ms;
    Some((
        crate::timestamp::format_millis(start),
        crate::timestamp::format_millis(end - MIN_BUCKET_MS),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: u8 = LogLevel::Info as u8;
    const ERROR: u8 = LogLevel::Error as u8;

    fn totals(h: &TimeHistogram) -> [u64; LogLevel::COUNT] {
        let mut out = [0u64; LogLevel::COUNT];
        for i in 0..h.len() {
            for (sum, &v) in out.iter_mut().zip(h.bucket(i).unwrap()) {
                *sum += u64::from(v);
            }
        }
        out
    }

    #[test]
    fn one_second_buckets_until_the_span_is_too_wide() {
        let mut h = TimeHistogram::default();
        h.add(10_500, INFO);
        h.add(10_999, ERROR);
        h.add(12_000, INFO);
        assert_eq!(h.bucket_ms(), 1_000);
        assert_eq!(h.first_bucket(), 10);
        assert_eq!(h.len(), 3);
        assert_eq!(h.bucket(0).unwrap()[INFO as usize], 1);
        assert_eq!(h.bucket(0).unwrap()[ERROR as usize], 1);
        assert_eq!(h.bucket(1).unwrap().iter().sum::<u32>(), 0);
        assert_eq!(h.span(), Some((10_000, 13_000)));
    }

    #[test]
    fn doubling_merges_pairs_and_preserves_totals() {
        let mut h = TimeHistogram::default();
        for s in 0..MAX_BUCKETS as i64 {
            h.add(s * 1_000, INFO);
        }
        assert_eq!(h.bucket_ms(), 1_000);
        assert_eq!(h.len(), MAX_BUCKETS);
        // One more second: the width doubles, pairs merge.
        h.add(MAX_BUCKETS as i64 * 1_000, ERROR);
        assert_eq!(h.bucket_ms(), 2_000);
        assert_eq!(h.len(), MAX_BUCKETS / 2 + 1);
        assert_eq!(h.bucket(0).unwrap()[INFO as usize], 2);
        assert_eq!(h.timed(), MAX_BUCKETS as u64 + 1);
        let t = totals(&h);
        assert_eq!(t[INFO as usize], MAX_BUCKETS as u64);
        assert_eq!(t[ERROR as usize], 1);
        // A day at one line per second fits in 2,048 buckets of 64 s.
        let mut day = TimeHistogram::default();
        for s in 0..86_400i64 {
            day.add(s * 1_000, INFO);
        }
        assert_eq!(day.bucket_ms(), 64_000);
        assert!(day.len() <= MAX_BUCKETS);
        assert_eq!(totals(&day)[INFO as usize], 86_400);
        assert_eq!(
            day.bucket_ms() % MIN_BUCKET_MS,
            0,
            "boundaries on whole seconds"
        );
    }

    #[test]
    fn out_of_order_line_extends_to_the_left() {
        let mut h = TimeHistogram::default();
        h.add(100_000, INFO);
        h.add(95_000, ERROR);
        assert_eq!(h.first_bucket(), 95);
        assert_eq!(h.len(), 6);
        assert_eq!(h.bucket(0).unwrap()[ERROR as usize], 1);
        // Far to the left: doubles until it fits, with negative bucket numbers too.
        h.add(-3_000_000, INFO);
        assert!(h.len() <= MAX_BUCKETS);
        assert_eq!(totals(&h).iter().sum::<u64>(), 3);
        assert_eq!(h.bucket_of(-3_000_000), Some(0));
    }

    #[test]
    fn remove_takes_back_an_add() {
        let mut h = TimeHistogram::default();
        h.add(NO_TIMESTAMP, INFO);
        h.add(1_000, INFO);
        h.add(5_000, ERROR);
        let before = h.clone();
        h.add(9_000, INFO);
        h.remove(9_000, INFO);
        assert_eq!(h, before, "empty bucket at the end is trimmed");
        h.remove(1_000, INFO);
        assert_eq!(h.first_bucket(), 5);
        assert_eq!(h.len(), 1);
        h.remove(NO_TIMESTAMP, INFO);
        h.remove(5_000, ERROR);
        assert!(h.is_empty());
        assert_eq!(h.untimed(), 0);
        assert_eq!(h.timed(), 0);
        // Removing what is not there changes nothing.
        h.remove(5_000, ERROR);
        assert!(h.is_empty());
    }

    #[test]
    fn columns_group_buckets_and_spread_few_buckets() {
        let mut h = TimeHistogram::default();
        for s in 0..100i64 {
            h.add(s * 1_000, if s % 10 == 0 { ERROR } else { INFO });
        }
        // Fewer columns than buckets: each sums the buckets it covers.
        let cols = h.columns(10);
        assert_eq!(cols.len(), 10);
        assert!(cols.iter().all(|c| c.buckets == 10));
        assert!(cols.iter().all(|c| c.counts[ERROR as usize] == 1));
        assert_eq!(cols.iter().map(Column::total).sum::<u64>(), 100);
        assert_eq!(cols[3].start_ms, 30_000);
        assert_eq!(cols[3].end_ms, 40_000);
        // Uneven split still covers every bucket once.
        let cols = h.columns(7);
        assert_eq!(cols.iter().map(|c| c.buckets).sum::<usize>(), 100);
        assert_eq!(cols.iter().map(Column::total).sum::<u64>(), 100);
        for b in 0..100 {
            let c = column_of_bucket(b, 100, 7);
            assert!(cols[c].first <= b && b < cols[c].first + cols[c].buckets);
        }
        // More columns than buckets: one column per bucket.
        let cols = h.columns(500);
        assert_eq!(cols.len(), 100);
        assert_eq!(column_of_bucket(42, 100, 500), 42);
        assert!(TimeHistogram::default().columns(100).is_empty());
    }

    #[test]
    fn selection_writes_whole_second_bounds() {
        let mut h = TimeHistogram::default();
        // 14:02:00 to 14:04:59 on 2024-03-05, one line per second.
        let base = 1_709_647_320_000i64; // 2024-03-05 14:02:00 UTC
        for s in 0..180i64 {
            h.add(base + s * 1_000 + 250, INFO);
        }
        let cols = h.columns(1_000);
        let (from, to) = selection_texts(&cols, cols.len() - 1, 0).unwrap();
        assert_eq!(from, "2024-03-05 14:02:00");
        assert_eq!(to, "2024-03-05 14:04:59");
        let to_ms = crate::timestamp::parse_user_time(&to, base).unwrap();
        assert_eq!(
            crate::timestamp::end_of_typed_time(&to, to_ms),
            base + 180_000 - 1,
            "the to bound covers the last selected second"
        );
        // A click selects one column.
        let (from, to) = selection_texts(&cols, 5, 5).unwrap();
        assert_eq!(from, "2024-03-05 14:02:05");
        assert_eq!(to, "2024-03-05 14:02:05");
        assert!(selection_texts(&cols, 0, 999).is_none());
    }
}
