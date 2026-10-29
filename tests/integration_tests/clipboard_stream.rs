// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::clipboard_source::{self, PasteError};
use fasttail::stdin_source::{Limits, Settings};
use fasttail::tail_engine::TailEngine;
use std::time::{Duration, Instant};

fn settings(spool: &std::path::Path) -> Settings {
    Settings::from_config(Some(spool), 64)
}

fn wait(engine: &mut TailEngine, done: impl Fn(&TailEngine) -> bool) {
    let started = Instant::now();
    while !done(engine) {
        engine.poll_updates();
        assert!(started.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn lines(engine: &TailEngine) -> Vec<String> {
    (0..engine.total_lines())
        .filter_map(|i| engine.get_line(i).map(|l| l.trim_end().to_string()))
        .collect()
}

#[test]
fn two_pastes_build_one_stream_in_order() {
    let spool = tempfile::tempdir().unwrap();
    let s = settings(spool.path());
    let first: String = (1..=20).map(|i| format!("first {i}\n")).collect();
    let mut engine = clipboard_source::open_engine(&first, "14:02", &s, None).unwrap();
    assert!(engine.is_clipboard());
    assert!(clipboard_source::is_clipboard_path(&engine.path));
    assert_eq!(engine.source_title().as_deref(), Some("clipboard 14:02"));
    engine.size_check_interval = Duration::ZERO;
    wait(&mut engine, |e| e.total_lines() >= 20);
    // The second paste, without a final line break, then a third: each starts a line.
    let second: String = (1..=5).map(|i| format!("second {i}\n")).collect();
    wait(&mut engine, |e| {
        !e.clipboard.as_ref().unwrap().copier.is_reading()
    });
    engine
        .append_clipboard(second.trim_end(), &s, None)
        .unwrap();
    wait(&mut engine, |e| e.total_lines() >= 25);
    wait(&mut engine, |e| {
        !e.clipboard.as_ref().unwrap().copier.is_reading()
    });
    engine.append_clipboard("third\n", &s, None).unwrap();
    wait(&mut engine, |e| e.total_lines() >= 26);
    let all = lines(&engine);
    assert_eq!(all.len(), 26, "{all:?}");
    assert_eq!(all[0], "first 1");
    assert_eq!(all[19], "first 20");
    assert_eq!(all[20], "second 1");
    assert_eq!(all[24], "second 5");
    assert_eq!(all[25], "third");
}

#[test]
fn empty_and_too_large_pastes_are_refused() {
    let spool = tempfile::tempdir().unwrap();
    let mut s = settings(spool.path());
    assert!(matches!(
        clipboard_source::open_engine("", "10:00", &s, None),
        Err(PasteError::Empty)
    ));
    s.limits = Limits {
        max_bytes: 16,
        ..s.limits
    };
    assert!(matches!(
        clipboard_source::open_engine(&"x".repeat(17), "10:00", &s, None),
        Err(PasteError::TooLarge(16))
    ));
    let mut engine = clipboard_source::open_engine("0123456789\n", "10:00", &s, None).unwrap();
    wait(&mut engine, |e| {
        !e.clipboard.as_ref().unwrap().copier.is_reading()
    });
    assert!(matches!(
        engine.append_clipboard("0123456789\n", &s, None),
        Err(PasteError::TooLarge(16))
    ));
}

#[test]
fn a_clipboard_stream_is_not_kept_by_the_workspace() {
    let spool = tempfile::tempdir().unwrap();
    let engine =
        clipboard_source::open_engine("a\nb\n", "10:00", &settings(spool.path()), None).unwrap();
    assert!(!fasttail::workspace::is_persisted(&engine));
    assert!(fasttail::stdin_source::is_ephemeral_path(&engine.path));
    let order = vec![engine.path.clone()];
    let state = fasttail::workspace::snapshot(&order, [&engine]);
    assert!(state.open_files.is_empty(), "{:?}", state.open_files);
}

/// A frame of the window with these input events and modifiers.
fn frame(
    app: &mut fasttail::ui::FastTailApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) {
    let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let input = egui::RawInput {
        events: all,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

#[test]
fn ctrl_shift_v_opens_a_tab_and_append_adds_to_it() {
    use fasttail::ui::app::PasteTarget;
    let spool = tempfile::tempdir().unwrap();
    let mut app = fasttail::ui::FastTailApp::from_config(fasttail::config::FastTailConfig {
        spool_dir: Some(spool.path().to_path_buf()),
        ..Default::default()
    });
    let ctx = egui::Context::default();
    let shift_cmd = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    // A plain Ctrl+V is not ours.
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Paste("no\n".into())],
        egui::Modifiers::COMMAND,
    );
    assert!(app.engines.is_empty());
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Paste("one\ntwo\n".into())],
        shift_cmd,
    );
    assert_eq!(app.engines.len(), 1);
    let path = app.engines[0].path.clone();
    assert!(clipboard_source::is_clipboard_path(&path));
    wait(&mut app.engines[0], |e| {
        e.total_lines() >= 2 && !e.clipboard.as_ref().unwrap().copier.is_reading()
    });
    app.paste_text("three\n", PasteTarget::Append(path.clone()));
    wait(&mut app.engines[0], |e| e.total_lines() >= 3);
    assert_eq!(lines(&app.engines[0]), ["one", "two", "three"]);
    // The workspace does not keep it.
    assert!(!app.config.open_files.contains(&path));
}
