// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::background_timestamps::write_timed_log;
use super::wait_for_jobs;
use fasttail::log_level::LogLevel;
use fasttail::scan_job::ScanKind;
use fasttail::tail_engine::TailEngine;
use fasttail::time_histogram::TimeHistogram;
use std::io::Write;

/// The histogram rebuilt from the engine's caches at the width the incremental one
/// reached: the two must be equal after every event.
fn assert_matches_caches(engine: &TailEngine) {
    let incremental = engine.time_histogram();
    let (stamps, _, _, _) = engine.timestamp_cache();
    let levels = engine.cached_levels();
    let lines = stamps.len().min(levels.len());
    assert_eq!(
        engine.histogram_lines(),
        lines,
        "every line with both caches"
    );
    let mut rebuilt = TimeHistogram::with_bucket_ms(incremental.bucket_ms());
    for idx in 0..lines {
        rebuilt.add(stamps[idx], levels[idx]);
    }
    assert_eq!(incremental, &rebuilt);
}

fn append(path: &std::path::Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

fn timed_log(entries: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("timeline.log");
    write_timed_log(&log, entries);
    (dir, log)
}

#[test]
fn built_only_once_the_stream_is_timed_and_counts_every_line() {
    let (_dir, log) = timed_log(3_000);
    let mut engine = TailEngine::open(&log).unwrap();
    assert!(
        engine.time_histogram().is_empty(),
        "nothing timed, nothing built"
    );
    engine.set_include_filter("svc-3");
    engine.request_timeline();
    assert!(engine.timestamps_complete());
    assert_matches_caches(&engine);
    let histogram = engine.time_histogram();
    // Filters do not hide the histogram's data; the banner is the only untimed line.
    assert_eq!(histogram.untimed(), 1);
    assert_eq!(histogram.timed() as usize, engine.total_lines() - 1);
    let errors: u64 = (0..histogram.len())
        .map(|i| u64::from(histogram.bucket(i).unwrap()[LogLevel::Error as usize]))
        .sum();
    assert_eq!(errors, 300);
    // 3,000 s fit in 2,048 buckets of 2 s.
    assert_eq!(histogram.bucket_ms(), 2_000);
}

#[test]
fn appends_and_a_completed_partial_line_are_counted_once() {
    let (_dir, log) = timed_log(1_000);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    engine.request_timeline();
    let before = engine.time_histogram().timed();
    append(
        &log,
        "2026-09-19T11:00:00.000Z ERROR appended\n2026-09-19T11:00:01.000Z IN",
    );
    engine.poll_updates();
    assert_matches_caches(&engine);
    // The partial last line is re-evaluated with the rest of it: removed, added again.
    append(&log, "FO done\n");
    engine.poll_updates();
    assert_matches_caches(&engine);
    let histogram = engine.time_histogram();
    assert_eq!(histogram.timed(), before + 2);
    let last = histogram.bucket(histogram.len() - 1).unwrap();
    assert_eq!(
        last[LogLevel::Info as usize],
        1,
        "the completed line is INFO"
    );
}

#[test]
fn a_rewrite_empties_the_histogram_then_shows_the_new_lines() {
    let (_dir, log) = timed_log(5_000);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    engine.request_timeline();
    assert!(engine.time_histogram().bucket_ms() > 1_000);
    std::fs::write(&log, "").unwrap();
    engine.poll_updates();
    assert!(engine.time_histogram().is_empty());
    assert_eq!(engine.time_histogram().untimed(), 0);
    append(
        &log,
        "2026-09-20 08:00:00 WARN fresh\n2026-09-20 08:00:05 INFO fresh\n",
    );
    engine.poll_updates();
    // The strip asks again every frame: a rewritten stream is timed anew.
    engine.request_timeline();
    assert_matches_caches(&engine);
    let histogram = engine.time_histogram();
    assert_eq!(
        histogram.bucket_ms(),
        1_000,
        "a full reset starts over at 1 s"
    );
    assert_eq!(histogram.timed(), 2);
    assert_eq!(histogram.len(), 6);
}

#[test]
fn a_background_timing_matches_the_synchronous_histogram() {
    let (_dir, log) = timed_log(40_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.request_timeline();

    // The level scan starts on open; the timeline's timing preempts it, finishes first,
    // and the levels resume: the histogram fills as the second cache catches up.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert_eq!(bg.scan_progress().map(|p| p.0), Some(ScanKind::Levels));
    bg.request_timeline();
    assert_eq!(bg.scan_progress().map(|p| p.0), Some(ScanKind::Timestamps));
    wait_for_jobs(&mut bg);
    assert!(bg.timestamps_complete() && bg.levels_complete());
    assert_matches_caches(&bg);
    assert_eq!(bg.time_histogram(), sync.time_histogram());
}

#[test]
fn the_timing_scan_brings_the_levels_along() {
    let (_dir, log) = timed_log(150_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.request_timeline();

    // The timeline preempts the level scan on open: the timing scan detects the
    // levels too, so the bars grow with it and no second pass is needed.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.request_timeline();
    assert_eq!(bg.scan_progress().map(|p| p.0), Some(ScanKind::Timestamps));
    let started = std::time::Instant::now();
    while bg.scan_progress().map(|p| p.0) == Some(ScanKind::Timestamps) {
        bg.poll_updates();
        assert_eq!(
            bg.histogram_lines(),
            bg.timestamp_cache().0.len(),
            "every timed line is already in the bars"
        );
        assert!(started.elapsed().as_secs() < 30, "timing hangs");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(bg.timestamps_complete() && bg.levels_complete());
    assert!(bg.scan_progress().is_none(), "no level scan left to run");
    assert_matches_caches(&bg);
    assert_eq!(bg.time_histogram(), sync.time_histogram());
}

#[test]
fn a_timing_scan_preempted_and_resumed_matches_too() {
    let (_dir, log) = timed_log(150_000);
    let mut sync = TailEngine::open(&log).unwrap();
    sync.request_timeline();

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut bg); // the level scan
    bg.request_timeline();
    let started = std::time::Instant::now();
    while bg.histogram_lines() == 0 && started.elapsed().as_secs() < 30 {
        bg.poll_updates();
        if bg.scan_progress().is_none() {
            break;
        }
    }
    // A filter takes over; the timing resumes from its prefix afterwards.
    bg.set_include_filter("ERROR");
    assert_matches_caches(&bg);
    wait_for_jobs(&mut bg);
    assert!(bg.timestamps_complete());
    assert_matches_caches(&bg);
    assert_eq!(bg.time_histogram(), sync.time_histogram());
}
