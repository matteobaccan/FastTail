// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;
use fasttail::tail_engine::merge_ranges;

fn numbered(path: &std::path::Path, lines: usize) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 1..=lines {
        let level = if i % 10 == 0 { "ERROR" } else { "INFO" };
        writeln!(f, "2026-10-21 10:00:00 {level} retry event {i}").unwrap();
    }
}

fn wait(engine: &mut TailEngine) {
    let start = std::time::Instant::now();
    while engine.scan_progress().is_some() || engine.index_pending {
        engine.poll_updates();
        assert!(start.elapsed().as_secs() < 30);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn ranges_merge_and_are_capped() {
    assert_eq!(
        merge_ranges(vec![(5, 7), (1, 2), (3, 4), (10, 10)]),
        vec![(1, 7), (10, 10)]
    );
    assert_eq!(merge_ranges(vec![(1, 5), (2, 3)]), vec![(1, 5)]);
    let many: Vec<_> = (0..1500).map(|i| (i * 10, i * 10)).collect();
    assert_eq!(
        merge_ranges(many).len(),
        fasttail::tail_engine::MAX_HIDDEN_RANGES
    );
    use fasttail::session::{hidden_from_ini, hidden_to_ini};
    assert_eq!(hidden_to_ini(&[(119, 179), (4095, 4095)]), "120-180,4096");
    assert_eq!(
        hidden_from_ini("120-180, 4096,x,0,9-3"),
        vec![(119, 179), (4095, 4095)]
    );
}

#[test]
fn hidden_lines_leave_the_view_search_and_come_back() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 300);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.update_search("retry");
    assert_eq!(engine.active_match_count(), 300);
    // Lines 120 to 180 (0-based 119..=179).
    engine.hide_lines(119..=179);
    assert_eq!(engine.hidden_count(), 61);
    assert_eq!(engine.visible_line_count(), 239);
    assert!(!engine.is_line_visible(150));
    assert!(engine.is_line_visible(118) && engine.is_line_visible(180));
    assert_eq!(
        engine.active_match_count(),
        239,
        "no hit in the hidden lines"
    );
    assert!(engine.hidden_dirty);
    // Combined with a filter: the ERROR lines minus the hidden ones.
    engine.set_include_filter("ERROR");
    // ERROR lines 120, 130, ..., 180 are hidden: 7 of the 30.
    assert_eq!(engine.visible_line_count(), 30 - 7);
    engine.set_include_filter("");
    // Show the range again.
    engine.show_hidden_range(0);
    assert_eq!(engine.hidden_count(), 0);
    assert_eq!(engine.visible_line_count(), 300);
    assert!(!engine.is_filter_active());
}

#[test]
fn appends_keep_them_and_a_rewrite_clears_them() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 100);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.hide_lines([4, 5, 6]);
    {
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(f, "2026-10-21 10:00:01 INFO appended").unwrap();
    }
    let start = std::time::Instant::now();
    while engine.total_lines() < 101 {
        engine.poll_updates();
        assert!(start.elapsed().as_secs() < 10);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(engine.hidden_ranges(), &[(4, 6)]);
    assert_eq!(engine.visible_line_count(), 98);
    // Truncated and rewritten: the numbers no longer name the same lines.
    numbered(&log, 50);
    let start = std::time::Instant::now();
    while !engine.hidden_cleared {
        engine.poll_updates();
        assert!(start.elapsed().as_secs() < 10, "the rewrite was not seen");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(engine.hidden_ranges().is_empty());
    assert_eq!(engine.visible_line_count(), 50);
}

#[test]
fn a_large_file_filters_them_in_the_background_too() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("big.log");
    numbered(&log, 50_000);
    let mut engine = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    wait(&mut engine);
    engine.hide_lines(0..10_000);
    wait(&mut engine);
    assert_eq!(engine.visible_line_count(), 40_000);
    engine.update_search("retry");
    wait(&mut engine);
    assert_eq!(engine.active_match_count(), 40_000);
    engine.set_include_filter("ERROR");
    wait(&mut engine);
    assert_eq!(engine.visible_line_count(), 4_000);
}

#[test]
fn hidden_lines_are_saved_with_the_stream() {
    use fasttail::workspace::{restore_stream, save_changes};
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 300);
    let mut cfg = FastTailConfig::default();
    cfg.open_files = vec![log.clone()];
    let mut engine = TailEngine::open(&log).unwrap();
    engine.hide_lines(119..=179);
    assert!(save_changes(&mut engine, &mut cfg, false));
    assert_eq!(cfg.stream_state_for(&log).unwrap().hidden, vec![(119, 179)]);
    let file = dir.path().join("fasttail.ini");
    cfg.save_to(&file).unwrap();
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("hidden=120-180"));
    let back = FastTailConfig::from_ini(&ini::Ini::load_from_file(&file).unwrap());
    let mut again = TailEngine::open(&log).unwrap();
    restore_stream(&mut again, &back, &log);
    assert_eq!(again.hidden_ranges(), &[(119, 179)]);
    assert_eq!(again.visible_line_count(), 239);
    assert!(!again.hidden_dirty);
}

#[test]
fn context_lines_never_bring_a_hidden_line_back() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 300);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_context_lines(3);
    // No text filter: line 150 alone.
    engine.hide_lines([149]);
    assert_eq!(engine.visible_line_count(), 299);
    assert!((0..299).all(|r| engine.get_actual_line_idx(r) != Some(149)));
    // A filter with context: the line right after an ERROR line stays hidden.
    engine.set_include_filter("ERROR");
    engine.hide_lines([20]);
    let shown: Vec<usize> = (0..engine.visible_line_count())
        .filter_map(|r| engine.get_actual_line_idx(r))
        .collect();
    assert!(shown.contains(&19) && shown.contains(&21), "{shown:?}");
    assert!(!shown.contains(&20) && !shown.contains(&149), "{shown:?}");
    engine.update_search("retry");
    assert_eq!(engine.active_match_count(), shown.len());
}

#[test]
fn select_all_and_collapsed_rows_hide_what_is_meant() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 100);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_include_filter("ERROR");
    engine.select_all_visible();
    assert_eq!(engine.hide_selection_or_row(None), 10);
    assert!(!engine.has_selection());
    engine.set_include_filter("");
    assert_eq!(engine.visible_line_count(), 90);
    // The clicked row outside the selection: that row only.
    engine.select_row(0);
    assert_eq!(engine.hide_selection_or_row(Some(5)), 1);
    assert!(engine.is_hidden(5) && !engine.is_hidden(0));
}

#[test]
fn many_ranges_are_capped_quickly_and_saved_ones_past_the_end_are_kept() {
    let start = std::time::Instant::now();
    let many: Vec<_> = (0..200_000).map(|i| (i * 2, i * 2)).collect();
    let capped = merge_ranges(many);
    assert_eq!(capped.len(), fasttail::tail_engine::MAX_HIDDEN_RANGES);
    assert!(start.elapsed().as_secs() < 2, "{:?}", start.elapsed());
    // A stream that has not reached a saved range yet keeps it; the count covers only
    // the lines that exist.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    numbered(&log, 100);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_hidden_ranges(vec![(90, 99), (500, 600)]);
    assert_eq!(engine.hidden_ranges(), &[(90, 99), (500, 600)]);
    assert_eq!(engine.hidden_count(), 10);
    engine.set_hidden_ranges(vec![(0, usize::MAX - 1)]);
    assert_eq!(engine.hidden_count(), 100, "no overflow");
}
