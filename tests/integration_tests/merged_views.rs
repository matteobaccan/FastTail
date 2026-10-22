// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::filter_tab::Stopped;
use fasttail::ui::merge_dialog::{mergeable, MergePrompt};
use fasttail::ui::FastTailApp;
use fasttail::workspace::WorkspaceState;
use std::io::Write;
use std::path::PathBuf;
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
    let mut prompt = MergePrompt::with_picked(vec![gw.clone(), pay.clone(), plain.clone()]);
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

    // Saved under its `merged:` name, not its spool.
    let identity = fasttail::merged_stream::identity(1, &[gw.clone(), pay.clone()]).unwrap();
    app.save_dock_layout();
    assert!(!app.config.open_files.contains(&merged));
    assert!(app.config.open_files.contains(&identity));
    assert!(fasttail::merged_stream::is_merged_path(&merged));
    let session = app.capture_session();
    assert!(session.streams.iter().any(|s| s.path == identity));
    let layout = session.dock_layout.unwrap_or_default();
    assert!(layout.contains("merged:1:"), "{layout}");
    assert!(!layout.contains(&*merged.to_string_lossy()));

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

/// A workspace with two logs and their merged view, filtered to `pay`.
fn merged_workspace(dir: &std::path::Path) -> (FastTailApp, std::path::PathBuf) {
    let gw = dir.join("gateway.log");
    let pay = dir.join("payment.log");
    write_lines(&gw, &["2026-09-28T14:02:05.100Z INFO gw request 42"]);
    write_lines(&pay, &["2026-09-28T14:02:05.250Z INFO pay request 42"]);
    let mut app = new_app(dir);
    app.open_log_file(gw.clone());
    app.open_log_file(pay.clone());
    app.open_merged_view(&[gw.clone(), pay.clone()]).unwrap();
    let m = app.engines.iter().position(|e| e.merged.is_some()).unwrap();
    app.engines[m].set_include_filter("pay");
    let identity = fasttail::merged_stream::identity(1, &[gw, pay]).unwrap();
    (app, identity)
}

fn merged_index(app: &FastTailApp) -> usize {
    app.engines
        .iter()
        .position(|e| e.merged.is_some())
        .expect("the merged view is back")
}

#[test]
fn a_merged_view_is_rebuilt_at_the_next_start_with_its_filter() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, identity) = merged_workspace(dir.path());
    let mut config = app.config.clone();
    for engine in app.engines.iter_mut() {
        fasttail::workspace::save_changes(engine, &mut config, false);
    }
    app.config = config;
    app.save_dock_layout();

    let mut again = FastTailApp::from_config(app.config.clone());
    assert_eq!(again.engines.len(), 3);
    let m = merged_index(&again);
    settle(&mut again, |a| a.engines[m].total_lines() == 2);
    let engine = &again.engines[m];
    assert_eq!(engine.merged.as_ref().unwrap().identity(), Some(identity));
    assert_eq!(engine.include_filter(), "pay");
    assert_eq!(
        again.dock_state.iter_all_tabs().count(),
        3,
        "one tab each, the merged one named after its spool"
    );
}

#[test]
fn a_session_brings_back_a_merged_view_with_its_filter() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = merged_workspace(dir.path());
    let file = dir
        .path()
        .join(format!("s{}", fasttail::session::SESSION_SUFFIX));
    app.save_session_as(file.clone()).unwrap();

    let mut other = new_app(dir.path());
    other.load_session_file(file, true);
    assert_eq!(other.engines.len(), 3);
    let m = merged_index(&other);
    assert_eq!(other.engines[m].include_filter(), "pay");
    settle(&mut other, |a| a.engines[m].total_lines() == 2);

    // A merged view whose source was closed is not saved: it would open it again.
    other.engines.retain(|e| !e.path.ends_with("gateway.log"));
    other.save_dock_layout();
    assert!(!other
        .config
        .open_files
        .iter()
        .any(|p| fasttail::merged_stream::parse_identity(p).is_some()));
}

#[test]
fn the_terminal_keeps_a_merged_view_while_its_sources_are_open() {
    let (a, b) = (PathBuf::from("/l/a.log"), PathBuf::from("/l/b.log"));
    let saved = fasttail::merged_stream::identity(1, &[a.clone(), b.clone()]).unwrap();
    let previous = vec![a.clone(), b.clone(), saved.clone()];
    let mut both = WorkspaceState {
        open_files: vec![a.clone(), b.clone()],
        streams: Vec::new(),
    };
    both.keep_derived(&previous);
    assert_eq!(both.open_files, previous);
    let mut one = WorkspaceState {
        open_files: vec![a],
        streams: Vec::new(),
    };
    one.keep_derived(&previous);
    assert!(!one.open_files.contains(&saved));
}
