// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::tail_engine::{is_relative_time, TailEngine};
use fasttail::timestamp::{format_millis, parse_user_time};
use std::io::Write;

/// 2026-09-18 10:00:00 on the log's clock.
fn base() -> i64 {
    parse_user_time("2026-09-18 10:00:00", 0).unwrap()
}

const MINUTE: i64 = 60_000;

/// One `INFO` line per minute from 10:00 (`minutes` lines), optionally with the minutes
/// given explicitly (a log that goes back in time).
fn log(minutes: &[i64]) -> (tempfile::TempDir, std::path::PathBuf, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.log");
    let lines: Vec<String> = minutes
        .iter()
        .map(|m| format!("{} INFO tick {m}", format_millis(base() + m * MINUTE)))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.ensure_timestamps();
    (dir, path, engine)
}

fn visible(engine: &TailEngine) -> Vec<usize> {
    (0..engine.visible_line_count())
        .filter_map(|r| engine.get_actual_line_idx(r))
        .collect()
}

#[test]
fn relative_texts_are_recognised() {
    for text in [
        "-15m", "-90s", "-3h", "-2d", "-1w", "-1h30m", "now", " NOW ",
    ] {
        assert!(is_relative_time(text), "{text}");
    }
    for text in ["", "-", "-5x", "- 5m", "14:02", "2026-09-18", "15m"] {
        assert!(!is_relative_time(text), "{text}");
    }
}

#[test]
fn a_live_window_slides_forward_on_an_ordered_log() {
    let minutes: Vec<i64> = (0..=60).collect();
    let (_dir, _path, mut engine) = log(&minutes);
    engine.update_search("INFO");
    let (from_ok, to_ok) = engine.apply_time_range_text("-10m", "");
    assert!(from_ok && to_ok);
    assert!(engine.time_window_live());
    assert_eq!(engine.time_from_text, "-10m");

    // "Now" is 10:30: lines from 10:20 on.
    engine.slide_live_window_at(base() + 30 * MINUTE);
    assert_eq!(visible(&engine), (20..=60).collect::<Vec<_>>());
    assert_eq!(engine.search_total(), 41, "the search follows");

    // Ten minutes later the ten oldest lines leave, nothing else changes.
    engine.slide_live_window_at(base() + 40 * MINUTE);
    assert_eq!(visible(&engine), (30..=60).collect::<Vec<_>>());
    assert_eq!(engine.search_total(), 31);
    assert!(engine.search_matches.iter().all(|&l| l >= 30));
}

#[test]
fn a_relative_to_side_lets_later_lines_in_as_it_moves() {
    let minutes: Vec<i64> = (0..=60).collect();
    let (_dir, _path, mut engine) = log(&minutes);
    engine.apply_time_range_text("-60m", "-30m");
    engine.slide_live_window_at(base() + 60 * MINUTE);
    assert_eq!(visible(&engine), (0..=30).collect::<Vec<_>>());
    // The "to" side is the exact instant, not widened to the end of a unit.
    engine.slide_live_window_at(base() + 70 * MINUTE);
    assert_eq!(visible(&engine), (10..=40).collect::<Vec<_>>());
}

#[test]
fn appended_lines_inside_a_live_window_appear() {
    let minutes: Vec<i64> = (0..=30).collect();
    let (_dir, path, mut engine) = log(&minutes);
    engine.apply_time_range_text("-5m", "");
    engine.slide_live_window_at(base() + 30 * MINUTE);
    assert_eq!(visible(&engine), (25..=30).collect::<Vec<_>>());
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{} INFO tick 31", format_millis(base() + 31 * MINUTE)).unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(visible(&engine), (25..=31).collect::<Vec<_>>());
}

#[test]
fn an_unordered_log_refilters_at_most_once_a_minute() {
    // Minute 5 appears after minute 20: the log goes back in time.
    let minutes: Vec<i64> = vec![0, 10, 20, 5, 30, 40];
    let (_dir, _path, mut engine) = log(&minutes);
    assert!(engine.timestamps_unordered());
    engine.apply_time_range_text("-25m", "");
    // First re-read: a full refilter.
    engine.slide_live_window_at(base() + 40 * MINUTE);
    assert_eq!(visible(&engine), vec![2, 4, 5]);
    // A second one within the minute is skipped: the view stays as it was.
    engine.slide_live_window_at(base() + 50 * MINUTE);
    assert_eq!(visible(&engine), vec![2, 4, 5]);
}

#[test]
fn an_absolute_window_is_not_live_and_presets_keep_the_relative_text() {
    let (_dir, _path, mut engine) = log(&[0, 1, 2]);
    engine.apply_time_range_text("10:00", "10:01");
    assert!(!engine.time_window_live());
    assert_eq!(visible(&engine), vec![0, 1]);
    engine.apply_time_range_text("-1h", "now");
    assert!(engine.time_window_live());
    assert_eq!(engine.time_to_text, "now");
}
