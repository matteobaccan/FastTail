// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! "Open filter as new tab": a derived stream holding the lines that pass a stream's
//! filter as it was when the tab was made (the filter is frozen), following the source.
//!
//! The derived stream is an ordinary `TailEngine` over a spool file, so search, its own
//! filters, collapse, bookmarks, copy and export work unchanged. The spool is fed from
//! the source stream itself (`DerivedFeeder::step`, a few milliseconds per frame): its
//! lines are read through the source engine, which already decodes them, handles ANSI,
//! follows appends and notices truncation and rotation. A map keeps the source line of
//! every derived line, for the gutter, go to line and "Show in context".

use crate::log_level::LogLevel;
use crate::scan_job::FilterSpec;
use crate::spool::SpoolFile;
use crate::tail_engine::{time_window_contains, TailEngine};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The filter of the source when the tab was made. The global filter is not part of it:
/// it applies live to the derived stream as to every stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub case_sensitive: bool,
    pub is_regex: bool,
    pub min_level: LogLevel,
    pub show_unknown_levels: bool,
    /// The time window in force (cache milliseconds), either side open.
    pub time_from: Option<i64>,
    pub time_to: Option<i64>,
}

impl FrozenFilter {
    /// The filter `engine` applies now (its own terms, level and time window).
    pub fn of(engine: &TailEngine) -> Self {
        let (time_from, mut time_to) = engine.time_window().unwrap_or((None, None));
        // A relative "to" (`now`, `-5m`) would freeze the window at this instant and stop
        // the tab from following: it stays open. A relative "from" is frozen where it is.
        if crate::tail_engine::is_relative_time(&engine.time_to_text) {
            time_to = None;
        }
        Self {
            include: nonempty(engine.include_terms()),
            exclude: nonempty(engine.exclude_terms()),
            case_sensitive: engine.filter_case_sensitive,
            is_regex: engine.filter_is_regex,
            min_level: engine.min_level,
            show_unknown_levels: engine.show_unknown_levels,
            time_from,
            time_to,
        }
    }

    /// Whether it filters anything at all.
    pub fn is_active(&self) -> bool {
        !self.include.is_empty()
            || !self.exclude.is_empty()
            || self.min_level != LogLevel::Unknown
            || self.time_from.is_some()
            || self.time_to.is_some()
    }

    fn has_window(&self) -> bool {
        self.time_from.is_some() || self.time_to.is_some()
    }

    fn spec(&self) -> FilterSpec {
        FilterSpec::build(
            &self.include,
            &self.exclude,
            self.case_sensitive,
            self.is_regex,
        )
        .with_levels(self.min_level, self.show_unknown_levels)
    }

    /// The short label of the tab title: the first include term, else the exclusion,
    /// the level or the time window.
    pub fn label(&self) -> String {
        if let Some(term) = self.include.first() {
            return term.clone();
        }
        if let Some(term) = self.exclude.first() {
            return format!("¬{term}");
        }
        if self.min_level != LogLevel::Unknown {
            return format!("≥{}", self.min_level.name());
        }
        "🕘".to_string()
    }

    /// Every part of the filter, one per line, for the tab tooltip.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        for term in &self.include {
            parts.push(format!("+ {term}"));
        }
        for term in &self.exclude {
            parts.push(format!("− {term}"));
        }
        if self.case_sensitive {
            parts.push("Aa".into());
        }
        if self.is_regex {
            parts.push(".*".into());
        }
        if self.min_level != LogLevel::Unknown {
            parts.push(format!("≥ {}", self.min_level.name()));
        }
        if self.has_window() {
            let side = |ms: Option<i64>| {
                ms.map_or_else(|| "…".to_string(), crate::timestamp::format_millis)
            };
            parts.push(format!(
                "🕘 {} → {}",
                side(self.time_from),
                side(self.time_to)
            ));
        }
        parts.join("\n")
    }
}

fn nonempty(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Whether `path` is the spool of a derived stream (`<pid>-<n>-filter-<name>`): such a
/// stream is not saved in the workspace, like standard input.
pub fn is_derived_path(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
        return false;
    };
    crate::spool::owner_pid(&name).is_some()
        && name
            .splitn(3, '-')
            .nth(2)
            .is_some_and(|rest| rest.starts_with("filter-"))
}

/// Why a derived stream stopped following its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// The source stream was closed.
    SourceClosed,
    /// The spool reached `stdin_spool_max_mb` (or the disk its free-space margin).
    Full,
    /// The spool could not be written.
    WriteFailed,
}

/// Feeds a derived stream's spool from its source stream. Kept in the derived engine
/// (`TailEngine::derived`).
#[derive(Debug)]
pub struct DerivedFeeder {
    /// The source stream (`TailEngine::path`) and its tab title when the tab was made.
    pub source: PathBuf,
    pub source_name: String,
    pub filter: FrozenFilter,
    spec: FilterSpec,
    spool: SpoolFile,
    out: Option<File>,
    /// Next source line to look at, and whether the entry above it passed (stack-trace
    /// lines follow their entry).
    next: usize,
    parent_visible: bool,
    /// Source line of every derived line, in order.
    pub map: Vec<usize>,
    /// `TailEngine::reload_generation` of the source the map was built on.
    source_generation: u64,
    written: u64,
    max_bytes: u64,
    pub stopped: Option<Stopped>,
    /// The source's size when last seen and since when it has not changed.
    seen_len: u64,
    seen_at: Instant,
    /// A last line read before its newline arrived (see `step`).
    partial: Option<usize>,
}

/// How long the source must stay the same size before an unterminated last line is read.
pub const SETTLED: Duration = Duration::from_secs(2);

impl DerivedFeeder {
    /// A new empty spool in `spool_dir` for a derived stream of `source`.
    pub fn create(
        source: &TailEngine,
        source_name: String,
        filter: FrozenFilter,
        spool_dir: &Path,
        max_bytes: u64,
    ) -> std::io::Result<Self> {
        // The name keeps the `filter-` prefix `is_derived_path` looks for (`sanitize`
        // would keep only what follows a path separator of the source name).
        let name = format!("filter-{}", crate::spool::sanitize(&source_name));
        let (spool, out) = SpoolFile::create(spool_dir, &name)?;
        Ok(Self {
            source: source.path.clone(),
            source_name,
            spec: filter.spec(),
            filter,
            spool,
            out: Some(out),
            next: 0,
            parent_visible: false,
            map: Vec::new(),
            source_generation: source.reload_generation,
            written: 0,
            max_bytes,
            stopped: None,
            seen_len: source.file_size,
            seen_at: Instant::now(),
            partial: None,
        })
    }

    /// The spool the derived engine reads.
    pub fn spool_path(&self) -> &Path {
        self.spool.path()
    }

    /// Source lines looked at so far (for the progress of the first fill).
    pub fn progress(&self, source: &TailEngine) -> f32 {
        let total = source.total_lines().max(1);
        (self.next as f32 / total as f32).min(1.0)
    }

    /// Empties the spool and starts reading the source again from its first line.
    fn restart(&mut self) -> bool {
        match self.spool.rewrite() {
            Ok(file) => self.out = Some(file),
            Err(_) => {
                self.stopped = Some(Stopped::WriteFailed);
                return false;
            }
        }
        self.next = 0;
        self.parent_visible = false;
        self.map.clear();
        self.written = 0;
        self.partial = None;
        true
    }

    /// Reads source lines and appends the ones the frozen filter passes, until the
    /// source's complete lines are all read or `budget` is spent. A reloaded source
    /// (truncated, rotated, rewritten) empties the spool and starts again. Returns
    /// whether the spool changed or work is left (the caller keeps stepping, repainting).
    pub fn step(&mut self, source: &TailEngine, budget: Duration) -> bool {
        if self.stopped.is_some() {
            return false;
        }
        let mut changed = false;
        if source.reload_generation != self.source_generation {
            self.source_generation = source.reload_generation;
            if !self.restart() {
                return false;
            }
            changed = true;
        }
        // While the source is (re)indexed its last line index spans unread text: wait.
        if source.index_pending {
            return true;
        }
        // Track growth: an unterminated last line is read once the source stops growing
        // for a moment; if it grows after that, the spool is written again.
        if source.file_size != self.seen_len {
            self.seen_len = source.file_size;
            self.seen_at = Instant::now();
            if self.partial.is_some() {
                if !self.restart() {
                    return false;
                }
                changed = true;
            }
        }
        // A time window reads the source's timestamp cache: wait until it is complete.
        if self.filter.has_window() && !source.timestamps_complete() {
            return true;
        }
        let idle = self.seen_at.elapsed() >= SETTLED;
        let end = if source.last_line_complete() || idle {
            source.total_lines()
        } else {
            source.total_lines().saturating_sub(1)
        };
        let started = Instant::now();
        while self.next < end {
            let idx = self.next;
            let text = source.get_line(idx).unwrap_or_default();
            let (visible, next) = self.spec.visible_in_sequence(&text, self.parent_visible);
            let in_window = !self.filter.has_window()
                || time_window_contains(
                    source.line_timestamp(idx),
                    self.filter.time_from,
                    self.filter.time_to,
                );
            if visible && in_window {
                let bytes = text.len() as u64 + 1;
                if self.written + bytes > self.max_bytes {
                    self.stopped = Some(Stopped::Full);
                    return changed;
                }
                let Some(out) = self.out.as_mut() else {
                    return changed;
                };
                if out
                    .write_all(text.as_bytes())
                    .and_then(|_| out.write_all(b"\n"))
                    .is_err()
                {
                    self.stopped = Some(Stopped::WriteFailed);
                    return changed;
                }
                self.written += bytes;
                self.map.push(idx);
                changed = true;
            }
            self.parent_visible = next;
            self.next += 1;
            if idx + 1 == source.total_lines() && !source.last_line_complete() {
                // Read before its newline arrived.
                self.partial = Some(idx);
            }
            if idx % 256 == 255 && started.elapsed() >= budget {
                break;
            }
        }
        if changed {
            if let Some(out) = self.out.as_mut() {
                let _ = out.flush();
            }
        }
        changed || self.next < end
    }

    /// The source line of derived line `idx`.
    pub fn source_line(&self, idx: usize) -> Option<usize> {
        self.map.get(idx).copied()
    }

    /// The derived line showing source line `source_line`, or the next one after it.
    pub fn derived_line_at_or_after(&self, source_line: usize) -> Option<usize> {
        let at = self.map.partition_point(|&l| l < source_line);
        (at < self.map.len()).then_some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(dir: &Path, name: &str, lines: &[&str]) -> TailEngine {
        let path = dir.join(name);
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, text).unwrap();
        TailEngine::open(&path).unwrap()
    }

    fn fill(feeder: &mut DerivedFeeder, source: &TailEngine) {
        for _ in 0..1000 {
            if !feeder.step(source, Duration::from_secs(5)) {
                return;
            }
        }
        panic!("the feeder never settles");
    }

    fn spool_text(feeder: &DerivedFeeder) -> String {
        std::fs::read_to_string(feeder.spool_path()).unwrap()
    }

    #[test]
    fn the_spool_holds_the_lines_the_frozen_filter_passes_with_their_traces() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = open(
            dir.path(),
            "app.log",
            &[
                "INFO start",
                "ERROR boom",
                "    at Foo.bar(Foo.java:1)",
                "INFO ok",
                "ERROR again",
            ],
        );
        source.set_include_filter("ERROR");
        let filter = FrozenFilter::of(&source);
        assert!(filter.is_active());
        assert_eq!(filter.label(), "ERROR");
        let mut feeder = DerivedFeeder::create(
            &source,
            "app.log".into(),
            filter,
            &dir.path().join("spool"),
            1 << 20,
        )
        .unwrap();
        fill(&mut feeder, &source);
        assert_eq!(
            spool_text(&feeder),
            "ERROR boom\n    at Foo.bar(Foo.java:1)\nERROR again\n"
        );
        assert_eq!(feeder.map, vec![1, 2, 4]);
        assert_eq!(feeder.derived_line_at_or_after(3), Some(2));
        assert_eq!(feeder.source_line(1), Some(2));

        // The source's filter changes afterwards: the frozen one does not.
        source.set_include_filter("INFO");
        fill(&mut feeder, &source);
        assert_eq!(feeder.map, vec![1, 2, 4]);
    }

    #[test]
    fn appends_are_followed_and_a_truncation_starts_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = open(dir.path(), "app.log", &["ERROR one", "INFO two"]);
        let path = source.path.clone();
        source.set_include_filter("ERROR");
        let filter = FrozenFilter::of(&source);
        let mut feeder = DerivedFeeder::create(
            &source,
            "app.log".into(),
            filter,
            &dir.path().join("spool"),
            1 << 20,
        )
        .unwrap();
        fill(&mut feeder, &source);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"INFO three\nERROR four\nERROR fi").unwrap();
        drop(f);
        source.poll_updates();
        fill(&mut feeder, &source);
        // The unterminated last line waits for its newline.
        assert_eq!(spool_text(&feeder), "ERROR one\nERROR four\n");
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"ve\n").unwrap();
        drop(f);
        source.poll_updates();
        fill(&mut feeder, &source);
        assert_eq!(spool_text(&feeder), "ERROR one\nERROR four\nERROR five\n");

        std::fs::write(&path, "ERROR new\n").unwrap();
        source.poll_updates();
        fill(&mut feeder, &source);
        assert_eq!(spool_text(&feeder), "ERROR new\n");
        assert_eq!(feeder.map, vec![0]);
    }

    #[test]
    fn an_unterminated_last_line_is_read_once_the_source_settles_and_rewritten_if_it_grows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, "ERROR one\nERROR two").unwrap();
        let mut source = TailEngine::open(&path).unwrap();
        source.set_include_filter("ERROR");
        let filter = FrozenFilter::of(&source);
        let mut feeder = DerivedFeeder::create(
            &source,
            "app.log".into(),
            filter,
            &dir.path().join("spool"),
            1 << 20,
        )
        .unwrap();
        fill(&mut feeder, &source);
        assert_eq!(spool_text(&feeder), "ERROR one\n");
        feeder.seen_at = Instant::now() - SETTLED;
        fill(&mut feeder, &source);
        assert_eq!(spool_text(&feeder), "ERROR one\nERROR two\n");
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b" more\n").unwrap();
        drop(f);
        source.poll_updates();
        fill(&mut feeder, &source);
        assert_eq!(spool_text(&feeder), "ERROR one\nERROR two more\n");
    }

    #[test]
    fn a_spool_named_after_an_archive_entry_is_still_a_derived_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = open(dir.path(), "x.log", &["ERROR a"]);
        source.set_include_filter("ERROR");
        let filter = FrozenFilter::of(&source);
        let feeder = DerivedFeeder::create(
            &source,
            "logs.zip › app/x.log".into(),
            filter,
            &dir.path().join("spool"),
            1 << 20,
        )
        .unwrap();
        assert!(
            is_derived_path(feeder.spool_path()),
            "{:?}",
            feeder.spool_path()
        );
        assert!(!is_derived_path(&dir.path().join("x.log")));
    }

    #[test]
    fn a_full_spool_stops_the_feeder() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = open(
            dir.path(),
            "app.log",
            &["ERROR a long enough line", "ERROR b"],
        );
        source.set_include_filter("ERROR");
        let filter = FrozenFilter::of(&source);
        let mut feeder = DerivedFeeder::create(
            &source,
            "app.log".into(),
            filter,
            &dir.path().join("spool"),
            10,
        )
        .unwrap();
        feeder.step(&source, Duration::from_secs(1));
        assert_eq!(feeder.stopped, Some(Stopped::Full));
        assert!(feeder.map.is_empty());
    }
}
