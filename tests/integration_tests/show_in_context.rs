// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::wait_for_jobs;
use egui_dock::DockState;
use fasttail::find_all::FindAllSession;
use fasttail::i18n::Language;
use fasttail::tail_engine::{TailEngine, ViewMode};
use fasttail::ui::dock::FastTailTab;
use fasttail::ui::find_results::apply_find_jump;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn write(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    let text: String = (0..lines)
        .map(|i| {
            if i % 10 == 3 {
                format!("ERROR line {i}\n")
            } else {
                format!("INFO line {i}\n")
            }
        })
        .collect();
    std::fs::write(&path, text).unwrap();
    path
}

fn visible(engine: &TailEngine) -> Vec<usize> {
    (0..engine.visible_line_count())
        .filter_map(|r| engine.get_actual_line_idx(r))
        .collect()
}

fn exported(engine: &TailEngine) -> String {
    let mut out = Vec::new();
    engine.export_visible(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn poll_until(engine: &mut TailEngine, what: &str, done: impl Fn(&TailEngine) -> bool) {
    let start = Instant::now();
    while !done(engine) {
        assert!(start.elapsed() < Duration::from_secs(10), "{what} not seen");
        engine.poll_updates();
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_for_jobs(engine);
}

#[test]
fn enter_and_leave_keep_the_filtered_view_and_start_no_job() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", 5_000);
    // Thresholds at 0: any filter work would run as a background job.
    let mut engine = TailEngine::open_with_thresholds(&path, 0, 0).unwrap();
    wait_for_jobs(&mut engine);
    engine.set_include_filter("ERROR");
    wait_for_jobs(&mut engine);
    let filtered = visible(&engine);
    assert_eq!(filtered.len(), 500);
    engine.select_row(filtered[7]);
    engine.follow_tail = true;
    let selection = engine.selection.clone();

    let line = filtered[7] + 2;
    assert!(engine.enter_context(line));
    assert!(engine.scan_progress().is_none(), "entering starts no job");
    assert_eq!(engine.context_line(), Some(line));
    assert!(!engine.rows_filtered());
    assert!(engine.is_filter_active(), "the filter settings stay");
    assert_eq!(engine.visible_line_count(), 5_000);
    assert!(!engine.follow_tail, "follow is paused");
    assert!(engine.selection.contains(&line));
    assert_eq!(engine.pending_jump, Some(line));

    engine.leave_context();
    assert!(engine.scan_progress().is_none(), "returning starts no job");
    assert_eq!(engine.context_line(), None);
    assert_eq!(visible(&engine), filtered);
    assert_eq!(engine.selection, selection);
    assert!(engine.follow_tail);
    assert_eq!(engine.include_filter(), "ERROR");
}

#[test]
fn scroll_position_is_restored_without_follow() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", 200)).unwrap();
    engine.set_include_filter("ERROR");
    engine.follow_tail = false;
    engine.current_scroll_y = 321.0;
    assert!(engine.enter_context(13));
    engine.current_scroll_y = 9_999.0;
    engine.leave_context();
    assert_eq!(engine.requested_scroll_y, Some(321.0));
    assert!(!engine.follow_tail);
}

#[test]
fn unavailable_without_a_filter_or_past_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", 50)).unwrap();
    assert!(!engine.enter_context(3), "no filter to suspend");
    engine.set_include_filter("ERROR");
    assert!(!engine.enter_context(50));
    assert!(engine.enter_context(4));
}

#[test]
fn rows_export_and_copy_match_an_unfiltered_stream() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", 300);
    let plain = TailEngine::open(&path).unwrap();
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_include_filter("ERROR");
    engine.set_exclude_filter("line 13");
    assert_eq!(engine.get_visible_row_of_line(13), None);
    assert!(engine.enter_context(13));
    assert_eq!(visible(&engine), visible(&plain));
    assert_eq!(engine.get_visible_row_of_line(13), Some(13));
    assert_eq!(exported(&engine), exported(&plain));
    assert_eq!(
        engine.copy_selection_text().as_deref().map(str::trim_end),
        Some("ERROR line 13")
    );
    engine.leave_context();
    // 30 ERROR lines, less "line 13" and "line 133".
    assert_eq!(exported(&engine).lines().count(), 28);
}

#[test]
fn appended_lines_are_filtered_while_in_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", 100);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_include_filter("ERROR");
    assert!(engine.enter_context(5));
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"INFO new 1\nERROR new 2\n").unwrap();
    }
    poll_until(&mut engine, "append", |e| e.total_lines() >= 102);
    assert_eq!(engine.context_line(), Some(5));
    assert_eq!(engine.visible_line_count(), 102, "every line is shown");
    engine.leave_context();
    let rows = visible(&engine);
    assert_eq!(rows.len(), 11);
    assert_eq!(rows.last(), Some(&101));
}

#[test]
fn a_filter_edit_ends_it_and_centres_the_line_when_visible() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", 100)).unwrap();
    engine.set_include_filter("ERROR");
    assert!(engine.enter_context(23));
    engine.pending_jump = None;
    engine.set_include_filter("line 2");
    assert_eq!(engine.context_line(), None);
    assert!(engine.rows_filtered());
    assert_eq!(engine.pending_jump, Some(23));
    // A filter that hides the line: no jump.
    assert!(engine.enter_context(23));
    engine.pending_jump = None;
    engine.set_include_filter("line 5");
    assert_eq!(engine.context_line(), None);
    assert_eq!(engine.pending_jump, None);
}

#[test]
fn truncation_and_hex_view_end_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", 100);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_include_filter("ERROR");
    assert!(engine.enter_context(40));
    engine.set_view_mode(ViewMode::Hex);
    assert_eq!(engine.context_line(), None);
    engine.set_view_mode(ViewMode::Text);

    assert!(engine.enter_context(40));
    std::fs::write(&path, "ERROR short\n").unwrap();
    poll_until(&mut engine, "truncation", |e| e.context_line().is_none());
    assert!(engine.rows_filtered());
}

#[test]
fn find_results_show_a_hidden_line_in_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", 100);
    let mut engines = vec![TailEngine::open(&path).unwrap()];
    let mut session = FindAllSession::default();
    session.input = "line 42".into();
    session.start(&engines);
    let start = Instant::now();
    while session.is_active() {
        assert!(start.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(2));
        session.poll(&engines);
    }
    assert_eq!(session.groups.len(), 1);
    // The filter set after the search hides the result's line.
    engines[0].set_include_filter("ERROR");
    assert_eq!(engines[0].get_visible_row_of_line(42), None);
    let mut dock = DockState::new(vec![FastTailTab::LogStream(path.clone())]);

    assert!(session.commit_in_context(0, 0));
    assert!(apply_find_jump(
        &mut session,
        &mut engines,
        &mut dock,
        Language::En
    ));
    assert_eq!(engines[0].context_line(), Some(42));
    assert_eq!(engines[0].pending_jump, Some(42));
    assert!(!session.jump_in_context);

    // A plain commit jumps without entering the context view.
    engines[0].leave_context();
    assert!(session.commit(0, 0));
    assert!(apply_find_jump(
        &mut session,
        &mut engines,
        &mut dock,
        Language::En
    ));
    assert_eq!(engines[0].context_line(), None);
}
