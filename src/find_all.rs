//! Search across every open stream: one query, one background job per stream.
//!
//! A `FindAllSession` owns its own `ScanJob`s, spawned over each stream's current file with
//! the stream's filter cloned at start, so it never cancels (and is never cancelled by) the
//! stream's own index, filter, search, level or timestamp scan. At most
//! `concurrency_limit()` jobs run at once, the others wait in the order of the streams.
//! The match is the per-stream search's: case-insensitive text over the lines the stream
//! shows under its include / exclude / level filter (evaluated by the job) and its time
//! window (applied to each drained batch). Streams in HEX view are skipped: their search
//! is byte-level.
//!
//! Results are a snapshot: each job covers the file as indexed when it started. A stream
//! reloaded since (truncation, rotation, rewrite, pattern switch) bumps the engine's
//! `reload_generation`; its group turns stale and its hits no longer jump.

use std::path::PathBuf;
use std::time::Instant;

use crate::paths::paths_equal_fast;
use crate::scan_job::{JobSpec, ScanBatch, ScanJob};
use crate::tail_engine::{time_window_contains, TailEngine, ViewMode};

/// Hits stored per stream; past it they are counted, not listed.
pub const FIND_ALL_MAX_HITS: usize = 100_000;
/// Most search jobs running at once, whatever the core count.
pub const MAX_CONCURRENT_JOBS: usize = 4;

/// `min(4, available cores)`, at least one.
pub fn concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, MAX_CONCURRENT_JOBS)
}

/// Where the search of one stream stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindState {
    /// Waiting for a free slot, or for the stream's index or time window to be ready.
    Queued,
    Running,
    Done,
    /// Stopped by the user (Stop) before it finished; what was found stays.
    Stopped,
    /// The file could not be read.
    Failed,
    /// The stream is in HEX view: its search is byte-level, not searched.
    SkippedHex,
    /// The stream was reloaded since its job started: its line numbers mean other lines.
    Stale,
}

/// Results of one stream.
pub struct StreamFind {
    /// Identity of the stream (`TailEngine::path`).
    pub path: PathBuf,
    /// Name shown in the group header (as the tab names it).
    pub name: String,
    /// `TailEngine::reload_generation` when the job started.
    pub reload_generation: u64,
    job: Option<ScanJob>,
    /// Time window of the stream when the job started, applied to the drained hits.
    window: Option<(Option<i64>, Option<i64>)>,
    /// Matching line indices in file order, at most the session's cap.
    pub hits: Vec<usize>,
    /// Every match, the ones past the cap included.
    pub total: usize,
    pub progress: f32,
    pub state: FindState,
    /// Collapsed in the results list.
    pub collapsed: bool,
}

impl StreamFind {
    /// More matches than listed.
    pub fn capped(&self) -> bool {
        self.total > self.hits.len()
    }

    /// A result of this group may be jumped to.
    pub fn jumpable(&self) -> bool {
        self.state != FindState::Stale
    }
}

/// The search across streams shown by the Find results tab.
pub struct FindAllSession {
    /// Text of the query box: what the next Find runs.
    pub input: String,
    /// Query of the results listed, as it was run (trimmed); empty before the first run.
    pub query: String,
    /// When the query was run (the snapshot time).
    pub started_at: Option<Instant>,
    /// One group per stream, in the order of the streams when the query ran.
    pub groups: Vec<StreamFind>,
    /// A result was committed: the app activates that stream and shows the line after
    /// the dock is drawn (the tab cannot change the dock while it is drawn).
    pub jump: Option<(PathBuf, usize)>,
    /// The query box takes the keyboard on the next frame (Ctrl+Shift+F).
    pub focus_input: bool,
    limit: usize,
    max_hits: usize,
    next_generation: u64,
}

impl Default for FindAllSession {
    fn default() -> Self {
        Self::with_limits(concurrency_limit(), FIND_ALL_MAX_HITS)
    }
}

impl FindAllSession {
    /// A session running at most `limit` jobs at once and storing `max_hits` per stream.
    pub fn with_limits(limit: usize, max_hits: usize) -> Self {
        Self {
            input: String::new(),
            query: String::new(),
            started_at: None,
            groups: Vec::new(),
            jump: None,
            focus_input: false,
            limit: limit.max(1),
            max_hits,
            next_generation: 0,
        }
    }

    pub fn concurrency(&self) -> usize {
        self.limit
    }

    pub fn max_hits(&self) -> usize {
        self.max_hits
    }

    /// Runs the query box text over `engines` (in that order). The previous results and
    /// their jobs are dropped (dropping a job cancels it). An empty query only clears.
    pub fn start(&mut self, engines: &[TailEngine]) {
        self.groups.clear();
        self.query = self.input.trim().to_string();
        if self.query.is_empty() {
            self.started_at = None;
            return;
        }
        self.started_at = Some(Instant::now());
        self.groups = engines
            .iter()
            .map(|engine| StreamFind {
                path: engine.path.clone(),
                name: stream_name(engine),
                reload_generation: engine.reload_generation,
                job: None,
                window: None,
                hits: Vec::new(),
                total: 0,
                progress: 0.0,
                state: if engine.view_mode == ViewMode::Hex {
                    FindState::SkippedHex
                } else {
                    FindState::Queued
                },
                collapsed: false,
            })
            .collect();
        self.launch(engines);
    }

    /// Runs the last query again over the streams as they are now.
    pub fn refresh(&mut self, engines: &[TailEngine]) {
        self.input = self.query.clone();
        self.start(engines);
    }

    /// Stops every running and queued job; the results found so far stay.
    pub fn stop(&mut self) {
        for g in &mut self.groups {
            if matches!(g.state, FindState::Queued | FindState::Running) {
                g.job = None;
                g.state = FindState::Stopped;
            }
        }
    }

    /// Forgets the results and their jobs (the Find results tab was closed).
    pub fn close(&mut self) {
        self.groups.clear();
        self.query.clear();
        self.started_at = None;
        self.jump = None;
    }

    /// Once per frame: drops the groups of closed streams, marks reloaded ones stale,
    /// drains the running jobs and starts queued ones in the free slots.
    pub fn poll(&mut self, engines: &[TailEngine]) {
        if self.groups.is_empty() {
            return;
        }
        self.groups
            .retain(|g| find_engine(engines, &g.path).is_some());
        for g in &mut self.groups {
            let Some(engine) = find_engine(engines, &g.path) else {
                continue;
            };
            let searched = matches!(
                g.state,
                FindState::Running | FindState::Done | FindState::Stopped | FindState::Failed
            );
            if searched && engine.reload_generation != g.reload_generation {
                // The line numbers now mean other lines: keep the rows, stop jumping.
                g.job = None;
                g.state = FindState::Stale;
                continue;
            }
            if g.state == FindState::Running {
                drain(g, engine, self.max_hits);
            }
        }
        self.launch(engines);
    }

    /// Starts queued jobs, in order, while fewer than `limit` run. A stream still being
    /// indexed, or whose time window waits for its timing, keeps its place in the queue.
    fn launch(&mut self, engines: &[TailEngine]) {
        let mut running = self.running_count();
        let query_lower = self.query.to_lowercase();
        for g in &mut self.groups {
            if running >= self.limit {
                break;
            }
            if g.state != FindState::Queued {
                continue;
            }
            let Some(engine) = find_engine(engines, &g.path) else {
                continue;
            };
            if engine.index_pending || engine.time_range_pending() {
                continue;
            }
            let (file, range) = engine.full_scan_range();
            let window = engine.time_window();
            // Under a time window the drain drops hits, so the worker must not stop listing
            // at the cap: the drain caps and counts (as the stream's own search does).
            let limit = if window.is_some() {
                usize::MAX
            } else {
                self.max_hits
            };
            self.next_generation = self.next_generation.wrapping_add(1);
            g.job = Some(ScanJob::spawn(
                self.next_generation,
                &file,
                range,
                JobSpec::Search {
                    query_lower: query_lower.clone(),
                    filter: engine.filter_spec(),
                    limit,
                    count_past_limit: true,
                },
            ));
            g.window = window;
            g.reload_generation = engine.reload_generation;
            g.state = FindState::Running;
            running += 1;
        }
    }

    pub fn running_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| g.state == FindState::Running)
            .count()
    }

    pub fn queued_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| g.state == FindState::Queued)
            .count()
    }

    /// Jobs are running or waiting.
    pub fn is_active(&self) -> bool {
        self.groups
            .iter()
            .any(|g| matches!(g.state, FindState::Queued | FindState::Running))
    }

    /// Every match of every stream, counted past the caps too.
    pub fn total_hits(&self) -> usize {
        self.groups.iter().map(|g| g.total).sum()
    }

    /// Streams with at least one match.
    pub fn streams_with_hits(&self) -> usize {
        self.groups.iter().filter(|g| g.total > 0).count()
    }

    /// Asks for a jump to hit `hit` of group `group`; refused on a stale group.
    pub fn commit(&mut self, group: usize, hit: usize) -> bool {
        let Some(g) = self.groups.get(group) else {
            return false;
        };
        let Some(&line) = g.hits.get(hit) else {
            return false;
        };
        if !g.jumpable() {
            return false;
        }
        self.jump = Some((g.path.clone(), line));
        true
    }
}

/// Applies the batches a group's job sent since the last poll.
fn drain(g: &mut StreamFind, engine: &TailEngine, max_hits: usize) {
    let Some(job) = g.job.as_ref() else {
        return;
    };
    let mut finished = None;
    while let Some(batch) = job.try_recv() {
        match batch {
            ScanBatch::Lines(mut lines) => {
                if let Some((from, to)) = g.window {
                    lines.retain(|&idx| time_window_contains(engine.line_timestamp(idx), from, to));
                }
                g.total += lines.len();
                let room = max_hits.saturating_sub(g.hits.len());
                g.hits.extend(lines.into_iter().take(room));
            }
            ScanBatch::Counted { hits, .. } => g.total += hits,
            ScanBatch::Progress(p) => g.progress = p,
            ScanBatch::Done { .. } => {
                finished = Some(FindState::Done);
                break;
            }
            ScanBatch::Failed => {
                finished = Some(FindState::Failed);
                break;
            }
            _ => {}
        }
    }
    if let Some(state) = finished {
        g.job = None;
        g.state = state;
        if state == FindState::Done {
            g.progress = 1.0;
        }
    }
}

fn find_engine<'a>(engines: &'a [TailEngine], path: &std::path::Path) -> Option<&'a TailEngine> {
    engines
        .iter()
        .find(|e| e.path == path)
        .or_else(|| engines.iter().find(|e| paths_equal_fast(&e.path, path)))
}

/// The stream as its tab names it: the pattern and the file it resolves to, the archive
/// and the entry, or the file name.
pub fn stream_name(engine: &TailEngine) -> String {
    let file_name = engine
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| engine.path.display().to_string());
    match (engine.current_file_name(), &engine.compressed) {
        (_, Some(c)) => c.title(),
        (Some(current), None) if engine.is_pattern() => format!("{file_name} ▸ {current}"),
        _ => file_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn open(dir: &std::path::Path, name: &str, text: &str) -> TailEngine {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        let mut engine = TailEngine::open(&path).unwrap();
        engine.size_check_interval = Duration::ZERO;
        engine
    }

    /// Polls until no job runs or waits, checking the concurrency bound on every poll;
    /// returns the most jobs seen running at once.
    fn run_to_end(session: &mut FindAllSession, engines: &[TailEngine]) -> usize {
        let started = Instant::now();
        let mut peak = session.running_count();
        while session.is_active() {
            assert!(started.elapsed() < Duration::from_secs(30), "search hangs");
            std::thread::sleep(Duration::from_millis(2));
            session.poll(engines);
            let running = session.running_count();
            assert!(running <= session.concurrency(), "{running} jobs at once");
            peak = peak.max(running);
        }
        peak
    }

    fn lines(n: usize, f: impl Fn(usize) -> String) -> String {
        (0..n).map(|i| f(i) + "\n").collect()
    }

    #[test]
    fn results_match_each_stream_own_search_under_its_filters() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = open(
            dir.path(),
            "gateway.log",
            "INFO req-7f3a in\nINFO other\nWARN REQ-7F3A retry\nERROR req-7f3a fail\n    at req-7f3a.frame\nINFO req-7f3a out\n",
        );
        let mut payment = open(
            dir.path(),
            "payment.log",
            "INFO req-7f3a charge\nINFO req-7f3a healthcheck\nDEBUG noise\nINFO req-7f3a done\n",
        );
        payment.set_exclude_filter("healthcheck");
        let audit = open(dir.path(), "audit.log", "INFO nothing here\n");
        let mut engines = vec![gateway, payment, audit];

        let mut session = FindAllSession::with_limits(4, FIND_ALL_MAX_HITS);
        session.input = "  req-7f3a ".into();
        session.start(&engines);
        run_to_end(&mut session, &engines);

        assert_eq!(session.query, "req-7f3a");
        let counts: Vec<usize> = session.groups.iter().map(|g| g.total).collect();
        assert_eq!(counts, vec![5, 2, 0]);
        assert_eq!(session.total_hits(), 7);
        assert_eq!(session.streams_with_hits(), 2);
        assert!(session.groups.iter().all(|g| g.state == FindState::Done));
        // Same lines as each stream's own search, and the stream's search is untouched.
        for (g, engine) in session.groups.iter().zip(engines.iter_mut()) {
            assert!(engine.last_searched_query.is_empty());
            engine.update_search("req-7f3a");
            assert_eq!(g.hits, engine.search_matches, "{}", g.name);
        }
        assert_eq!(
            session.groups[1].hits,
            vec![0, 3],
            "the healthcheck line is excluded"
        );
    }

    #[test]
    fn level_filter_time_window_and_ansi_strip_are_honoured() {
        let dir = tempfile::tempdir().unwrap();
        let mut timed = open(
            dir.path(),
            "timed.log",
            "2026-09-18T14:00:00Z INFO job a\n2026-09-18T14:05:00Z ERROR job b\n    at job.frame\n2026-09-18T14:10:00Z ERROR job c\n",
        );
        let from = crate::timestamp::detect_timestamp(
            "2026-09-18T14:04:00Z",
            crate::timestamp::FormatHint::default(),
        )
        .unwrap()
        .0;
        let to = from + 2 * 60_000;
        timed.set_time_range(Some(from), Some(to));
        assert!(!timed.time_range_pending());
        let mut levels = open(
            dir.path(),
            "levels.log",
            "INFO job low\nERROR job high\nWARN job mid\n",
        );
        levels.set_min_level(crate::log_level::LogLevel::Warn);
        let mut ansi = open(
            dir.path(),
            "ansi.log",
            "\x1b[31mjo\x1b[0mb split by a colour code\nplain job\n",
        );
        ansi.set_ansi_mode(crate::ansi::AnsiMode::Strip);
        let engines = vec![timed, levels, ansi];

        let mut session = FindAllSession::with_limits(2, FIND_ALL_MAX_HITS);
        session.input = "job".into();
        session.start(&engines);
        run_to_end(&mut session, &engines);
        // 14:05 and its stack frame are in the window; 14:00 and 14:10 are not.
        assert_eq!(session.groups[0].hits, vec![1, 2]);
        assert_eq!(session.groups[1].hits, vec![1, 2]);
        assert_eq!(
            session.groups[2].hits,
            vec![0, 1],
            "matched without the codes"
        );
    }

    #[test]
    fn hex_streams_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let mut hex = open(dir.path(), "a.log", "needle\n");
        hex.set_view_mode(ViewMode::Hex);
        let text = open(dir.path(), "b.log", "needle\n");
        let engines = vec![hex, text];
        let mut session = FindAllSession::with_limits(4, 10);
        session.input = "needle".into();
        session.start(&engines);
        run_to_end(&mut session, &engines);
        assert_eq!(session.groups[0].state, FindState::SkippedHex);
        assert!(session.groups[0].hits.is_empty());
        assert_eq!(session.groups[1].hits, vec![0]);
    }

    #[test]
    fn stores_the_cap_and_counts_the_true_total() {
        let dir = tempfile::tempdir().unwrap();
        let text = lines(250, |i| format!("line {i} match"));
        let plain = open(dir.path(), "plain.log", &text);
        // Under a time window the drain caps instead of the worker.
        let stamped = lines(250, |i| {
            format!("2026-09-18T14:00:{:02}Z match {i}", i % 60)
        });
        let mut windowed = open(dir.path(), "windowed.log", &stamped);
        windowed.set_time_range(Some(0), None);
        let engines = vec![plain, windowed];
        let mut session = FindAllSession::with_limits(2, 100);
        session.input = "MATCH".into();
        session.start(&engines);
        run_to_end(&mut session, &engines);
        for g in &session.groups {
            assert_eq!(g.hits.len(), 100, "{}", g.name);
            assert_eq!(g.hits, (0..100).collect::<Vec<_>>());
            assert_eq!(g.total, 250);
            assert!(g.capped());
        }
    }

    #[test]
    fn concurrency_never_exceeds_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        // A few MB each, so the jobs overlap.
        let text = lines(60_000, |i| {
            format!("padding padding padding padding {i} hit")
        });
        let engines: Vec<TailEngine> = (0..6)
            .map(|n| open(dir.path(), &format!("s{n}.log"), &text))
            .collect();
        let mut session = FindAllSession::with_limits(2, 1_000);
        session.input = "hit".into();
        session.start(&engines);
        assert_eq!(session.running_count(), 2, "the free slots start at once");
        assert_eq!(session.queued_count(), 4);
        let peak = run_to_end(&mut session, &engines);
        assert_eq!(peak, 2);
        assert!(session.groups.iter().all(|g| g.total == 60_000));
        assert!(concurrency_limit() >= 1 && concurrency_limit() <= MAX_CONCURRENT_JOBS);
    }

    #[test]
    fn stop_new_query_close_and_closed_streams_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let text = lines(200_000, |i| {
            format!("padding padding padding padding {i} hit")
        });
        let engines: Vec<TailEngine> = (0..3)
            .map(|n| open(dir.path(), &format!("s{n}.log"), &text))
            .collect();
        let mut session = FindAllSession::with_limits(1, 1_000);
        session.input = "hit".into();
        session.start(&engines);
        session.poll(&engines);
        session.stop();
        assert!(!session.is_active());
        assert!(session
            .groups
            .iter()
            .all(|g| g.state == FindState::Stopped && g.job.is_none()));
        // Nothing starts again on later polls.
        session.poll(&engines);
        assert_eq!(session.running_count(), 0);

        // A new query replaces every group and job.
        session.input = "padding".into();
        session.start(&engines);
        assert_eq!(session.running_count(), 1);
        assert_eq!(session.query, "padding");

        // Closing a stream drops its group (and job).
        let running = session
            .groups
            .iter()
            .position(|g| g.state == FindState::Running)
            .unwrap();
        let mut remaining = engines;
        remaining.remove(running);
        session.poll(&remaining);
        assert_eq!(session.groups.len(), 2);
        assert_eq!(
            session.running_count(),
            1,
            "the next queued stream took the slot"
        );

        // Closing the tab forgets everything.
        session.close();
        assert!(session.groups.is_empty() && session.query.is_empty());
    }

    #[test]
    fn a_reloaded_stream_turns_stale_and_does_not_jump() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = open(dir.path(), "gateway.log", "a hit\nb\nc hit\n");
        let mut session = FindAllSession::with_limits(1, 10);
        session.input = "hit".into();
        session.start(std::slice::from_ref(&engine));
        run_to_end(&mut session, std::slice::from_ref(&engine));
        assert_eq!(session.groups[0].hits, vec![0, 2]);
        assert!(session.commit(0, 1));
        assert_eq!(
            session.jump.take(),
            Some((engine.path.clone(), 2)),
            "a fresh result jumps"
        );

        // Appends keep the line numbers: still valid.
        let path = engine.path.clone();
        std::fs::write(&path, "a hit\nb\nc hit\nd\n").unwrap();
        engine.poll_updates();
        session.poll(std::slice::from_ref(&engine));
        assert_eq!(session.groups[0].state, FindState::Done);

        // The writer truncates the file: the rows stay, dimmed, and do not jump.
        std::fs::write(&path, "x\n").unwrap();
        engine.poll_updates();
        session.poll(std::slice::from_ref(&engine));
        assert_eq!(session.groups[0].state, FindState::Stale);
        assert_eq!(session.groups[0].hits, vec![0, 2]);
        assert!(!session.commit(0, 0));
        assert!(session.jump.is_none());

        // Refresh runs the query again over the new content.
        session.refresh(std::slice::from_ref(&engine));
        run_to_end(&mut session, std::slice::from_ref(&engine));
        assert_eq!(session.groups[0].state, FindState::Done);
        assert!(session.groups[0].hits.is_empty());
    }

    #[test]
    fn an_empty_query_only_clears() {
        let dir = tempfile::tempdir().unwrap();
        let engines = vec![open(dir.path(), "a.log", "x\n")];
        let mut session = FindAllSession {
            input: "   ".into(),
            ..Default::default()
        };
        session.start(&engines);
        assert!(session.groups.is_empty() && session.started_at.is_none());
    }
}
