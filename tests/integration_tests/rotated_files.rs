// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::rotation;
use fasttail::stdin_source::Settings;
use fasttail::tail_engine::TailEngine;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn gz(path: &Path, text: &str) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    enc.write_all(text.as_bytes()).unwrap();
    enc.finish().unwrap();
}

fn open(live: &Path, spool: &Path) -> TailEngine {
    let mut engine =
        rotation::open_engine(live, &Settings::from_config(Some(spool), 64), None).unwrap();
    engine.size_check_interval = Duration::ZERO;
    engine
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
fn pieces_are_read_oldest_first_then_the_live_file() {
    let dir = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    gz(&dir.path().join("app.log.2.gz"), "two-a\ntwo-b\n");
    // No final line break: the next piece still starts a line.
    std::fs::write(dir.path().join("app.log.1"), "one-a\none-b").unwrap();
    std::fs::write(dir.path().join("app.log"), "live-a\n").unwrap();
    std::fs::write(dir.path().join("other.log.1"), "not mine\n").unwrap();
    let live = dir.path().join("app.log");
    assert_eq!(rotation::offer(&live).map(|(n, _)| n), Some(2));
    let mut engine = open(&live, spool.path());
    assert_eq!(engine.path, rotation::identity(&live));
    assert_eq!(engine.source_title().as_deref(), Some("app.log (+2)"));
    wait(&mut engine, |e| e.total_lines() >= 5);
    assert_eq!(
        lines(&engine),
        vec!["two-a", "two-b", "one-a", "one-b", "live-a"]
    );
    let pieces = engine.rotation_pieces();
    assert_eq!(
        pieces,
        vec![
            (0, "app.log.2.gz".to_string()),
            (2, "app.log.1".to_string()),
            (4, "app.log".to_string())
        ]
    );
    // The live file is followed.
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&live)
        .unwrap();
    writeln!(f, "live-b").unwrap();
    drop(f);
    wait(&mut engine, |e| e.total_lines() >= 6);
    assert_eq!(lines(&engine).last().unwrap(), "live-b");
}

#[test]
fn a_rotation_while_following_keeps_every_line() {
    let dir = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    let live = dir.path().join("svc.log");
    std::fs::write(&live, "before-1\nbefore-2\n").unwrap();
    std::fs::write(dir.path().join("svc.log.1"), "old\n").unwrap();
    let mut engine = open(&live, spool.path());
    wait(&mut engine, |e| e.total_lines() >= 3);
    // The application rotates: the live file becomes svc.log.1, a new svc.log starts.
    std::fs::rename(dir.path().join("svc.log.1"), dir.path().join("svc.log.2")).unwrap();
    std::fs::rename(&live, dir.path().join("svc.log.1")).unwrap();
    std::fs::write(&live, "after-1\n").unwrap();
    wait(&mut engine, |e| e.total_lines() >= 4);
    assert_eq!(
        lines(&engine),
        vec!["old", "before-1", "before-2", "after-1"]
    );
}

#[test]
fn rotated_identities_open_through_open_target() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("web.log.1"), "a\n").unwrap();
    std::fs::write(dir.path().join("web.log"), "b\n").unwrap();
    let config = fasttail::config::FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let id = rotation::identity(&dir.path().join("web.log"));
    match fasttail::workspace::open_target(&id, &config, None) {
        fasttail::workspace::OpenOutcome::Opened(engine) => {
            assert!(engine.rotation.is_some());
            assert_eq!(engine.path, id);
        }
        _ => panic!("the rotated stream opens"),
    }
    // With `rotations=always` a plain path opens with its rotations too.
    let always = fasttail::config::FastTailConfig {
        rotations: rotation::Policy::Always,
        ..config
    };
    match fasttail::workspace::open_target(&dir.path().join("web.log"), &always, None) {
        fasttail::workspace::OpenOutcome::Opened(engine) => assert_eq!(engine.path, id),
        _ => panic!("opens"),
    }
}

#[test]
fn the_window_offers_the_rotations_and_keeps_the_filters() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("api.log.1"), "old ERROR a\nold INFO b\n").unwrap();
    std::fs::write(dir.path().join("api.log"), "new ERROR c\n").unwrap();
    let live = dir.path().join("api.log");
    let mut app = fasttail::ui::app::FastTailApp::from_config(fasttail::config::FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    });
    app.open_log_file(live.clone());
    assert_eq!(app.engines[0].rotation_offer.map(|(n, _)| n), Some(1));
    app.engines[0].set_include_filter("ERROR");
    app.engines[0].rotation_request = true;
    app.apply_merged_views();
    assert_eq!(app.engines.len(), 1, "the same tab");
    let engine = &mut app.engines[0];
    assert_eq!(engine.path, rotation::identity(&live));
    assert_eq!(engine.include_filter(), "ERROR");
    wait(engine, |e| e.total_lines() >= 3);
    assert!(app
        .config
        .open_files
        .iter()
        .any(|p| *p == rotation::identity(&live)));
}
