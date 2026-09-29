// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::wait_for_jobs;
use fasttail::scan_job::ScanKind;
use fasttail::tail_engine::TailEngine;
use std::io::Write;

/// A banner line, then one entry per second from 10:00:00 with a stack trace under
/// every seventh one and an entry stamped a minute early every 500 lines, so the log
/// has continuation lines, untimed lines and goes back in time.
pub(super) fn write_timed_log(path: &std::path::Path, entries: usize) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(f, "==== service starting, no timestamp here ====").unwrap();
    for i in 0..entries {
        let secs = if i % 500 == 499 { i - 60 } else { i };
        let level = if i % 10 == 0 { "ERROR" } else { "INFO" };
        writeln!(
            f,
            "2026-09-19T{:02}:{:02}:{:02}.000Z {level} svc-{} req={i}",
            10 + secs / 3600,
            (secs / 60) % 60,
            secs % 60,
            i % 7
        )
        .unwrap();
        if i % 7 == 0 {
            writeln!(f, "    at com.example.Handler.run(Handler.java:{i})").unwrap();
        }
    }
}

fn timed_log(entries: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("timed.log");
    write_timed_log(&log, entries);
    (dir, log)
}

fn running(engine: &TailEngine) -> Option<ScanKind> {
    engine.scan_progress().map(|(kind, _, _)| kind)
}

fn assert_same_cache(bg: &TailEngine, sync: &TailEngine) {
    let (values, parsed, unordered, hint) = bg.timestamp_cache();
    let (want_values, want_parsed, want_unordered, want_hint) = sync.timestamp_cache();
    assert_eq!(values.len(), want_values.len(), "every line timed");
    assert!(values == want_values, "same effective timestamps");
    assert_eq!(
        parsed, want_parsed,
        "same count of lines with a time of their own"
    );
    assert_eq!(unordered, want_unordered, "same out-of-order flag");
    assert!(unordered, "the sample log goes back in time");
    assert_eq!(hint, want_hint, "same final format hint");
}

#[test]
fn background_and_synchronous_timing_fill_the_same_cache() {
    let (_dir, log) = timed_log(40_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.ensure_timestamps();

    // Threshold 0: the level scan starts on open, the timestamp request preempts it.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert_eq!(running(&bg), Some(ScanKind::Levels));
    let target = bg.resolve_goto("10:05", 0).expect("a time");
    assert!(target.waiting, "the jump waits for the background timing");
    assert_eq!(running(&bg), Some(ScanKind::Timestamps));
    wait_for_jobs(&mut bg);
    assert!(bg.timestamps_complete());
    assert_same_cache(&bg, &sync);
    assert!(bg.levels_complete(), "the level scan resumed afterwards");
}

#[test]
fn a_timing_scan_preempted_by_a_filter_resumes_from_its_prefix() {
    let (_dir, log) = timed_log(150_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.ensure_timestamps();
    sync.set_include_filter("ERROR");

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert!(bg.resolve_goto("10:30", 0).unwrap().waiting);
    // Let part of the file be timed, then type a filter: with no window involved the
    // filter takes over and the timing waits for it.
    let started = std::time::Instant::now();
    let mut timed = 0;
    while timed == 0 && started.elapsed().as_secs() < 30 {
        bg.poll_updates();
        timed = match bg.scan_progress() {
            Some((ScanKind::Timestamps, _, hits)) => hits,
            _ => break,
        };
    }
    bg.set_include_filter("ERROR");
    if timed > 0 && timed < bg.total_lines() {
        assert_eq!(running(&bg), Some(ScanKind::Filter), "the filter preempts");
        assert!(
            bg.line_timestamp(timed - 1).is_some(),
            "the timed prefix is kept, not thrown away"
        );
    }
    wait_for_jobs(&mut bg);
    assert_same_cache(&bg, &sync);
    assert_eq!(bg.filtered_lines, sync.filtered_lines);
    // The jump that waited lands where the synchronous one does.
    let want = sync.resolve_goto("10:30", 0).unwrap();
    assert_eq!(bg.take_goto_time_result(), Some(Some(want)));
}

#[test]
fn a_window_typed_during_the_scan_is_held_then_applied() {
    let (_dir, log) = timed_log(40_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.apply_time_range_text("10:20", "10:40");
    sync.set_include_filter("svc-3");
    assert!(!sync.time_range_pending());

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut bg); // the level scan
    let (from_ok, to_ok) = bg.apply_time_range_text("10:20", "10:40");
    assert!(from_ok && to_ok);
    assert!(bg.time_range_pending());
    assert!(
        !bg.is_time_filtered(),
        "nothing is hidden before the cache is complete"
    );
    assert_eq!(bg.visible_line_count(), bg.total_lines());
    assert_eq!(running(&bg), Some(ScanKind::Timestamps));
    // A filter typed meanwhile waits for the timing instead of cancelling it.
    bg.set_include_filter("svc-3");
    assert_eq!(running(&bg), Some(ScanKind::Timestamps));
    wait_for_jobs(&mut bg);

    assert!(!bg.time_range_pending());
    assert!(bg.is_time_filtered());
    assert_eq!((bg.time_from, bg.time_to), (sync.time_from, sync.time_to));
    assert_eq!(bg.filtered_lines, sync.filtered_lines);
    assert!(!bg.filtered_lines.is_empty());
}

#[test]
fn editing_or_clearing_a_held_window() {
    let (_dir, log) = timed_log(40_000);
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.apply_time_range_text("10:20", "");
    assert!(bg.time_range_pending());
    // Editing replaces the held window; clearing drops it, the timing goes on.
    bg.apply_time_range_text("10:25", "");
    assert!(bg.time_range_pending());
    assert_eq!(bg.time_from_text, "10:25");
    bg.apply_time_range_text("", "");
    assert!(!bg.time_range_pending());
    assert_eq!(running(&bg), Some(ScanKind::Timestamps));
    wait_for_jobs(&mut bg);
    assert!(bg.timestamps_complete());
    assert!(!bg.is_time_filtered());
    assert_eq!(bg.visible_line_count(), bg.total_lines());

    // An unreadable side is flagged at once, while the window is held.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert_eq!(bg.apply_time_range_text("10:20", "soon"), (true, false));
    bg.clear_time_range();
    assert!(!bg.time_range_pending());
}

#[test]
fn go_to_time_waits_for_the_scan_and_lands_like_the_synchronous_path() {
    let (_dir, log) = timed_log(40_000);
    let mut sync = TailEngine::open(&log).unwrap();
    let want = sync.resolve_goto("10:12:30", 0).expect("synchronous jump");
    assert!(!want.waiting);

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.follow_tail = true;
    assert!(bg.resolve_goto("10:12:30", 0).unwrap().waiting);
    assert!(bg.goto_time_waiting());
    wait_for_jobs(&mut bg);
    assert!(!bg.goto_time_waiting());
    assert_eq!(bg.take_goto_time_result(), Some(Some(want)));
    assert_eq!(bg.take_goto_time_result(), None, "handed over once");
    assert_eq!(bg.scroll_to_line, Some(want.line));
    assert!(!bg.follow_tail, "a jump pauses follow");

    // A time past the end resolves to nothing once the scan is done.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert!(bg.resolve_goto("23:59", 0).unwrap().waiting);
    wait_for_jobs(&mut bg);
    assert_eq!(bg.take_goto_time_result(), Some(None));
}

#[test]
fn a_cancelled_time_jump_does_not_move_the_view_but_the_timing_goes_on() {
    let (_dir, log) = timed_log(40_000);
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert!(bg.resolve_goto("10:12:30", 0).unwrap().waiting);
    bg.cancel_goto_time();
    assert!(!bg.goto_time_waiting());
    assert_eq!(running(&bg), Some(ScanKind::Timestamps));
    wait_for_jobs(&mut bg);
    assert_eq!(bg.take_goto_time_result(), None);
    assert_eq!(bg.scroll_to_line, None);
    assert!(bg.timestamps_complete());

    // Entering another target drops the waiting jump too.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert!(bg.resolve_goto("10:12:30", 0).unwrap().waiting);
    assert_eq!(bg.resolve_goto("5", 0).unwrap().line, 4);
    assert!(!bg.goto_time_waiting());
}

#[test]
fn filter_and_search_jobs_respect_the_window_above_the_threshold() {
    let (_dir, log) = timed_log(40_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.apply_time_range_text("10:20", "10:40");
    let window_only = sync.filtered_lines.clone();
    sync.set_include_filter("ERROR");
    sync.update_search("svc-5");
    assert!(!sync.filtered_lines.is_empty() && !sync.search_matches.is_empty());

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.apply_time_range_text("10:20", "10:40");
    wait_for_jobs(&mut bg);
    assert!(bg.is_time_filtered());
    assert_eq!(bg.filtered_lines, window_only, "a window alone");

    bg.set_include_filter("ERROR");
    assert_eq!(
        running(&bg),
        Some(ScanKind::Filter),
        "the window no longer forces the filter onto the interface thread"
    );
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, sync.filtered_lines);

    bg.update_search("svc-5");
    assert_eq!(running(&bg), Some(ScanKind::Search));
    wait_for_jobs(&mut bg);
    assert_eq!(bg.search_matches, sync.search_matches);
    assert!(bg.search_matches.iter().all(|&idx| bg.in_time_range(idx)));
}

#[test]
fn a_rewrite_during_the_scan_times_the_new_content() {
    let (_dir, log) = timed_log(150_000);
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.size_check_interval = std::time::Duration::ZERO;
    bg.apply_time_range_text("10:00:30", "");
    assert_eq!(running(&bg), Some(ScanKind::Timestamps));

    // The file is truncated and rewritten with other times while the scan runs.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_timed_log(&log, 3_000);
    wait_for_jobs(&mut bg);
    bg.poll_updates();
    wait_for_jobs(&mut bg);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.apply_time_range_text("10:00:30", "");
    assert_eq!(bg.total_lines(), sync.total_lines());
    assert_same_cache(&bg, &sync);
    assert!(!bg.time_range_pending());
    assert_eq!(bg.filtered_lines, sync.filtered_lines);
}

#[test]
fn background_scans_of_a_pattern_stream_read_the_matched_file() {
    // `path` is the pattern itself: the worker must open the file it resolved to.
    let dir = tempfile::tempdir().unwrap();
    write_timed_log(&dir.path().join("app-1.log"), 5_000);
    let pattern = dir.path().join("app-*.log");
    let mut bg = TailEngine::open_pattern_with_thresholds(&pattern, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut bg);
    bg.apply_time_range_text("10:10", "");
    assert!(bg.time_range_pending());
    wait_for_jobs(&mut bg);
    assert!(bg.timestamps_complete());
    assert!(bg.is_time_filtered());

    let mut sync = TailEngine::open(dir.path().join("app-1.log")).unwrap();
    sync.apply_time_range_text("10:10", "");
    assert_eq!(bg.filtered_lines, sync.filtered_lines);
    assert!(!bg.filtered_lines.is_empty());
}

#[test]
fn lines_appended_under_a_window_are_timed_before_they_are_filtered() {
    let (_dir, log) = timed_log(100);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    engine.apply_time_range_text("10:00:30", "");
    let before = engine.filtered_lines.len();
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    writeln!(f, "2026-09-19T11:00:00.000Z INFO appended").unwrap();
    drop(f);
    engine.poll_updates();
    let last = engine.total_lines() - 1;
    assert_eq!(engine.filtered_lines.len(), before + 1);
    assert_eq!(engine.filtered_lines.last(), Some(&last));
}
