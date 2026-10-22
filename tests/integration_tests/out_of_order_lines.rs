// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::config::FastTailConfig;
use fasttail::tail_engine::TailEngine;
use std::io::Write;

const LOG: [&str; 7] = [
    "2026-09-28 14:00:00 INFO start",
    "2026-09-28 14:10:00 INFO ten minutes later",
    "2026-09-28 14:04:48 WARN the clock went back 5:12",
    "    at Clock.sync(Clock.java:1)",
    "2026-09-28 14:04:48.500 INFO half a second later",
    "2026-09-28 14:04:48.200 INFO 300 ms back: within the tolerance",
    "2026-09-28 14:05:00 INFO on",
];

fn timed(path: &std::path::Path, thresholds: Option<u64>) -> TailEngine {
    let mut e = match thresholds {
        None => TailEngine::open(path).unwrap(),
        Some(t) => TailEngine::open_with_thresholds(path, t, t).unwrap(),
    };
    e.size_check_interval = std::time::Duration::ZERO;
    wait_for_jobs(&mut e);
    e.ensure_timestamps();
    wait_for_jobs(&mut e);
    while !e.timestamps_complete() {
        e.fill_timestamps();
        wait_for_jobs(&mut e);
    }
    e
}

#[test]
fn a_clock_jumping_back_is_marked_counted_and_walked() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    for thresholds in [None, Some(0)] {
        let mut e = timed(&log, thresholds);
        assert_eq!(e.out_of_order_lines(), &[(2, -312_000)], "{thresholds:?}");
        assert_eq!(e.out_of_order_count(), 1);
        assert!(!e.timestamps_go_backwards());
        assert_eq!(
            e.out_of_order_jump(3),
            None,
            "the trace line carries the entry's time"
        );
        assert_eq!(fasttail::timestamp::format_jump(-312_000), "-00:05:12");
        assert_eq!(fasttail::timestamp::format_jump(-300), "-00:00:00.300");
        // The walk selects it, both ways, and wraps.
        assert_eq!(e.walk_out_of_order(true), Some(2));
        assert_eq!(e.walk_out_of_order(true), Some(2));
        assert_eq!(e.walk_out_of_order(false), Some(2));
        // A tighter tolerance marks the 300 ms step too; 0 turns the marks off.
        e.set_out_of_order_tolerance(100);
        assert_eq!(e.out_of_order_count(), 2);
        e.set_out_of_order_tolerance(0);
        assert_eq!(e.out_of_order_count(), 0);
        e.set_out_of_order_tolerance(1000);

        // An appended jump is found as it is timed.
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(f, "2026-09-28 13:00:00 ERROR an hour back").unwrap();
        drop(f);
        e.poll_updates();
        wait_for_jobs(&mut e);
        while !e.timestamps_complete() {
            e.fill_timestamps();
            wait_for_jobs(&mut e);
        }
        assert_eq!(e.out_of_order_jump(7), Some(-3_900_000), "{thresholds:?}");
        // Reset for the next pass.
        write_lines(&log, &LOG);
    }
}

#[test]
fn a_log_written_in_reverse_says_so_once() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("rev.log");
    let lines: Vec<String> = (0..30)
        .rev()
        .map(|i| format!("2026-09-28 14:{i:02}:00 INFO step {i}"))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&log, &refs);
    let e = timed(&log, None);
    assert_eq!(e.out_of_order_count(), 29);
    assert!(e.timestamps_go_backwards());
}

#[test]
fn the_tolerance_is_a_setting() {
    let config = FastTailConfig {
        out_of_order_tolerance_ms: 2500,
        ..Default::default()
    };
    let back = FastTailConfig::from_ini(&config.to_ini());
    assert_eq!(back.out_of_order_tolerance_ms, 2500);
    assert_eq!(FastTailConfig::default().out_of_order_tolerance_ms, 1000);
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    let mut e = timed(&log, None);
    fasttail::workspace::apply_settings(
        &mut e,
        &FastTailConfig {
            out_of_order_tolerance_ms: 100,
            ..Default::default()
        },
    );
    assert_eq!(e.out_of_order_count(), 2);
}
