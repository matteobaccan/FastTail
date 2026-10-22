// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::filter_tab::Stopped;
use fasttail::ui::merge_dialog::{mergeable, MergePrompt};
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::time::{Duration, Instant};

fn new_app(dir: &std::path::Path) -> FastTailApp {
    FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.join("spool")),
        ..FastTailConfig::default()
    })
}

/// Merges and lets the merged view read its spool until `done` holds.
fn settle(app: &mut FastTailApp, done: impl Fn(&FastTailApp) -> bool) {
    let started = Instant::now();
    while !done(app) {
        assert!(started.elapsed() < Duration::from_secs(10), "never settled");
        app.apply_merged_views();
        for engine in app.engines.iter_mut() {
            engine.size_check_interval = Duration::ZERO;
            engine.poll_updates();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn lines_of(app: &FastTailApp, idx: usize) -> Vec<String> {
    let e = &app.engines[idx];
    (0..e.total_lines())
        .filter_map(|l| e.get_line(l).map(|t| t.into_owned()))
        .collect()
}

#[test]
fn two_services_read_as_one_timeline_that_follows_them() {
    let dir = tempfile::tempdir().unwrap();
    let gw = dir.path().join("gateway.log");
    let pay = dir.path().join("payment.log");
    let plain = dir.path().join("notes.txt");
    write_lines(&gw, &["2026-09-28T14:02:05.100Z INFO gw request 42"]);
    write_lines(&pay, &["2026-09-28T14:02:05.250Z INFO pay request 42"]);
    write_lines(&plain, &["no time"]);
    let mut app = new_app(dir.path());
    for p in [&gw, &pay, &plain] {
        app.open_log_file(p.clone());
    }
    // The dialog keeps the streams without timestamps out.
    assert!(!mergeable(&app.engines[2]));
    let prompt = MergePrompt {
        picked: vec![gw.clone(), pay.clone(), plain.clone()],
    };
    let sources = prompt.sources(&app.engines);
    assert_eq!(sources, vec![gw.clone(), pay.clone()]);
    let merged = app.open_merged_view(&sources).unwrap();
    let m = app.engines.iter().position(|e| e.path == merged).unwrap();
    settle(&mut app, |a| a.engines[m].total_lines() == 2);
    let lines = lines_of(&app, m);
    assert!(lines[0].contains("gw request 42") && lines[1].contains("pay request 42"));
    assert_eq!(
        app.engines[m].shown_line_number(1),
        1,
        "the source's line number"
    );

    // Both grow: the merged view follows, in time order.
    let mut f = std::fs::OpenOptions::new().append(true).open(&pay).unwrap();
    writeln!(f, "2026-09-28T14:02:06.000Z INFO pay done").unwrap();
    drop(f);
    let mut f = std::fs::OpenOptions::new().append(true).open(&gw).unwrap();
    writeln!(f, "2026-09-28T14:02:05.900Z WARN gw slow").unwrap();
    drop(f);
    settle(&mut app, |a| a.engines[m].total_lines() == 4);
    let lines = lines_of(&app, m);
    assert!(lines[2].contains("gw slow"), "{lines:?}");
    assert!(lines[3].contains("pay done"), "{lines:?}");

    // Filters and search work on it as on any stream.
    app.engines[m].set_include_filter("pay");
    assert_eq!(app.engines[m].visible_line_count(), 2);
    app.engines[m].set_include_filter("");

    // A source turned off leaves the view.
    app.engines[m]
        .merged
        .as_mut()
        .unwrap()
        .set_enabled(0, false);
    settle(&mut app, |a| a.engines[m].total_lines() == 2);
    assert!(lines_of(&app, m).iter().all(|l| l.contains("pay")));

    // Not kept in the workspace: its spool goes with the process.
    app.save_dock_layout();
    assert!(!app.config.open_files.contains(&merged));

    // Every source closed: it stops following.
    app.engines.retain(|e| e.merged.is_some());
    app.apply_merged_views();
    assert_eq!(
        app.engines[0].merged.as_ref().unwrap().stopped,
        Some(Stopped::SourceClosed)
    );
}

#[test]
fn the_stream_menu_opens_the_dialog_with_that_stream_ticked() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    write_lines(&a, &["2026-01-01T10:00:00Z a"]);
    let mut app = new_app(dir.path());
    app.open_log_file(a.clone());
    app.engines[0].merged_request = true;
    assert!(app.apply_merged_views());
    assert_eq!(app.merge_prompt.as_ref().unwrap().picked, vec![a]);
}
