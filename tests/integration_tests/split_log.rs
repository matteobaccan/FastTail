// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::split::{preview, SplitBy, SplitJob, SplitPlan};
use fasttail::tail_engine::TailEngine;
use std::time::Duration;

fn open(path: &std::path::Path) -> TailEngine {
    let mut e = TailEngine::open(path).unwrap();
    wait_for_jobs(&mut e);
    e
}

fn run(engine: &TailEngine, plan: SplitPlan) -> SplitJob {
    let mut job = SplitJob::start(engine, plan);
    while !job.step(engine, Duration::from_millis(5)) {}
    job
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn by_lines_with_a_preview_and_no_silent_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&log, &refs);
    let e = open(&log);
    let plan = SplitPlan::for_file(&log, SplitBy::Lines(4));
    let p = preview(&e, &plan);
    assert_eq!(p.pieces, 3);
    assert_eq!(
        p.names,
        ["app.part-001.log", "app.part-002.log", "app.part-003.log"]
    );
    assert!(p.existing.is_empty());
    let job = run(&e, plan.clone());
    assert_eq!(job.error, None);
    assert_eq!(job.written.len(), 3);
    assert_eq!(
        read(&dir.path().join("app.part-001.log")),
        "line 1\nline 2\nline 3\nline 4\n"
    );
    assert_eq!(
        read(&dir.path().join("app.part-003.log")),
        "line 9\nline 10\n"
    );
    assert_eq!(job.lines_written, 10);
    // Written again: the preview lists the files that would be replaced.
    assert_eq!(preview(&e, &plan).existing.len(), 3);
}

#[test]
fn a_piece_name_taken_by_a_folder_stops_the_split() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["one", "two"]);
    std::fs::create_dir(dir.path().join("app.part-001.log")).unwrap();
    let e = open(&log);
    let job = run(&e, SplitPlan::for_file(&log, SplitBy::Lines(1)));
    assert!(job.error.is_some());
    assert!(dir.path().join("app.part-001.log").is_dir());
}

#[test]
fn by_size_cuts_at_line_ends_and_a_long_line_is_its_own_piece() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("big.txt");
    let long = "x".repeat(50);
    write_lines(&log, &["aaaa", "bbbb", "cccc", &long, "dd"]);
    let e = open(&log);
    let mut plan = SplitPlan::for_file(&log, SplitBy::Bytes(12));
    plan.dir = dir.path().join("out");
    std::fs::create_dir_all(&plan.dir).unwrap();
    let job = run(&e, plan.clone());
    let pieces: Vec<String> = job.written.iter().map(|p| read(p)).collect();
    assert_eq!(
        pieces,
        [
            "aaaa\nbbbb\n",
            "cccc\n",
            format!("{long}\n").as_str(),
            "dd\n"
        ]
    );
    assert!(job.written[0].starts_with(dir.path().join("out")));
}

#[test]
fn by_hour_with_untimed_head_and_traces_and_visible_only() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("svc.log");
    write_lines(
        &log,
        &[
            "banner without time",
            "2026-10-05 13:59:00 INFO a",
            "    at trace",
            "2026-10-05 14:01:00 ERROR b",
            "2026-10-05 16:30:00 INFO c",
        ],
    );
    let mut e = open(&log);
    e.ensure_timestamps();
    while !e.timestamps_complete() {
        e.fill_timestamps();
        wait_for_jobs(&mut e);
    }
    let plan = SplitPlan::for_file(&log, SplitBy::Time(3_600_000));
    let p = preview(&e, &plan);
    assert_eq!(p.pieces, 4, "13:00 to 16:00, the empty 15:00 included");
    let job = run(&e, plan.clone());
    let names: Vec<String> = job
        .written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "svc.2026-10-05T13.log",
            "svc.2026-10-05T14.log",
            "svc.2026-10-05T16.log"
        ]
    );
    assert_eq!(
        read(&job.written[0]),
        "banner without time\n2026-10-05 13:59:00 INFO a\n    at trace\n"
    );

    // Visible lines only: the filters apply.
    e.set_include_filter("ERROR");
    let mut plan = SplitPlan::for_file(&log, SplitBy::Lines(100));
    plan.visible_only = true;
    plan.dir = dir.path().join("vis");
    std::fs::create_dir_all(&plan.dir).unwrap();
    let job = run(&e, plan);
    assert_eq!(read(&job.written[0]), "2026-10-05 14:01:00 ERROR b\n");
}

#[test]
fn cancel_keeps_the_pieces_written_and_drops_the_incomplete_one() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    let lines: Vec<String> = (0..5000).map(|i| format!("line {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&log, &refs);
    let e = open(&log);
    let mut job = SplitJob::start(&e, SplitPlan::for_file(&log, SplitBy::Lines(1000)));
    assert!(!job.step(&e, Duration::ZERO), "one slice of lines");
    job.cancel();
    assert!(job.done);
    let left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|f| f.file_name().to_string_lossy().contains(".part-"))
        .collect();
    assert_eq!(left.len(), job.written.len(), "no incomplete piece left");
}

#[test]
fn the_stream_menu_opens_the_dialog_which_splits_with_progress() {
    use fasttail::config::FastTailConfig;
    use fasttail::ui::FastTailApp;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    let lines: Vec<String> = (0..25).map(|i| format!("line {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&log, &refs);
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..Default::default()
    });
    app.open_log_file(log.clone());
    app.engines[0].split_request = true;
    app.apply_merged_views();
    let dialog = app.split_dialog.as_mut().expect("the dialog");
    dialog.lines = 10;
    assert_eq!(dialog.dir, dir.path());
    let plan = dialog.plan(&app.engines[0]);
    dialog.job = Some(SplitJob::start(&app.engines[0], plan));
    let ctx = egui::Context::default();
    for _ in 0..50 {
        let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
        out.textures_delta.clear();
        if app.split_dialog.as_ref().is_some_and(|d| d.job.is_none()) {
            break;
        }
    }
    let dialog = app.split_dialog.as_ref().unwrap();
    assert!(dialog.job.is_none(), "finished");
    let notice = dialog
        .notice
        .as_ref()
        .map(|(t, _)| t.clone())
        .unwrap_or_default();
    assert!(notice.starts_with("3 files written"), "{notice}");
    assert!(dir.path().join("app.part-003.log").exists());
}
