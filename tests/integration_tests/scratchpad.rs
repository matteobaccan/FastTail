// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::ui::scratchpad::{sidecar_of, Scratchpad};
use fasttail::ui::FastTailApp;

fn new_app(dir: &std::path::Path) -> FastTailApp {
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.join("spool")),
        ..FastTailConfig::default()
    });
    // Each test keeps its own pad (the test config path is shared).
    app.scratchpad = Scratchpad::load(dir.join("scratchpad.txt"));
    app
}

#[test]
fn rows_from_two_streams_and_a_find_result_land_in_the_pad() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    write_lines(&a, &["a one", "a two", "a three"]);
    write_lines(&b, &["b one", "b two"]);
    let mut app = new_app(dir.path());
    app.open_log_file(a.clone());
    app.open_log_file(b.clone());

    app.engines[0].select_row(1);
    app.engines[0].extend_selection_to(2);
    app.engines[0].scratch_request = Some(true);
    app.engines[1].select_row(0);
    app.engines[1].scratch_request = Some(false);
    assert!(app.apply_scratchpad_requests());
    assert_eq!(
        app.scratchpad.text,
        "── a.log:2 ──\na two\na three\nb one\n"
    );
    assert!(app.engines[0].view_notice.as_deref().unwrap().contains('2'));

    // "Send to scratchpad" on a Find results hit.
    app.find_all.scratch = Some((b.clone(), 1));
    app.apply_scratchpad_requests();
    assert!(app.scratchpad.text.ends_with("── b.log:2 ──\nb two\n"));

    // Nothing selected and no search hit: nothing is sent, and the stream says so.
    app.engines[1].clear_selection();
    app.engines[1].scratch_request = Some(true);
    let before = app.scratchpad.text.clone();
    app.apply_scratchpad_requests();
    assert_eq!(app.scratchpad.text, before);
    assert!(app.engines[1].view_notice.is_some());
}

#[test]
fn a_reference_reopens_a_closed_file_and_shows_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    write_lines(&a, &["zero", "one", "two", "three"]);
    let mut app = new_app(dir.path());
    app.open_log_file(a.clone());
    app.engines[0].select_row(2);
    app.engines[0].scratch_request = Some(true);
    app.apply_scratchpad_requests();
    // The stream is closed; the pad remembers where a.log was.
    app.engines.clear();
    app.scratchpad.jump_request = Some(("a.log".into(), 2));
    assert!(app.apply_scratchpad_requests());
    assert_eq!(app.engines.len(), 1, "the file is opened again");
    assert_eq!(
        app.find_all.jump.as_ref().map(|(p, l)| (p.clone(), *l)),
        Some((a.clone(), 2))
    );
    assert!(app.find_all.jump_in_context);
}

#[test]
fn the_pad_follows_the_session_it_is_saved_with() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    write_lines(&a, &["line"]);
    let mut app = new_app(dir.path());
    app.open_log_file(a.clone());
    app.engines[0].select_row(0);
    app.engines[0].scratch_request = Some(true);
    app.apply_scratchpad_requests();
    let session = dir.path().join("incident.fasttail-session.ini");
    app.save_session_as(session.clone()).unwrap();
    let pad = sidecar_of(&session);
    assert_eq!(app.scratchpad.file.as_deref(), Some(pad.as_path()));
    assert_eq!(
        std::fs::read_to_string(&pad).unwrap(),
        "── a.log:1 ──\nline\n"
    );
    // A fresh app loading that session gets its pad.
    let mut other = new_app(dir.path());
    other.load_session_file(session, true);
    assert_eq!(other.scratchpad.text, "── a.log:1 ──\nline\n");
}
