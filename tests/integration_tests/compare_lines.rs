// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::compare::{CompareRequest, RowKind};
use fasttail::config::FastTailConfig;
use fasttail::ui::FastTailApp;

fn new_app(dir: &std::path::Path) -> FastTailApp {
    FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.join("spool")),
        ..FastTailConfig::default()
    })
}

#[test]
fn two_selected_lines_open_a_compare_with_one_change() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("a.log");
    write_lines(
        &log,
        &[
            "2026-09-18T14:02:05.100Z INFO pay id=7f3a status=ok",
            "noise",
            "2026-09-18T14:09:41.900Z INFO pay id=7f3a status=failed",
        ],
    );
    let mut app = new_app(dir.path());
    app.open_log_file(log.clone());
    app.engines[0].select_row(0);
    app.engines[0].toggle_row(2);
    app.engines[0].compare_request = Some(CompareRequest::SelectedPair);
    assert!(app.apply_compare_requests());
    let view = app.compare.as_mut().expect("a compare");
    assert_eq!(view.left.label(), "a.log:1");
    assert_eq!(view.right.label(), "a.log:3");
    let diff = view.diff().clone();
    assert_eq!(diff.changes.len(), 1);
    assert_eq!(diff.rows[0].kind, RowKind::Changed);
    let words = diff.rows[0].words.as_ref().unwrap();
    assert_eq!(words.left.len(), 1, "only status=ok differs: {words:?}");
    assert!(view
        .unified_text()
        .contains("-2026-09-18T14:02:05.100Z INFO pay id=7f3a status=ok"));
}

#[test]
fn a_marked_region_compares_with_a_selection_of_another_stream() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("run1.log");
    let b = dir.path().join("run2.log");
    write_lines(&a, &["start", "load config", "connect db", "done"]);
    write_lines(
        &b,
        &["start", "load config", "retry db", "connect db", "done"],
    );
    let mut app = new_app(dir.path());
    app.open_log_file(a.clone());
    app.open_log_file(b.clone());
    app.engines[0].select_all_visible();
    app.engines[0].compare_request = Some(CompareRequest::Mark);
    app.apply_compare_requests();
    assert!(app.compare_mark.is_some());
    assert!(app.engines[0].view_notice.is_some());

    app.engines[1].select_all_visible();
    app.engines[1].compare_request = Some(CompareRequest::WithMark);
    app.apply_compare_requests();
    let view = app.compare.as_mut().expect("a compare");
    let diff = view.diff().clone();
    let added: Vec<_> = diff
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Added)
        .collect();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].right, Some(2));

    // A double-click on a row goes back to its stream.
    view.jump = Some((b.clone(), 2));
    app.apply_compare_requests();
    assert_eq!(
        app.find_all.jump.as_ref().map(|(p, l)| (p.clone(), *l)),
        Some((b.clone(), 2))
    );

    // Closing the marked stream forgets the mark.
    app.engines.remove(0);
    app.apply_compare_requests();
    assert!(app.compare_mark.is_none());
}

#[test]
fn json_lines_compare_field_by_field() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("j.log");
    write_lines(
        &log,
        &[
            r#"INFO {"user":"bob","cart":{"items":3,"total":10}}"#,
            r#"INFO {"cart":{"total":12,"items":3},"user":"bob"}"#,
        ],
    );
    let mut app = new_app(dir.path());
    app.open_log_file(log);
    app.engines[0].select_row(0);
    app.engines[0].toggle_row(1);
    app.engines[0].compare_request = Some(CompareRequest::SelectedPair);
    app.apply_compare_requests();
    let view = app.compare.as_mut().unwrap();
    assert!(view.json_possible());
    view.opts.json = true;
    let diff = view.diff().clone();
    let changed = diff
        .rows
        .iter()
        .filter(|r| r.kind != RowKind::Equal)
        .count();
    assert_eq!(changed, 1, "only \"total\" differs");
}
