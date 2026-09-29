// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::tail_engine::{is_relative_time, TailEngine};
use fasttail::timestamp::{format_millis, local_now_millis, parse_user_time};
use std::io::Write;

const MINUTE: i64 = 60_000;

/// A log written up to about now: line `m` is stamped `now - (last - m) minutes - 30 s`,
/// so every bound counted in whole minutes from now falls between two lines.
struct Live {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    engine: TailEngine,
    /// "Now" when the log was written (the engine reads its own clock when a window is
    /// applied, a few milliseconds later).
    now: i64,
    base: i64,
}

fn live_log(minutes: &[i64]) -> Live {
    let now = local_now_millis();
    let last = *minutes.iter().max().unwrap();
    let base = now - last * MINUTE - 30_000;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.log");
    let lines: Vec<String> = minutes
        .iter()
        .map(|m| format!("{} INFO tick {m}", stamp(base + m * MINUTE)))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.ensure_timestamps();
    Live {
        _dir: dir,
        path,
        engine,
        now,
        base,
    }
}

/// A timestamp with its milliseconds, as a log line writes it.
fn stamp(millis: i64) -> String {
    format!("{}.{:03}", format_millis(millis), millis.rem_euclid(1000))
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
    let mut live = live_log(&minutes);
    let engine = &mut live.engine;
    engine.update_search("INFO");
    let (from_ok, to_ok) = engine.apply_time_range_text("-10m", "");
    assert!(from_ok && to_ok);
    assert!(engine.time_window_live());
    assert!(engine.time_window_slides_in_place());
    assert_eq!(visible(engine), (51..=60).collect::<Vec<_>>());
    assert_eq!(engine.search_total(), 10);

    // Five minutes later the five oldest lines leave, in place.
    engine.slide_live_window_at(live.now + 5 * MINUTE);
    assert_eq!(visible(engine), (56..=60).collect::<Vec<_>>());
    assert_eq!(engine.search_total(), 5, "the search follows");
    assert!(engine.search_matches.iter().all(|&l| l >= 56));
}

#[test]
fn a_relative_to_side_lets_later_lines_in_as_it_moves() {
    let minutes: Vec<i64> = (0..=60).collect();
    let mut live = live_log(&minutes);
    let engine = &mut live.engine;
    engine.apply_time_range_text("-60m", "-30m");
    assert_eq!(visible(engine), (1..=30).collect::<Vec<_>>());
    // The "to" side is the exact instant, not widened to the end of a unit.
    engine.slide_live_window_at(live.now + 10 * MINUTE);
    assert_eq!(visible(engine), (11..=40).collect::<Vec<_>>());
}

#[test]
fn appended_lines_inside_a_live_window_appear() {
    let minutes: Vec<i64> = (0..=30).collect();
    let mut live = live_log(&minutes);
    live.engine.apply_time_range_text("-5m", "");
    assert_eq!(visible(&live.engine), (26..=30).collect::<Vec<_>>());
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&live.path)
        .unwrap();
    writeln!(f, "{} INFO tick 31", stamp(live.base + 31 * MINUTE)).unwrap();
    drop(f);
    live.engine.poll_updates();
    assert_eq!(visible(&live.engine), (26..=31).collect::<Vec<_>>());
}

#[test]
fn an_unordered_log_refilters_at_most_once_a_minute() {
    // Minute 5 is written after minute 20: the log goes back in time.
    let mut live = live_log(&[0, 10, 20, 5, 30, 40]);
    let engine = &mut live.engine;
    assert!(engine.timestamps_unordered());
    assert!(!engine.time_window_slides_in_place());
    engine.apply_time_range_text("-25m", "");
    assert_eq!(visible(engine), vec![2, 4, 5]);
    // Within the minute that follows the window being applied, it does not move.
    engine.slide_live_window_at(live.now + 10 * MINUTE);
    assert_eq!(visible(engine), vec![2, 4, 5]);
}

#[test]
fn an_absolute_window_is_not_live_and_the_relative_text_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixed.log");
    let base = parse_user_time("2026-09-18 10:00:00", 0).unwrap();
    let lines: Vec<String> = (0..3)
        .map(|m| format!("{} INFO tick {m}", format_millis(base + m * MINUTE)))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.ensure_timestamps();
    engine.apply_time_range_text("10:00", "10:01");
    assert!(!engine.time_window_live());
    assert_eq!(visible(&engine), vec![0, 1]);
    engine.apply_time_range_text("-1h", "now");
    assert!(engine.time_window_live());
    assert_eq!(engine.time_to_text, "now");
}

#[test]
fn a_search_time_scope_refuses_relative_times() {
    use fasttail::tail_engine::SearchScope;
    let mut live = live_log(&[0, 1, 2]);
    let scope = SearchScope::Time {
        from: "-15m".into(),
        to: String::new(),
    };
    assert_eq!(live.engine.set_search_scope(scope), (false, true));
}
