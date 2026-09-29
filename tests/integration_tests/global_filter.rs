// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::wait_for_jobs;
use fasttail::config::FastTailConfig;
use fasttail::find_all::FindAllSession;
use fasttail::global_filter::GlobalFilter;
use fasttail::scan_job::FilterSpec;
use fasttail::tail_engine::TailEngine;
use fasttail::ui::FastTailApp;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const LOG: &str = "INFO req-7f3a start\n\
        INFO GET /healthcheck req-7f3a\n\
        ERROR req-7f3a failed\n\
        \x20   at Pay.charge(Pay.java:10)\n\
        \x20   at healthcheck.Probe(Probe.java:3)\n\
        INFO other request\n\
        ERROR other failed\n";

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn visible(engine: &TailEngine) -> Vec<usize> {
    (0..engine.visible_line_count())
        .filter_map(|r| engine.get_actual_line_idx(r))
        .collect()
}

fn global(include: &[&str], exclude: &[&str]) -> Option<Arc<FilterSpec>> {
    Some(Arc::new(FilterSpec::build(include, exclude, false, false)))
}

#[test]
fn combined_with_the_stream_terms_and_the_stack_trace_rule() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", LOG)).unwrap();
    engine.set_global_filter(global(&["req-7f3a"], &["healthcheck"]));
    // The trace follows its ERROR entry, except the frame the global exclude matches.
    assert_eq!(visible(&engine), vec![0, 2, 3]);
    assert!(engine.global_filter().is_some());
    // The stream's own include narrows further; its own terms stay as they were.
    engine.set_include_filter("ERROR");
    assert_eq!(visible(&engine), vec![2, 3]);
    assert_eq!(engine.include_filter(), "ERROR");
    // Off: the stream's own filtering only.
    engine.set_global_filter(None);
    assert_eq!(visible(&engine), vec![2, 3, 4, 6]);
    // A set without any term is no filter.
    engine.set_include_filter("");
    engine.set_global_filter(global(&[""], &[""]));
    assert!(engine.global_filter().is_none());
    assert!(!engine.is_filter_active());
}

#[test]
fn a_background_filter_job_matches_the_synchronous_path() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..3_000)
        .map(|i| LOG.replace("other", &format!("n{i}")))
        .collect();
    let path = write(dir.path(), "big.log", &text);
    let set = global(&["req-7f3a"], &["healthcheck"]);
    let mut sync = TailEngine::open(&path).unwrap();
    sync.set_global_filter(set.clone());
    let mut bg = TailEngine::open_with_thresholds(&path, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut bg);
    bg.set_global_filter(set);
    wait_for_jobs(&mut bg);
    assert_eq!(visible(&bg), visible(&sync));
    assert_eq!(visible(&sync).len(), 3 * 3_000);
}

#[test]
fn find_results_see_the_global_filter() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = TailEngine::open(write(dir.path(), "a.log", LOG)).unwrap();
    let mut b = TailEngine::open(write(dir.path(), "b.log", LOG)).unwrap();
    let set = global(&[], &["healthcheck"]);
    a.set_global_filter(set.clone());
    b.set_global_filter(set);
    let engines = vec![a, b];
    let mut session = FindAllSession::default();
    session.input = "req-7f3a".into();
    session.start(&engines);
    let started = Instant::now();
    while session.is_active() {
        assert!(started.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(2));
        session.poll(&engines);
    }
    for g in &session.groups {
        assert_eq!(g.hits, vec![0, 2], "no healthcheck line among the results");
    }
}

fn frame(app: &mut FastTailApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

#[test]
fn every_stream_now_and_later_until_switched_off() {
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    for name in ["a.log", "b.log", "c.log"] {
        app.open_log_file(write(dir.path(), name, LOG));
    }
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    app.config.global_filter = GlobalFilter {
        enabled: true,
        exclude: vec!["healthcheck".into()],
        ..Default::default()
    };
    app.apply_global_filter();
    frame(&mut app, &ctx, vec![]);
    for engine in &app.engines {
        assert!(engine.global_filter().is_some(), "badge on every stream");
        assert!(!visible(engine).contains(&1));
        assert_eq!(engine.exclude_filter(), "", "own fields untouched");
    }
    // A stream opened later is filtered from its first frame.
    app.open_log_file(write(dir.path(), "d.log", LOG));
    frame(&mut app, &ctx, vec![]);
    let later = app
        .engines
        .iter()
        .find(|e| e.path.ends_with("d.log"))
        .unwrap();
    assert!(!visible(later).contains(&1));

    // Switched off: own filtering only, terms kept, also across a restart.
    app.config.global_filter.enabled = false;
    app.apply_global_filter();
    frame(&mut app, &ctx, vec![]);
    assert!(app.engines.iter().all(|e| e.global_filter().is_none()));
    let restored = FastTailConfig::from_ini(&app.config.to_ini());
    assert!(!restored.global_filter.enabled);
    assert_eq!(
        restored.global_filter.exclude,
        vec!["healthcheck".to_string()]
    );
}

#[test]
fn an_apply_without_a_real_change_refilters_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    app.config.global_filter = GlobalFilter {
        enabled: true,
        exclude: vec!["healthcheck".into()],
        ..Default::default()
    };
    app.apply_global_filter();
    let first = app.global_spec.clone().expect("applied");
    // An empty row added, the bar closed, a term typed then deleted: same filter.
    app.config.global_filter.include.push(String::new());
    app.config.global_filter.bar_open = false;
    app.apply_global_filter();
    app.config.global_filter.exclude[0].push('x');
    app.config.global_filter.exclude[0].pop();
    app.apply_global_filter();
    assert!(Arc::ptr_eq(&first, app.global_spec.as_ref().unwrap()));
    // A real change does compile a new set.
    app.config.global_filter.case_sensitive = true;
    app.apply_global_filter();
    assert!(!Arc::ptr_eq(&first, app.global_spec.as_ref().unwrap()));
}

#[test]
fn ctrl_shift_h_shows_and_hides_the_bar_and_typing_waits_for_a_pause() {
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    app.open_log_file(write(dir.path(), "a.log", LOG));
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let shortcut = egui::Event::Key {
        key: egui::Key::H,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    };
    frame(&mut app, &ctx, vec![shortcut.clone()]);
    assert!(app.config.global_filter.bar_open);
    app.config.global_filter.enabled = true;
    app.apply_global_filter();

    // Type an exclude term in the bar: applied only once typing pauses.
    let id = fasttail::ui::global_filter_bar::term_id(true, 0);
    let pos = ctx
        .read_response(id)
        .expect("the bar is drawn")
        .rect
        .center();
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(pos), press(true)],
    );
    frame(&mut app, &ctx, vec![press(false)]);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("healthcheck".into())],
    );
    assert_eq!(
        app.config.global_filter.exclude,
        vec!["healthcheck".to_string()]
    );
    assert!(app.engines[0].global_filter().is_none(), "not while typing");
    let started = Instant::now();
    while app.engines[0].global_filter().is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "applied after the pause"
        );
        std::thread::sleep(Duration::from_millis(50));
        frame(&mut app, &ctx, vec![]);
    }
    assert!(!visible(&app.engines[0]).contains(&1));

    frame(&mut app, &ctx, vec![shortcut]);
    assert!(!app.config.global_filter.bar_open, "hidden again");
    assert!(
        app.engines[0].global_filter().is_some(),
        "hiding keeps it on"
    );
}
