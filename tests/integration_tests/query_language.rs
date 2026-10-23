// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::fields::ParserChoice;
use fasttail::query::{run, LineIn, Query};
use fasttail::tail_engine::TailEngine;
use std::time::{Duration, Instant};

fn access(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let status = [200, 200, 200, 404, 500][i % 5];
            format!(
                "ts=2026-10-23T10:{:02}:{:02}Z level={} status={status} path=/p{} ms={}",
                i / 60 % 60,
                i % 60,
                if status == 500 { "error" } else { "info" },
                i % 3,
                (i * 7) % 300 + 1
            )
        })
        .collect()
}

fn open(lines: &[String]) -> (tempfile::TempDir, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("access.log");
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    engine.set_field_choice(ParserChoice::Logfmt);
    (dir, engine)
}

fn wait_query(engine: &mut TailEngine) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while engine.query.as_ref().is_some_and(|q| q.running()) {
        engine.poll_updates();
        assert!(Instant::now() < deadline, "query did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_background_query_equals_a_direct_run_over_the_visible_lines() {
    let lines = access(1000);
    let (_dir, mut engine) = open(&lines);
    engine.set_include_filter("status>=400");
    super::wait_for_jobs(&mut engine);
    let text = "stats count() as n, p95(ms), dc(path) by status, level | sort status";
    engine.run_query(text);
    wait_query(&mut engine);
    let got = engine
        .query
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap()
        .unwrap();
    // The same query over the visible lines, run directly.
    let q = Query::parse(text, true, false, false).unwrap();
    let visible: Vec<&String> = lines.iter().filter(|l| !l.contains("status=200")).collect();
    let want = run(
        &q,
        engine.field_parser().cloned(),
        visible.iter().enumerate().map(|(i, t)| LineIn {
            idx: i,
            text: t,
            level: if t.contains("error") { 5 } else { 3 },
            ts: 0,
        }),
    )
    .unwrap();
    assert_eq!(got.columns, want.columns);
    assert_eq!(got.rows, want.rows);
    assert_eq!(got.rows.len(), 2);
    assert_eq!(got.rows[1][..3], ["500", "ERROR", "200"]);
    assert_eq!(got.lines_read, 400);

    // Row results carry their stream lines.
    engine.run_query("where ms>290 | fields line, ms | head 3");
    wait_query(&mut engine);
    let rows = engine
        .query
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap()
        .unwrap();
    let lines_of = rows.lines.clone().unwrap();
    assert_eq!(rows.rows.len(), 3);
    for (row, line) in rows.rows.iter().zip(&lines_of) {
        assert_eq!(row[0], (line + 1).to_string());
        assert!(row[1].parse::<u32>().unwrap() > 290);
    }

    // A timeslice over the timestamps.
    engine.set_include_filter("");
    super::wait_for_jobs(&mut engine);
    engine.run_query("timeslice 5m | stats count() by _slice");
    wait_query(&mut engine);
    let slices = engine
        .query
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap()
        .unwrap();
    assert_eq!(slices.rows.len(), 4, "{:?}", slices.rows);
    assert_eq!(slices.rows[0][1], "300");
}

#[test]
fn a_bad_query_fails_at_once_and_a_reload_stops_a_running_one() {
    let (_dir, mut engine) = open(&access(50));
    engine.run_query("where status>=500 | frobnicate");
    let err = engine
        .query
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap()
        .unwrap_err();
    assert_eq!((err.key, err.stage), ("query_err_stage", 2));
    assert!(!engine.query.as_ref().unwrap().running());

    engine.run_query("stats count()");
    assert!(engine.query.as_ref().unwrap().running());
    engine.rebuild_line_index();
    let r = engine.query.as_ref().unwrap().result.clone();
    match r {
        Some(Err(e)) => assert_eq!(e.key, "query_err_reloaded"),
        // The tiny file may have been read before the reload.
        Some(Ok(t)) => assert_eq!(t.rows[0][0], "50"),
        None => panic!("no result and no cancel"),
    }
}

fn app_frame(app: &mut fasttail::ui::FastTailApp) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(2000.0, 1200.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
        out.textures_delta.clear();
        texts.clear();
        for clipped in &out.shapes {
            collect_texts(&clipped.shape, &mut texts);
        }
    }
    texts
}

fn collect_texts(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_texts(s, out)),
        _ => {}
    }
}

#[test]
fn the_query_tab_shows_the_result_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("access.log");
    let lines = access(40);
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut app = fasttail::ui::FastTailApp::from_config(fasttail::config::FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    });
    app.open_log_file(path.clone());
    app.engines[0].set_field_choice(ParserChoice::Logfmt);
    app.engines[0].query_request = true;
    app_frame(&mut app);
    app.engines[0].run_query("stats count() as hits by status | sort -hits");
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.engines[0].query.as_ref().is_some_and(|q| q.running()) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
        app_frame(&mut app);
    }
    let texts = app_frame(&mut app);
    for cell in ["status", "hits", "200", "24", "404", "8"] {
        assert!(
            texts.iter().any(|t| t == cell),
            "{cell} missing in {texts:?}"
        );
    }
    let saved = fasttail::ui::find_results::without_find_results(&app.dock_state);
    assert!(!saved
        .iter_all_tabs()
        .any(|(_, t)| matches!(t, fasttail::ui::dock::FastTailTab::Query(_))));
}
