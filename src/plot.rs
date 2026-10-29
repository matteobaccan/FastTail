// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Numeric fields plotted over the timeline histogram: per time bucket the count, sum,
//! minimum, maximum and a log-scale histogram (p95 within 1 %) of a field's values over
//! the lines the stream shows.
//!
//! Buckets work like the histogram's (`time_histogram`): one second wide at first, at
//! most `MAX_BUCKETS`, doubled (merged in pairs) when the span outgrows them. A value
//! with a unit is read in its family's base unit (`units`: durations in milliseconds,
//! sizes in bytes, rates per second); when a field mixes families, or a family with
//! plain numbers, the series says so and the values are plotted as written.

use std::collections::VecDeque;

use crate::field_stats::NumHist;
use crate::tail_engine::NO_TIMESTAMP;
use crate::units::{Family, Scale, Unit};

/// Fields plotted at once at most.
pub const MAX_PLOTS: usize = 3;
/// Buckets of a series at most.
pub const MAX_BUCKETS: usize = 2_048;
/// Width of a bucket before any doubling.
pub const MIN_BUCKET_MS: i64 = 1_000;

/// What a bucket's values become on the plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlotStat {
    #[default]
    Avg,
    Max,
    Min,
    P95,
}

impl PlotStat {
    pub const ALL: [PlotStat; 4] = [PlotStat::Avg, PlotStat::Max, PlotStat::Min, PlotStat::P95];

    pub fn name(self) -> &'static str {
        match self {
            PlotStat::Avg => "avg",
            PlotStat::Max => "max",
            PlotStat::Min => "min",
            PlotStat::P95 => "p95",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == s.trim())
    }

    /// The next one of `ALL`, back to the first after the last.
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// A plotted field: its key and what is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotSpec {
    pub key: String,
    pub stat: PlotStat,
}

impl PlotSpec {
    /// `latency_ms:avg` (the stat may be left out: avg).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (key, stat) = match s.rsplit_once(':') {
            Some((k, st)) => (k, PlotStat::from_name(st)?),
            None => (s, PlotStat::Avg),
        };
        let key = key.trim();
        (!key.is_empty()).then(|| PlotSpec {
            key: key.to_string(),
            stat,
        })
    }

    pub fn to_text(&self) -> String {
        format!("{}:{}", self.key, self.stat.name())
    }

    /// The `plot=` value of a stream: its specs separated by commas.
    pub fn list_text(specs: &[PlotSpec]) -> String {
        specs
            .iter()
            .map(PlotSpec::to_text)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The specs of a `plot=` value (at most `MAX_PLOTS`, unreadable ones skipped).
    pub fn parse_list(s: &str) -> Vec<PlotSpec> {
        let mut out: Vec<PlotSpec> = Vec::new();
        for spec in s.split(',').filter_map(PlotSpec::parse) {
            if out.len() < MAX_PLOTS && !out.iter().any(|o| o.key == spec.key) {
                out.push(spec);
            }
        }
        out
    }
}

/// The bit of a unit family in `Series::families`.
fn bit(family: Family) -> u8 {
    1 << family.index()
}

/// The values of one bucket.
#[derive(Debug, Clone, Default)]
pub struct Agg {
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub hist: NumHist,
}

impl Agg {
    fn add(&mut self, v: f64) {
        if self.count == 0 {
            (self.min, self.max) = (v, v);
        } else {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        self.count += 1;
        self.sum += v;
        self.hist.add(v);
    }

    fn merge(&mut self, o: &Agg) {
        if o.count == 0 {
            return;
        }
        if self.count == 0 {
            (self.min, self.max) = (o.min, o.max);
        } else {
            self.min = self.min.min(o.min);
            self.max = self.max.max(o.max);
        }
        self.count += o.count;
        self.sum += o.sum;
        self.hist.merge(&o.hist);
    }

    /// The bucket's value for `stat`.
    pub fn value(&self, stat: PlotStat) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        Some(match stat {
            PlotStat::Avg => self.sum / self.count as f64,
            PlotStat::Max => self.max,
            PlotStat::Min => self.min,
            PlotStat::P95 => self.hist.percentile(0.95)?,
        })
    }
}

/// The series of one field: buckets of values in normalised units, and the same in the
/// units as written (used when the families are mixed).
#[derive(Debug, Clone)]
pub struct Series {
    pub spec: PlotSpec,
    bucket_ms: i64,
    first_bucket: i64,
    buckets: VecDeque<(Agg, Agg)>,
    /// Lines with the field but no number in it.
    pub skipped: u64,
    families: u8,
    /// Values read with a binary size prefix, of `values`.
    binary: u64,
    values: u64,
    /// What a bare number means (a log format's `unit.<field>`).
    pub bare: Option<Unit>,
}

impl Series {
    pub fn new(spec: PlotSpec) -> Self {
        Self {
            spec,
            bucket_ms: MIN_BUCKET_MS,
            first_bucket: 0,
            buckets: VecDeque::new(),
            skipped: 0,
            families: 0,
            binary: 0,
            values: 0,
            bare: None,
        }
    }

    /// The same, a bare number meaning `bare`.
    pub fn with_bare(mut self, bare: Option<Unit>) -> Self {
        self.bare = bare;
        self
    }

    /// Whether the values mix unit families (plotted as written).
    pub fn mixed(&self) -> bool {
        self.families.count_ones() > 1
    }

    /// The scale the values are drawn in: their family when they share one, plain
    /// (as written) otherwise.
    pub fn unit(&self) -> Scale {
        match Family::ALL.iter().find(|f| bit(**f) == self.families) {
            Some(&family) => Scale {
                family,
                binary: matches!(family, Family::Size | Family::ByteRate)
                    && self.binary * 2 > self.values,
            },
            None => Scale::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// The instants its buckets cover, `[start_ms, end_ms)`; none while it is empty.
    pub fn span(&self) -> Option<(i64, i64)> {
        let end = self.first_bucket.saturating_add(self.buckets.len() as i64);
        (!self.buckets.is_empty()).then(|| {
            (
                self.first_bucket.saturating_mul(self.bucket_ms),
                end.saturating_mul(self.bucket_ms),
            )
        })
    }

    /// Adds the value text of a line timed `ts` (an untimed line, or no number, is
    /// skipped).
    pub fn add(&mut self, ts: i64, text: &str) {
        let Some(read) = crate::units::read_as(text, self.bare) else {
            self.skipped += 1;
            return;
        };
        if ts == NO_TIMESTAMP {
            return;
        }
        let (v, raw) = (read.value, read.written);
        self.families |= bit(read.family);
        self.values += 1;
        if read.binary {
            self.binary += 1;
        }
        let mut b = ts.div_euclid(self.bucket_ms);
        if self.buckets.is_empty() {
            self.first_bucket = b;
            self.buckets.push_back(Default::default());
        } else {
            loop {
                let last = self.first_bucket + self.buckets.len() as i64 - 1;
                let lo = self.first_bucket.min(b) as i128;
                let hi = last.max(b) as i128;
                if hi - lo < MAX_BUCKETS as i128 {
                    break;
                }
                self.double();
                b = ts.div_euclid(self.bucket_ms);
            }
            while b < self.first_bucket {
                self.buckets.push_front(Default::default());
                self.first_bucket -= 1;
            }
            while b >= self.first_bucket + self.buckets.len() as i64 {
                self.buckets.push_back(Default::default());
            }
        }
        let rel = (b - self.first_bucket) as usize;
        let (norm, written) = &mut self.buckets[rel];
        norm.add(v);
        written.add(raw);
    }

    /// Doubles the bucket width, merging buckets in pairs by their absolute numbers.
    fn double(&mut self) {
        self.bucket_ms = self.bucket_ms.saturating_mul(2);
        let new_first = self.first_bucket.div_euclid(2);
        let mut merged: VecDeque<(Agg, Agg)> = VecDeque::with_capacity(self.buckets.len() / 2 + 1);
        for (i, (a, w)) in self.buckets.iter().enumerate() {
            let rel = ((self.first_bucket + i as i64).div_euclid(2) - new_first) as usize;
            if merged.len() <= rel {
                merged.push_back(Default::default());
            }
            merged[rel].0.merge(a);
            merged[rel].1.merge(w);
        }
        self.first_bucket = new_first;
        self.buckets = merged;
    }

    /// The values of the instants `[start_ms, end_ms)` merged, for a column of the strip.
    pub fn agg_between(&self, start_ms: i64, end_ms: i64) -> Agg {
        let mut out = Agg::default();
        if self.buckets.is_empty() || end_ms <= start_ms {
            return out;
        }
        let first = start_ms.div_euclid(self.bucket_ms) - self.first_bucket;
        let last = (end_ms - 1).div_euclid(self.bucket_ms) - self.first_bucket;
        let n = self.buckets.len() as i64;
        let mixed = self.mixed();
        for rel in first.max(0)..=last.min(n - 1) {
            let (norm, written) = &self.buckets[rel as usize];
            out.merge(if mixed { written } else { norm });
        }
        out
    }

    /// The value drawn for `[start_ms, end_ms)`.
    pub fn value_between(&self, start_ms: i64, end_ms: i64) -> Option<f64> {
        self.agg_between(start_ms, end_ms).value(self.spec.stat)
    }
}

/// The plotted fields of a stream and their series: counted by a background scan over
/// the visible lines, then the appended ones as they come.
#[derive(Default)]
pub struct PlotState {
    pub series: Vec<Series>,
    pub(crate) job: Option<crate::scan_job::ScanJob>,
    pub(crate) job_generation: u64,
    pub progress: f32,
    /// The view or the fields changed: count again on the next poll.
    pub(crate) restart: bool,
    /// The lines counted: `0..covered`.
    pub(crate) covered: usize,
    /// The timestamp an appended line without its own inherits.
    pub(crate) last_ts: i64,
    /// Bumped whenever a series changes, for views that cache what they draw.
    pub generation: u64,
}

impl PlotState {
    pub fn running(&self) -> bool {
        self.job.is_some() || self.restart
    }
}

/// `v` for a scale or a tooltip, in `unit` (`1.2 s`, `3.4 MB`, or the number).
pub fn format_value(v: f64, unit: Scale) -> String {
    crate::units::format(v, unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse_and_round_trip() {
        assert_eq!(
            PlotSpec::parse("latency_ms:p95"),
            Some(PlotSpec {
                key: "latency_ms".into(),
                stat: PlotStat::P95
            })
        );
        assert_eq!(PlotSpec::parse("took").unwrap().stat, PlotStat::Avg);
        assert!(PlotSpec::parse("took:median").is_none());
        let list = PlotSpec::parse_list("a:max, b ,a:min,c:p95,d");
        assert_eq!(PlotSpec::list_text(&list), "a:max,b:avg,c:p95");
    }

    #[test]
    fn units_are_normalised_within_a_family() {
        let mut s = Series::new(PlotSpec::parse("took:max").unwrap());
        s.add(0, "250ms");
        s.add(0, "1.5 s");
        assert_eq!(s.unit().family, Family::Time);
        assert_eq!(
            format_value(s.value_between(0, 1000).unwrap(), s.unit()),
            "1.5 s"
        );
        let mut b = Series::new(PlotSpec::parse("size:max").unwrap());
        b.add(0, "2KiB");
        assert_eq!(
            format_value(b.value_between(0, 1000).unwrap(), b.unit()),
            "2 KiB"
        );
        let mut bare = Series::new(PlotSpec::parse("took:max").unwrap())
            .with_bare(crate::units::unit_named("s"));
        bare.add(0, "2");
        assert_eq!(
            format_value(bare.value_between(0, 1000).unwrap(), bare.unit()),
            "2 s"
        );
    }

    #[test]
    fn buckets_aggregate_double_and_answer_columns() {
        let spec = PlotSpec::parse("took:avg").unwrap();
        let mut s = Series::new(spec);
        for i in 0..10 {
            s.add(i * 1000, &format!("{}ms", i * 10));
        }
        s.add(5_500, "x");
        assert_eq!(s.skipped, 1);
        assert_eq!(s.unit().family, Family::Time);
        // Seconds 2..4: 20, 30, 40.
        let a = s.agg_between(2_000, 5_000);
        assert_eq!((a.count, a.min, a.max), (3, 20.0, 40.0));
        assert_eq!(s.value_between(2_000, 5_000), Some(30.0));
        // A value far later doubles the width until it fits.
        s.add(10_000_000, "1s");
        assert!(s.buckets.len() <= MAX_BUCKETS);
        assert!(s.bucket_ms > MIN_BUCKET_MS);
        assert_eq!(s.agg_between(0, 10_001_000).count, 11);
        assert_eq!(s.agg_between(0, 10_001_000).max, 1000.0);
        // p95 of 0..90 by 10 within 1 %.
        let mut p = Series::new(PlotSpec::parse("v:p95").unwrap());
        for i in 1..=100 {
            p.add(0, &i.to_string());
        }
        let v = p.value_between(0, 1000).unwrap();
        assert!((v - 95.0).abs() <= 1.5, "{v}");
    }

    #[test]
    fn mixed_families_are_plotted_as_written() {
        let mut s = Series::new(PlotSpec::parse("v:max").unwrap());
        s.add(0, "2s");
        s.add(0, "5KB");
        assert!(s.mixed());
        assert_eq!(s.unit(), Scale::default());
        assert_eq!(s.value_between(0, 1000), Some(5.0));
    }
}
