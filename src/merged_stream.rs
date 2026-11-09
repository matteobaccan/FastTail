// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Merged timelines: several open streams interleaved by timestamp in one stream.
//!
//! Like a derived stream ("Open filter as new tab", `filter_tab`), a merged view is an
//! ordinary `TailEngine` on a spool file: `MergedFeeder` writes the sources' lines into
//! it in time order (a k-way merge over the sources' next lines), so filters, search,
//! highlight rules, bookmarks and export work on it unchanged. `map` keeps the source
//! and source line of every merged line, for the source chips and the line numbers.
//!
//! The merge follows the sources live. A line is written once every enabled source has
//! either a next line to compare it with or has been quiet for `SETTLED`: a source that
//! is still writing cannot later produce an older line that should have come first.
//! Lines without a timestamp of their own (stack traces) take the one of the entry above
//! them and stay with it. A source that is reloaded (truncated, rotated), and a source
//! turned on or off, write the spool again from the start.

use crate::filter_tab::{Stopped, SETTLED};
use crate::spool::SpoolFile;
use crate::tail_engine::{TailEngine, NO_TIMESTAMP};
use crate::timestamp::FormatHint;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Most streams one merged view holds.
pub const MAX_SOURCES: usize = 16;

/// Longest a growing source without a next line holds the others (a line that grows
/// without ever ending must not stop the view).
const MAX_HOLD: Duration = Duration::from_secs(10);

/// Lines read from the start of a stream to tell whether it carries timestamps.
const PROBE_LINES: usize = 200;

/// Whether `engine` carries timestamps a merge can order by: one of its first lines has
/// one (in the text or, with a field parser, in a time field).
pub fn has_timestamps(engine: &TailEngine) -> bool {
    let parser = engine.field_parser().cloned();
    (0..engine.total_lines().min(PROBE_LINES)).any(|i| {
        engine.get_line(i).is_some_and(|text| {
            crate::fields::line_timestamp(parser.as_deref(), &text, FormatHint::default()).is_some()
        })
    })
}

/// Whether `path` is the spool of a merged view (`<pid>-<counter>-merged`), which is not
/// kept in a saved layout or session.
pub fn is_merged_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    crate::spool::owner_pid(name).is_some() && name.splitn(3, '-').nth(2) == Some("merged")
}

/// Prefix of the name a merged view is saved under: `merged:<n>:<source>|<source>...`,
/// each source by the name it is saved under itself. It is never a file, so an older
/// build skips it.
pub const IDENTITY_PREFIX: &str = "merged:";

/// Separates the sources in a saved name (not allowed in a Windows path; a source whose
/// saved name holds one is not saved).
const SOURCE_SEPARATOR: char = '|';

/// The saved name of merged view `n` of `sources`; `None` when there are fewer than two or
/// one holds the separator.
pub fn identity(n: usize, sources: &[PathBuf]) -> Option<PathBuf> {
    let names: Vec<String> = sources
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    if names.len() < 2
        || names
            .iter()
            .any(|s| s.is_empty() || s.contains(SOURCE_SEPARATOR))
    {
        return None;
    }
    let joined = names.join(&SOURCE_SEPARATOR.to_string());
    Some(PathBuf::from(format!("{IDENTITY_PREFIX}{n}:{joined}")))
}

/// The number and the sources of a saved name made by `identity`.
pub fn parse_identity(path: &Path) -> Option<(usize, Vec<PathBuf>)> {
    let rest = path.to_str()?.strip_prefix(IDENTITY_PREFIX)?;
    let (n, sources) = rest.split_once(':')?;
    let n = n.parse().ok()?;
    let sources: Vec<PathBuf> = sources.split(SOURCE_SEPARATOR).map(PathBuf::from).collect();
    (sources.len() >= 2 && sources.iter().all(|p| !p.as_os_str().is_empty()))
        .then_some((n, sources))
}

/// The engine of merged view `n` of the open streams `found` (two or more), following,
/// with the configuration's stream settings: saved by the names its sources are saved
/// under, not saved when one of them is standard input or the clipboard.
pub fn open_view(
    n: usize,
    found: &[&TailEngine],
    config: &crate::config::FastTailConfig,
) -> std::io::Result<TailEngine> {
    let named: Vec<(PathBuf, String)> = found
        .iter()
        .map(|e| (e.path.clone(), crate::find_all::stream_name(e)))
        .collect();
    let saved: Vec<PathBuf> = if found.iter().all(|e| crate::workspace::is_persisted(e)) {
        found
            .iter()
            .map(|e| crate::workspace::persisted_path(e))
            .collect()
    } else {
        Vec::new()
    };
    let spool_dir = config.compressed_settings().spool_dir;
    let max_bytes = u64::from(config.stdin_spool_max_mb) * 1024 * 1024;
    let mut feeder = MergedFeeder::create(n, named, &spool_dir, max_bytes)?;
    feeder.saved_sources = saved;
    let mut engine = TailEngine::open(feeder.spool_path())?;
    engine.merged = Some(Box::new(feeder));
    crate::workspace::apply_settings(&mut engine, config);
    engine.follow_tail = true;
    Ok(engine)
}

/// The number for a new merged view among `engines`: one past the highest in use.
pub fn next_number<'a>(engines: impl Iterator<Item = &'a TailEngine>) -> usize {
    1 + engines
        .filter_map(|e| e.merged.as_ref().map(|m| m.n))
        .max()
        .unwrap_or(0)
}

/// Feeds every merged view among `engines` from its sources (the other engines, by
/// path), `budget` each. Returns whether any spool changed or has work left.
pub fn feed_views(engines: &mut [&mut TailEngine], budget: Duration) -> bool {
    let mut changed = false;
    for m in 0..engines.len() {
        let Some(mut feeder) = engines[m].merged.take() else {
            continue;
        };
        let sources: Vec<Option<&TailEngine>> = feeder
            .sources
            .iter()
            .map(|s| {
                engines
                    .iter()
                    .find(|e| e.merged.is_none() && crate::paths::paths_equal(&e.path, &s.path))
                    .map(|e| &**e)
            })
            .collect();
        changed |= feeder.step(&sources, budget);
        engines[m].merged = Some(feeder);
    }
    changed
}

/// One stream of a merged view and how far it has been read.
#[derive(Debug, Clone)]
pub struct MergedSource {
    /// The stream (`TailEngine::path`) and its short name, as its tab shows it.
    pub path: PathBuf,
    pub name: String,
    /// Off: its lines leave the merged view (the spool is written again).
    pub enabled: bool,
    /// Next line to merge, the timestamp the lines without one inherit, the format that
    /// matched last.
    next: usize,
    inherited: i64,
    hint: FormatHint,
    /// The next line, read once while it waits its turn.
    head: Option<Head>,
    /// A line written before its newline arrived.
    partial: Option<usize>,
    /// `TailEngine::reload_generation` the read position belongs to.
    generation: u64,
    /// Its size when last seen (`None`: not seen yet) and since when it has not changed.
    seen_len: Option<u64>,
    seen_at: Instant,
}

impl MergedSource {
    fn new(path: PathBuf, name: String) -> Self {
        Self {
            path,
            name,
            enabled: true,
            next: 0,
            inherited: NO_TIMESTAMP,
            hint: FormatHint::default(),
            head: None,
            partial: None,
            generation: 0,
            seen_len: None,
            // Quiet from the start: a source that is not growing never holds the others.
            seen_at: Instant::now() - SETTLED,
        }
    }

    fn reset(&mut self) {
        self.next = 0;
        self.inherited = NO_TIMESTAMP;
        self.hint = FormatHint::default();
        self.head = None;
        self.partial = None;
    }
}

/// A source's next line waiting its turn: its index, timestamp (its own, or inherited),
/// the format that matched, its text, and whether it had no newline yet.
#[derive(Debug, Clone)]
struct Head {
    line: usize,
    ts: i64,
    hint: FormatHint,
    text: String,
    unterminated: bool,
}

/// Feeds a merged view's spool from its sources. Kept in the merged engine
/// (`TailEngine::merged`).
#[derive(Debug)]
pub struct MergedFeeder {
    /// Tells the merged views apart in their saved name.
    pub n: usize,
    pub sources: Vec<MergedSource>,
    spool: SpoolFile,
    out: Option<File>,
    /// Source index and source line of every merged line, in order.
    pub map: Vec<(u8, usize)>,
    /// The source of the line written last: on equal timestamps it goes on.
    last: Option<usize>,
    /// Since when a growing source without a next line has held the merge.
    held_since: Option<Instant>,
    /// A source was turned on or off: the next step writes the spool again.
    restart_pending: bool,
    written: u64,
    max_bytes: u64,
    pub stopped: Option<Stopped>,
    /// The sources by the names they are saved under (see `identity`); empty: the view is
    /// not saved (a source is standard input).
    pub saved_sources: Vec<PathBuf>,
}

impl MergedFeeder {
    /// A new empty spool in `spool_dir` for a merged view of `sources` (path and short
    /// name of each, at most `MAX_SOURCES`).
    pub fn create(
        n: usize,
        sources: Vec<(PathBuf, String)>,
        spool_dir: &Path,
        max_bytes: u64,
    ) -> std::io::Result<Self> {
        let (spool, out) = SpoolFile::create(spool_dir, "merged")?;
        Ok(Self {
            n,
            sources: sources
                .into_iter()
                .take(MAX_SOURCES)
                .map(|(path, name)| MergedSource::new(path, name))
                .collect(),
            spool,
            out: Some(out),
            map: Vec::new(),
            last: None,
            held_since: None,
            restart_pending: false,
            written: 0,
            max_bytes,
            stopped: None,
            saved_sources: Vec::new(),
        })
    }

    /// The name the view is saved under in the workspace and sessions, `None` when it is
    /// not saved.
    pub fn identity(&self) -> Option<PathBuf> {
        identity(self.n, &self.saved_sources)
    }

    /// The spool the merged engine reads.
    pub fn spool_path(&self) -> &Path {
        self.spool.path()
    }

    /// The source index and source line of merged line `idx`.
    pub fn source_of(&self, idx: usize) -> Option<(usize, usize)> {
        self.map.get(idx).map(|&(s, l)| (s as usize, l))
    }

    /// The tab title: the sources' names joined, `+N` past three.
    pub fn title(&self) -> String {
        let names: Vec<&str> = self.sources.iter().map(|s| s.name.as_str()).collect();
        if names.len() <= 3 {
            names.join(" + ")
        } else {
            format!("{} +{}", names[..2].join(" + "), names.len() - 2)
        }
    }

    /// Turns source `i` on or off: the spool is written again with the sources left.
    pub fn set_enabled(&mut self, i: usize, on: bool) {
        if let Some(s) = self.sources.get_mut(i) {
            if s.enabled != on {
                s.enabled = on;
                self.restart_pending = true;
            }
        }
    }

    /// Empties the spool and reads every source again from its first line. A view that
    /// stopped (full, every source closed) follows again; one that cannot write stays
    /// stopped.
    fn restart(&mut self) -> bool {
        match self.spool.rewrite() {
            Ok(file) => self.out = Some(file),
            Err(_) => {
                self.stopped = Some(Stopped::WriteFailed);
                return false;
            }
        }
        for s in &mut self.sources {
            s.reset();
        }
        self.map.clear();
        self.last = None;
        self.written = 0;
        self.held_since = None;
        self.restart_pending = false;
        if self.stopped != Some(Stopped::WriteFailed) {
            self.stopped = None;
        }
        true
    }

    /// Merges the sources' lines into the spool until no line can be placed yet or
    /// `budget` is spent. `engines[i]` is the open stream of source `i` (`None`: closed,
    /// it adds nothing more). Returns whether the spool changed or work is left.
    pub fn step(&mut self, engines: &[Option<&TailEngine>], budget: Duration) -> bool {
        // A source reloaded (rotated, truncated, re-decoded) since it was read, or one
        // whose unterminated last line was written and has grown since, writes the
        // spool again; so does a toggle.
        let mut restart = self.restart_pending;
        for (s, e) in self.sources.iter_mut().zip(engines) {
            let Some(e) = e else {
                continue;
            };
            let read = s.next > 0 || s.head.is_some();
            if e.reload_generation != s.generation && read {
                restart = true;
            }
            s.generation = e.reload_generation;
            if s.seen_len.is_some_and(|len| len != e.file_size) {
                // Growth seen after the first look: the source is being written to.
                s.seen_at = Instant::now();
                if s.partial.is_some() {
                    restart = true;
                }
            }
            s.seen_len = Some(e.file_size);
        }
        if restart {
            // Nothing is written in this call: the merged engine sees the empty spool
            // first, or a refill to the old size would hide the truncation from it.
            self.restart();
            return true;
        }
        if self.stopped.is_some() {
            return false;
        }
        if self
            .sources
            .iter()
            .zip(engines)
            .any(|(s, e)| s.enabled && e.is_some_and(|e| e.index_pending))
        {
            return true;
        }
        if engines.iter().all(Option::is_none) {
            self.stopped = Some(Stopped::SourceClosed);
            return false;
        }
        let parsers: Vec<Option<std::sync::Arc<crate::fields::FieldParser>>> = engines
            .iter()
            .map(|e| e.and_then(|e| e.field_parser().cloned()))
            .collect();
        let started = Instant::now();
        let mut changed = false;
        let mut written_now = 0usize;
        loop {
            // The next line of every enabled open source; a source without one that is
            // still growing holds the merge, for at most `MAX_HOLD`.
            let mut best: Option<(i64, usize)> = None;
            let mut held = false;
            for (i, (s, e)) in self.sources.iter_mut().zip(engines).enumerate() {
                let Some(e) = e.filter(|_| s.enabled) else {
                    continue;
                };
                if s.head.is_none() {
                    let idle = s.seen_at.elapsed() >= SETTLED;
                    let complete = e.last_line_complete();
                    let end = if complete || idle {
                        e.total_lines()
                    } else {
                        e.total_lines().saturating_sub(1)
                    };
                    if s.next < end {
                        let text = e.get_line(s.next).unwrap_or_default().into_owned();
                        let (ts, hint) = match crate::fields::line_timestamp(
                            parsers[i].as_deref(),
                            &text,
                            s.hint,
                        ) {
                            Some((ts, hint)) => (ts, hint),
                            None => (s.inherited, s.hint),
                        };
                        let unterminated = !complete && s.next + 1 == e.total_lines();
                        s.head = Some(Head {
                            line: s.next,
                            ts,
                            hint,
                            text,
                            unterminated,
                        });
                    } else if !idle {
                        held = true;
                    }
                }
                if let Some(head) = &s.head {
                    let better = match best {
                        None => true,
                        Some((b, j)) => {
                            head.ts < b || (head.ts == b && self.last == Some(i) && j != i)
                        }
                    };
                    if better {
                        best = Some((head.ts, i));
                    }
                }
            }
            if held {
                let since = *self.held_since.get_or_insert_with(Instant::now);
                if since.elapsed() < MAX_HOLD {
                    break;
                }
            } else {
                self.held_since = None;
            }
            let Some((_, i)) = best else {
                break;
            };
            let s = &mut self.sources[i];
            let Some(head) = s.head.take() else {
                break;
            };
            let bytes = head.text.len() as u64 + 1;
            if self.written + bytes > self.max_bytes {
                self.stopped = Some(Stopped::Full);
                break;
            }
            let Some(out) = self.out.as_mut() else {
                break;
            };
            if out
                .write_all(head.text.as_bytes())
                .and_then(|_| out.write_all(b"\n"))
                .is_err()
            {
                self.stopped = Some(Stopped::WriteFailed);
                break;
            }
            self.written += bytes;
            self.map.push((i as u8, head.line));
            s.next = head.line + 1;
            s.inherited = head.ts;
            s.hint = head.hint;
            if head.unterminated {
                // Written before its newline: the source growing writes it again.
                s.partial = Some(head.line);
            }
            self.last = Some(i);
            changed = true;
            written_now += 1;
            if written_now.is_multiple_of(256) && started.elapsed() >= budget {
                break;
            }
        }
        if changed {
            if let Some(out) = self.out.as_mut() {
                let _ = out.flush();
            }
        }
        if self.stopped.is_some() {
            return changed;
        }
        let left = self
            .sources
            .iter()
            .zip(engines)
            .any(|(s, e)| s.enabled && e.is_some_and(|e| s.next < e.total_lines()));
        changed || left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Merges until nothing is left, the sources taken as quiet (no one writing).
    fn fill(feeder: &mut MergedFeeder, engines: &[Option<&TailEngine>]) {
        for _ in 0..1000 {
            for s in &mut feeder.sources {
                s.seen_at = Instant::now() - SETTLED;
            }
            if !feeder.step(engines, Duration::from_secs(5)) {
                return;
            }
        }
        panic!("the merge never settles");
    }

    fn spool_text(feeder: &MergedFeeder) -> String {
        std::fs::read_to_string(feeder.spool_path()).unwrap()
    }

    fn feeder(dir: &Path, engines: &[&TailEngine]) -> MergedFeeder {
        let sources = engines
            .iter()
            .map(|e| (e.path.clone(), crate::find_all::stream_name(e)))
            .collect();
        MergedFeeder::create(1, sources, &dir.join("spool"), u64::MAX).unwrap()
    }

    #[test]
    fn two_services_interleave_by_time_with_their_traces() {
        let dir = tempfile::tempdir().unwrap();
        let gw = write(
            dir.path(),
            "gateway.log",
            &[
                "2026-09-28T14:02:05.100Z INFO gw request 42",
                "2026-09-28T14:02:05.400Z ERROR gw failed",
                "    at gw.frame",
            ],
        );
        let pay = write(
            dir.path(),
            "payment.log",
            &[
                "2026-09-28T14:02:05.050Z INFO pay ready",
                "2026-09-28T14:02:05.250Z INFO pay request 42",
                "2026-09-28T14:02:05.400Z INFO pay same time",
            ],
        );
        let a = TailEngine::open(&gw).unwrap();
        let b = TailEngine::open(&pay).unwrap();
        assert!(has_timestamps(&a) && has_timestamps(&b));
        let mut f = feeder(dir.path(), &[&a, &b]);
        fill(&mut f, &[Some(&a), Some(&b)]);
        let text = spool_text(&f);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 6);
        assert!(lines[0].contains("pay ready"));
        assert!(lines[1].contains("gw request 42"));
        assert!(lines[2].contains("pay request 42"));
        // Equal times: the source written last goes on; a trace stays with its entry.
        assert!(lines[3].contains("pay same time"));
        assert!(lines[4].contains("gw failed"));
        assert_eq!(lines[5], "    at gw.frame");
        assert_eq!(f.source_of(5), Some((0, 2)));
        assert_eq!(f.source_of(3), Some((1, 2)));
        assert_eq!(f.title(), "gateway.log + payment.log");
    }

    #[test]
    fn appends_are_merged_and_a_toggle_or_a_truncation_writes_again() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = write(dir.path(), "a.log", &["2026-01-01T10:00:00Z a1"]);
        let b_path = write(dir.path(), "b.log", &["2026-01-01T10:00:01Z b1"]);
        let mut a = TailEngine::open(&a_path).unwrap();
        let mut b = TailEngine::open(&b_path).unwrap();
        a.size_check_interval = Duration::ZERO;
        b.size_check_interval = Duration::ZERO;
        let mut f = feeder(dir.path(), &[&a, &b]);
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert_eq!(f.map.len(), 2);
        use std::io::Write as _;
        let mut fa = std::fs::OpenOptions::new()
            .append(true)
            .open(&a_path)
            .unwrap();
        writeln!(fa, "2026-01-01T10:00:02Z a2").unwrap();
        drop(fa);
        a.poll_updates();
        // `a` grew just now: `b` has no next line but has been quiet, so a2 goes in.
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert!(spool_text(&f).ends_with("a2\n"), "{}", spool_text(&f));
        f.set_enabled(1, false);
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert_eq!(spool_text(&f).lines().count(), 2, "b off");
        f.set_enabled(1, true);
        std::fs::write(&a_path, "2026-01-01T09:00:00Z new a\n").unwrap();
        a.poll_updates();
        fill(&mut f, &[Some(&a), Some(&b)]);
        let text = spool_text(&f);
        assert!(text.starts_with("2026-01-01T09:00:00Z new a\n"), "{text}");
        assert_eq!(text.lines().count(), 2);
        // Every source closed: the view stops following.
        f.step(&[None, None], Duration::from_secs(1));
        assert_eq!(f.stopped, Some(Stopped::SourceClosed));
    }

    #[test]
    fn a_growing_source_without_a_next_line_holds_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = write(dir.path(), "a.log", &["2026-01-01T10:00:00Z a1"]);
        let b_path = write(
            dir.path(),
            "b.log",
            &["2026-01-01T10:00:01Z b1", "2026-01-01T10:00:05Z b2"],
        );
        let a = TailEngine::open(&a_path).unwrap();
        let b = TailEngine::open(&b_path).unwrap();
        let mut f = feeder(dir.path(), &[&a, &b]);
        // `a` just grew (as if written a moment ago): b's lines wait for it.
        f.sources[0].seen_len = Some(a.file_size);
        f.sources[0].seen_at = Instant::now();
        f.step(&[Some(&a), Some(&b)], Duration::from_secs(1));
        assert_eq!(spool_text(&f), "2026-01-01T10:00:00Z a1\n");
        f.sources[0].seen_at = Instant::now() - SETTLED;
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert_eq!(spool_text(&f).lines().count(), 3);
    }

    #[test]
    fn a_stream_without_timestamps_cannot_be_merged() {
        let dir = tempfile::tempdir().unwrap();
        let plain = write(dir.path(), "plain.log", &["no time here", "nor here"]);
        let e = TailEngine::open(&plain).unwrap();
        assert!(!has_timestamps(&e));
    }

    #[test]
    fn an_unterminated_line_is_written_again_once_it_grows() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = dir.path().join("a.log");
        std::fs::write(&a_path, "2026-01-01T10:00:00Z a1\n2026-01-01T10:00:01Z hal").unwrap();
        let b_path = write(dir.path(), "b.log", &["2026-01-01T10:00:05Z b1"]);
        let mut a = TailEngine::open(&a_path).unwrap();
        a.size_check_interval = Duration::ZERO;
        let b = TailEngine::open(&b_path).unwrap();
        let mut f = feeder(dir.path(), &[&a, &b]);
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert!(spool_text(&f).contains("hal\n"), "quiet: written as it is");
        use std::io::Write as _;
        let mut fa = std::fs::OpenOptions::new()
            .append(true)
            .open(&a_path)
            .unwrap();
        writeln!(fa, "f done").unwrap();
        drop(fa);
        a.poll_updates();
        fill(&mut f, &[Some(&a), Some(&b)]);
        let text = spool_text(&f);
        assert!(text.contains("half done\n"), "{text}");
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn a_full_view_follows_again_once_a_source_is_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = write(dir.path(), "a.log", &["2026-01-01T10:00:00Z a1"]);
        let b_path = write(
            dir.path(),
            "b.log",
            &["2026-01-01T10:00:01Z bbbbbbbbbbbbbbbbbbbbbbbbbbbb"],
        );
        let a = TailEngine::open(&a_path).unwrap();
        let b = TailEngine::open(&b_path).unwrap();
        let sources = vec![(a.path.clone(), "a".into()), (b.path.clone(), "b".into())];
        let mut f = MergedFeeder::create(1, sources, &dir.path().join("spool"), 40).unwrap();
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert_eq!(f.stopped, Some(Stopped::Full));
        f.set_enabled(1, false);
        fill(&mut f, &[Some(&a), Some(&b)]);
        assert_eq!(f.stopped, None);
        assert_eq!(spool_text(&f), "2026-01-01T10:00:00Z a1\n");
    }

    #[test]
    fn a_first_line_waiting_its_turn_is_read_again_after_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = write(dir.path(), "a.log", &["2026-01-01T12:00:00Z old a"]);
        let b_path = write(dir.path(), "b.log", &["2026-01-01T10:00:00Z b1"]);
        let mut a = TailEngine::open(&a_path).unwrap();
        a.size_check_interval = Duration::ZERO;
        let b = TailEngine::open(&b_path).unwrap();
        let mut f = feeder(dir.path(), &[&a, &b]);
        // b is being written: a's first line waits.
        f.sources[1].seen_len = Some(b.file_size);
        f.sources[1].seen_at = Instant::now();
        f.step(&[Some(&a), Some(&b)], Duration::from_secs(1));
        f.sources[1].seen_at = Instant::now();
        f.step(&[Some(&a), Some(&b)], Duration::from_secs(1));
        std::fs::write(&a_path, "2026-01-01T09:00:00Z new a\n").unwrap();
        a.poll_updates();
        fill(&mut f, &[Some(&a), Some(&b)]);
        let text = spool_text(&f);
        assert!(text.starts_with("2026-01-01T09:00:00Z new a\n"), "{text}");
        assert!(!text.contains("old a"));
    }

    #[test]
    fn the_saved_name_holds_the_sources() {
        let sources = vec![
            PathBuf::from("/l/a.log"),
            PathBuf::from("filter:1:/l/b.log"),
        ];
        let name = identity(3, &sources).unwrap();
        assert_eq!(name, PathBuf::from("merged:3:/l/a.log|filter:1:/l/b.log"));
        assert_eq!(parse_identity(&name), Some((3, sources)));
        assert_eq!(identity(1, &[PathBuf::from("/a")]), None, "one source");
        let piped = [PathBuf::from("/a|b"), PathBuf::from("/c")];
        assert_eq!(identity(1, &piped), None);
        assert_eq!(parse_identity(Path::new("merged:x:/a|/b")), None);
        assert_eq!(parse_identity(Path::new("merged:1:/a")), None);
        assert_eq!(parse_identity(Path::new("/var/log/merged:1:/a|/b")), None);
    }
}
