// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::filter_tab::{identity, is_derived_path, Stopped};
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::time::{Duration, Instant};

fn new_app(dir: &std::path::Path) -> FastTailApp {
    FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.join("spool")),
        ..FastTailConfig::default()
    })
}

/// Feeds the derived streams and lets them read their spools until `done` holds.
fn settle(app: &mut FastTailApp, done: impl Fn(&FastTailApp) -> bool) {
    let started = Instant::now();
    while !done(app) {
        assert!(started.elapsed() < Duration::from_secs(10), "never settled");
        app.apply_filter_tab_requests();
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
fn a_filter_opens_as_a_following_tab_numbered_like_its_source() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["INFO a", "ERROR b", "INFO c", "ERROR d"]);
    let mut app = new_app(dir.path());
    app.open_log_file(log.clone());
    app.engines[0].set_include_filter("ERROR");
    app.engines[0].filter_tab_request = true;
    assert!(app.apply_filter_tab_requests());
    assert_eq!(app.engines.len(), 2);
    let derived = app.engines[1].path.clone();
    assert!(is_derived_path(&derived));
    settle(&mut app, |a| a.engines[1].total_lines() == 2);
    assert_eq!(lines_of(&app, 1), vec!["ERROR b", "ERROR d"]);
    // Source numbers in the gutter and in go to line.
    assert_eq!(app.engines[1].shown_line_number(1), 4);
    let target = app.engines[1].resolve_goto("3", 0).unwrap();
    assert_eq!(target.line, 1, "source line 3 -> the next derived row");

    // The source's filter changes: the derived tab keeps its frozen one and follows.
    app.engines[0].set_include_filter("INFO");
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"ERROR e\nINFO f\n").unwrap();
    drop(f);
    settle(&mut app, |a| a.engines[1].total_lines() == 3);
    assert_eq!(lines_of(&app, 1), vec!["ERROR b", "ERROR d", "ERROR e"]);

    // Saved with the workspace under its `filter:` name, not its spool.
    app.save_dock_layout();
    assert!(!app.config.open_files.iter().any(|p| is_derived_path(p)));
    assert!(app.config.open_files.contains(&identity(1, &log)));

    // Show in context on a derived row goes to the source line.
    app.engines[1].source_context_request = Some(3);
    app.apply_filter_tab_requests();
    app.apply_scratchpad_requests();
    assert_eq!(
        app.find_all.jump.as_ref().map(|(p, l)| (p.clone(), *l)),
        Some((log.clone(), 3))
    );

    // Closing the source stops the derived tab.
    app.engines.remove(0);
    app.apply_filter_tab_requests();
    let stopped = app.engines[0].derived.as_ref().and_then(|d| d.stopped);
    assert_eq!(stopped, Some(Stopped::SourceClosed));
}

#[test]
fn without_a_filter_nothing_opens() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["one"]);
    let mut app = new_app(dir.path());
    app.open_log_file(log);
    app.engines[0].filter_tab_request = true;
    app.apply_filter_tab_requests();
    assert_eq!(app.engines.len(), 1);
    assert!(app.engines[0].view_notice.is_some());
}

#[test]
fn a_filter_tab_is_rebuilt_at_the_next_start_with_its_state_and_bookmarks() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["INFO a", "ERROR b", "INFO c", "ERROR d", "ERROR e"]);
    let mut app = new_app(dir.path());
    app.open_log_file(log.clone());
    app.engines[0].set_include_filter("ERROR");
    app.engines[0].filter_tab_request = true;
    app.apply_filter_tab_requests();
    settle(&mut app, |a| a.engines[1].total_lines() == 3);
    // Its own filter, and a bookmark with a note on "ERROR d" (source line 3).
    app.engines[1].set_exclude_filter("e");
    app.engines[1].toggle_bookmark(1);
    app.engines[1].set_bookmark_note(1, "second");
    let mut config = app.config.clone();
    for engine in app.engines.iter_mut() {
        fasttail::workspace::save_changes(engine, &mut config, false);
    }
    app.config = config;
    app.save_dock_layout();

    // The next start: the source and the derived tab, filled again from the source.
    let mut again = FastTailApp::from_config(app.config.clone());
    assert_eq!(again.engines.len(), 2);
    let d = again
        .engines
        .iter()
        .position(|e| e.derived.is_some())
        .expect("the derived tab is back");
    settle(&mut again, |a| {
        a.engines[d].total_lines() == 3 && !a.engines[d].bookmarks.is_empty()
    });
    assert_eq!(lines_of(&again, d), vec!["ERROR b", "ERROR d", "ERROR e"]);
    let engine = &again.engines[d];
    assert_eq!(
        engine.derived.as_ref().unwrap().identity(),
        identity(1, &log)
    );
    assert_eq!(engine.exclude_filter(), "e");
    assert!(engine.bookmarks.contains(&1));
    assert_eq!(
        engine.bookmark_notes.get(&1).map(String::as_str),
        Some("second")
    );
    let tabs = again.dock_state.iter_all_tabs().count();
    assert_eq!(
        tabs, 2,
        "one tab each, the derived one named after its spool"
    );
}

#[test]
fn a_session_brings_back_a_filter_tab_and_its_source_with_their_own_filters() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["INFO a", "ERROR b", "INFO c", "ERROR d"]);
    let mut app = new_app(dir.path());
    app.open_log_file(log.clone());
    app.engines[0].set_include_filter("ERROR");
    app.engines[0].filter_tab_request = true;
    app.apply_filter_tab_requests();
    settle(&mut app, |a| a.engines[1].total_lines() == 2);
    app.engines[0].set_include_filter("INFO");
    app.engines[1].set_exclude_filter("b");
    let file = dir
        .path()
        .join(format!("s{}", fasttail::session::SESSION_SUFFIX));
    app.save_session_as(file.clone()).unwrap();

    let mut other = new_app(dir.path());
    other.load_session_file(file, true);
    assert_eq!(other.engines.len(), 2);
    let d = other
        .engines
        .iter()
        .position(|e| e.derived.is_some())
        .expect("the filter tab is back");
    let s = 1 - d;
    assert_eq!(other.engines[s].include_filter(), "INFO");
    assert_eq!(other.engines[d].exclude_filter(), "b");
    settle(&mut other, |a| a.engines[d].total_lines() == 2);
    assert_eq!(lines_of(&other, d), vec!["ERROR b", "ERROR d"]);
}
