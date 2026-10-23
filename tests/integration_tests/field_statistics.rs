// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::field_stats::{SliceChoice, StatsSource};
use fasttail::fields::ParserChoice;
use fasttail::tail_engine::TailEngine;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn open(path: &Path) -> TailEngine {
    let mut engine = TailEngine::open(path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    engine
}

fn wait_stats(engine: &mut TailEngine) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.poll_updates();
        let s = engine.stats.as_ref().unwrap();
        if s.done() && s.covered_all(engine.total_lines()) {
            return;
        }
        assert!(Instant::now() < deadline, "statistics did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn count(engine: &TailEngine, value: &str) -> u64 {
    engine
        .stats
        .as_ref()
        .unwrap()
        .result
        .values
        .get(value)
        .map_or(0, |v| v.count)
}

fn access_log(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let status = [200, 200, 200, 404, 500][i % 5];
            let level = if status == 500 { "error" } else { "info" };
            format!(
                "ts=2026-10-23T10:{:02}:{:02}Z level={level} status={status} path=/p{} took={}ms",
                i / 60 % 60,
                i % 60,
                i % 3,
                i % 100
            )
        })
        .collect()
}

#[test]
fn field_values_are_counted_over_the_view_and_follow_filters_and_appends() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("access.log");
    let lines = access_log(500);
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = open(&path);
    engine.set_field_choice(ParserChoice::Logfmt);
    engine.open_stats(StatsSource::Field("status".into()), SliceChoice::Auto);
    wait_stats(&mut engine);
    let s = engine.stats.as_ref().unwrap();
    assert_eq!((s.result.lines, s.result.with_value), (500, 500));
    assert_eq!(count(&engine, "200"), 300);
    assert_eq!(count(&engine, "500"), 100);
    // ERROR is level 5: every 500 is an error line.
    assert_eq!(s.result.values["500"].levels[5], 100);
    let slices = s.result.slices.expect("timed lines get slices");
    assert_eq!(s.result.slice_totals().iter().sum::<u64>(), 500);
    assert!(slices.count <= fasttail::field_stats::MAX_AUTO_SLICES);

    // A filter narrows the view: counted again over what it shows.
    engine.set_include_filter("status>=404");
    wait_stats(&mut engine);
    assert_eq!(count(&engine, "200"), 0);
    assert_eq!(count(&engine, "404"), 100);
    assert_eq!(engine.stats.as_ref().unwrap().result.lines, 200);

    // Appended visible lines are added.
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(
        f,
        "ts=2026-10-23T11:00:00Z level=error status=500 path=/x took=1ms"
    )
    .unwrap();
    writeln!(
        f,
        "ts=2026-10-23T11:00:01Z level=info status=200 path=/x took=1ms"
    )
    .unwrap();
    drop(f);
    wait_stats(&mut engine);
    assert_eq!(count(&engine, "500"), 101);
    assert_eq!(count(&engine, "200"), 0);

    // A truncated file is counted again from scratch.
    write_lines(
        &path,
        &["ts=2026-10-23T12:00:00Z level=warn status=404 path=/y took=2ms"],
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while engine.total_lines() != 1 {
        engine.poll_updates();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_stats(&mut engine);
    assert_eq!(count(&engine, "404"), 1);
    assert_eq!(count(&engine, "500"), 0);
}

#[test]
fn a_regex_capture_works_without_a_parser_and_the_job_matches_a_direct_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.log");
    let lines: Vec<String> = (0..3000)
        .map(|i| format!("INFO query {} took {}ms", i, (i * 7) % 250 + 1))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = open(&path);
    let re = regex::Regex::new(r"took (\d+)ms").unwrap();
    engine.open_stats(StatsSource::Capture(re.clone()), SliceChoice::Off);
    wait_stats(&mut engine);
    let s = &engine.stats.as_ref().unwrap().result;
    assert!(s.slices.is_none());
    assert!(s.is_numeric());
    let mut direct = std::collections::HashMap::<String, u64>::new();
    for l in &lines {
        *direct
            .entry(re.captures(l).unwrap()[1].to_string())
            .or_default() += 1;
    }
    assert_eq!(s.distinct(), direct.len());
    for (v, n) in &direct {
        assert_eq!(s.values[v.as_str()].count, *n, "{v}");
    }
    let mut sorted: Vec<f64> = lines
        .iter()
        .map(|l| re.captures(l).unwrap()[1].parse().unwrap())
        .collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let exact = sorted[(0.95 * sorted.len() as f64).ceil() as usize - 1];
    let p95 = s.hist.percentile(0.95).unwrap();
    assert!((p95 - exact).abs() <= exact * 0.011, "{p95} vs {exact}");
    engine.close_stats();
    assert!(engine.stats.is_none());
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
fn the_statistics_tab_opens_from_a_request_and_lists_the_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("access.log");
    let lines = access_log(50);
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut app = fasttail::ui::FastTailApp::from_config(fasttail::config::FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    });
    app.open_log_file(path.clone());
    app.engines[0].set_field_choice(ParserChoice::Logfmt);
    app.engines[0].stats_request = Some(Some("status".into()));
    app_frame(&mut app);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !app.engines[0].stats.as_ref().is_some_and(|s| s.done()) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
        app_frame(&mut app);
    }
    let texts = app_frame(&mut app);
    let lang = app.config.language;
    let title = fasttail::i18n::t(lang, "stats_title");
    assert!(texts.iter().any(|t| t.contains(title)), "{texts:?}");
    for cell in ["200", "404", "500", "30", "60.0%"] {
        assert!(
            texts.iter().any(|t| t == cell),
            "{cell} missing in {texts:?}"
        );
    }
    // The tab is not saved with the layout.
    let saved = fasttail::ui::find_results::without_find_results(&app.dock_state);
    assert!(!saved
        .iter_all_tabs()
        .any(|(_, t)| matches!(t, fasttail::ui::dock::FastTailTab::Statistics(_))));
}

#[test]
fn filter_terms_are_added_until_a_side_is_full() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.log");
    write_lines(&path, &["status=200", "status=500"]);
    let mut engine = open(&path);
    engine.set_field_choice(ParserChoice::Logfmt);
    assert!(engine.add_filter_term("status=500", true));
    assert!(
        engine.add_filter_term("status=500", true),
        "a duplicate is fine"
    );
    super::wait_for_jobs(&mut engine);
    assert_eq!(engine.visible_lines(), 1);
    assert_eq!(
        engine
            .include_terms()
            .iter()
            .filter(|t| !t.is_empty())
            .count(),
        1
    );
    for i in 0..7 {
        assert!(engine.add_filter_term(&format!("x{i}"), false));
    }
    assert!(engine.add_filter_term("x7", false));
    assert!(!engine.add_filter_term("x8", false), "eight terms at most");
}
