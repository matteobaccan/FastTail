// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::tail_engine::TailEngine;
use std::io::Write;
use std::time::{Duration, Instant};

fn open(lines: &[String], threshold: u64) -> (tempfile::TempDir, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    engine.job_threshold_bytes = threshold;
    super::wait_for_jobs(&mut engine);
    (dir, engine)
}

/// Polls until the patterns are learnt and counted up to the last line.
fn wait_patterns(engine: &mut TailEngine) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.poll_updates();
        let p = engine.patterns.as_ref().unwrap();
        if !p.running() && p.snapshot.is_some() {
            super::wait_for_jobs(engine);
            engine.poll_updates();
            let p = engine.patterns.as_ref().unwrap();
            if !p.running() {
                return;
            }
        }
        assert!(Instant::now() < deadline, "patterns did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Five kinds of entries, one of them with a stack trace.
fn app_log() -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..300 {
        let ts = format!("2026-10-24 10:{:02}:{:02}", i / 60 % 60, i % 60);
        match i % 5 {
            0 => lines.push(format!("{ts} INFO GET /api/users/{i} 200 {}ms", i % 97)),
            1 => lines.push(format!("{ts} INFO job {i} finished in {} s", i % 13)),
            2 => lines.push(format!("{ts} WARN user u{i} logged out")),
            3 => {
                lines.push(format!("{ts} ERROR payment {i} failed: gateway timeout"));
                lines.push("    at com.example.Pay.charge(Pay.java:42)".to_string());
                lines.push("    at com.example.Main.run(Main.java:7)".to_string());
            }
            _ => lines.push(format!("{ts} DEBUG worker-{} heartbeat", i % 7)),
        }
    }
    lines
}

/// The id of the pattern whose template contains `text`.
fn pattern_of(engine: &TailEngine, text: &str) -> u32 {
    let p = engine.patterns.as_ref().unwrap();
    let drain = p.tree();
    p.counts
        .ids()
        .find(|&i| drain.template(i).unwrap().text().contains(text))
        .unwrap_or_else(|| panic!("no pattern with {text}"))
}

#[test]
fn filtering_to_a_pattern_shows_exactly_its_counted_entries() {
    for threshold in [u64::MAX, 0] {
        let lines = app_log();
        let (_dir, mut engine) = open(&lines, threshold);
        engine.open_patterns();
        wait_patterns(&mut engine);
        let p = engine.patterns.as_ref().unwrap();
        // Five patterns over 300 entries; the stack trace lines are not entries.
        assert_eq!(p.counts.entries, 300, "threshold {threshold}");
        assert_eq!(p.counts.ids().count(), 5, "threshold {threshold}");
        let pay = pattern_of(&engine, "payment <*> failed: gateway timeout");
        let p = engine.patterns.as_ref().unwrap();
        assert_eq!(p.counts.per[pay as usize].count, 60);
        // Each entry's line falls in the sparkline over the stream's time span.
        let spark: u32 = p.counts.per[pay as usize].buckets.iter().sum();
        assert_eq!(spark, 60);
        // Filtered to the pattern: its 60 entries and their two-line traces.
        engine.set_pattern_filter(vec![pay], Vec::new());
        super::wait_for_jobs(&mut engine);
        assert_eq!(engine.visible_line_count(), 180, "threshold {threshold}");
        assert!(engine.patterns.as_ref().unwrap().snapshot.is_some());
        assert!(!engine.pattern_filter_dropped);
        // Hidden instead: everything else.
        engine.set_pattern_filter(Vec::new(), vec![pay]);
        super::wait_for_jobs(&mut engine);
        assert_eq!(engine.visible_line_count(), lines.len() - 180);
        // Two patterns at once (OR), together with a text filter (AND).
        let users = pattern_of(&engine, "GET /api/users/<*>");
        engine.set_pattern_filter(vec![pay, users], Vec::new());
        super::wait_for_jobs(&mut engine);
        assert_eq!(engine.visible_line_count(), 240);
        engine.set_pattern_filter(Vec::new(), Vec::new());
        super::wait_for_jobs(&mut engine);
        assert_eq!(engine.visible_line_count(), lines.len());
    }
}

#[test]
fn another_filter_relearns_and_clears_the_pattern_filter() {
    let lines = app_log();
    let (_dir, mut engine) = open(&lines, u64::MAX);
    engine.open_patterns();
    wait_patterns(&mut engine);
    let pay = pattern_of(&engine, "payment");
    engine.set_pattern_filter(vec![pay], Vec::new());
    super::wait_for_jobs(&mut engine);
    // The pattern filter does not relearn.
    assert!(!engine.patterns.as_ref().unwrap().running());
    engine.set_exclude_filter("heartbeat");
    super::wait_for_jobs(&mut engine);
    assert!(engine.pattern_filter().is_none());
    assert!(engine.pattern_filter_dropped);
    wait_patterns(&mut engine);
    let p = engine.patterns.as_ref().unwrap();
    // Learnt over what the exclude term leaves.
    assert_eq!(p.counts.entries, 240);
    assert_eq!(p.counts.ids().count(), 4);
}

#[test]
fn appended_entries_are_learnt_and_truncation_relearns() {
    let lines = app_log();
    let (dir, mut engine) = open(&lines, u64::MAX);
    engine.open_patterns();
    wait_patterns(&mut engine);
    let path = dir.path().join("app.log");
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        for i in 0..10 {
            writeln!(f, "2026-10-24 11:00:{i:02} INFO cache flushed {i} keys").unwrap();
        }
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.poll_updates();
        let p = engine.patterns.as_ref().unwrap();
        if p.counts.entries == 310 {
            break;
        }
        assert!(Instant::now() < deadline, "appended entries not counted");
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = pattern_of(&engine, "cache flushed <*> keys");
    // Truncated and rewritten: learnt again from scratch.
    write_lines(
        &path,
        &[
            "2026-10-24 12:00:00 INFO restarted",
            "2026-10-24 12:00:01 INFO restarted",
        ],
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.poll_updates();
        let p = engine.patterns.as_ref().unwrap();
        if !p.running() && p.snapshot.is_some() && p.counts.entries == 2 {
            break;
        }
        assert!(Instant::now() < deadline, "truncation did not relearn");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(engine.patterns.as_ref().unwrap().counts.ids().count(), 1);
}
