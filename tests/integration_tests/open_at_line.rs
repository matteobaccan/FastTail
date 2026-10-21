// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;
use fasttail::i18n::Language;

fn numbered(path: &std::path::Path, lines: usize) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 1..=lines {
        let level = if i % 10 == 0 { "ERROR" } else { "INFO" };
        writeln!(f, "2026-10-21 10:00:00 {level} event {i}").unwrap();
    }
}

#[test]
fn a_small_file_opens_at_the_line_selected_and_follow_paused() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 5000);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.follow_tail = true;
    engine.open_at_line(1204, Language::En);
    assert!(!engine.follow_tail);
    assert_eq!(engine.pending_jump, Some(1203));
    assert!(engine.selection.contains(&1203));
    assert!(engine.view_notice.is_none());
    assert!(!engine.open_line_pending());
}

#[test]
fn a_line_past_the_end_lands_on_the_last_line_with_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 8123);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.open_at_line(9000, Language::En);
    assert_eq!(engine.pending_jump, Some(8122));
    assert_eq!(
        engine.view_notice.as_deref(),
        Some("Line 9000 is past the end (8123 lines)")
    );
}

#[test]
fn a_line_the_filter_hides_lands_on_the_next_shown_line() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 100);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_include_filter("ERROR");
    engine.open_at_line(15, Language::En);
    // ERROR lines are 10, 20, ...: line 15 is hidden, line 20 shows.
    assert_eq!(engine.pending_jump, Some(19));
    assert!(engine.view_notice.as_deref().unwrap().contains("hidden"));
    assert_eq!(engine.include_filter(), "ERROR", "the filter stays");
}

#[test]
fn a_large_file_waits_for_the_index_to_reach_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("big.log");
    numbered(&log, 200_000);
    // Threshold 0: the index is built in the background.
    let mut engine = TailEngine::open_with_thresholds(&log, 0, 0).unwrap();
    assert!(engine.index_pending);
    engine.open_at_line(150_000, Language::En);
    let start = std::time::Instant::now();
    while engine.open_line_pending() {
        engine.poll_updates();
        assert!(
            start.elapsed().as_secs() < 30,
            "the index never reached the line"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(engine.pending_jump, Some(149_999));
    assert!(engine.view_notice.is_none());
}
