// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Issue #161: a minimised or hidden window draws nothing, so the streams are read by
//! `FastTailApp::background_step` (called from `logic`).

use super::write_lines;
use fasttail::audio::SoundAlertPreset;
use fasttail::config::FastTailConfig;
use fasttail::tail_engine::HighlightRule;
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::time::{Duration, Instant};

#[test]
fn a_window_that_draws_nothing_still_reads_the_streams_and_asks_for_attention() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["INFO start"]);
    let mut rule = HighlightRule::new("boom", [255, 0, 0], [0, 0, 0], false);
    rule.sound_alert = SoundAlertPreset::Critical;
    let mut app = FastTailApp::from_config(FastTailConfig {
        highlight_rules: vec![rule],
        flash_on_alert: true,
        sound_enabled: false,
        size_check_interval_ms: 0,
        ..FastTailConfig::default()
    });
    app.open_log_file(log.clone());
    let ctx = egui::Context::default();
    app.background_step(&ctx);
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"ERROR boom\nINFO after\n").unwrap();
    drop(f);
    let started = Instant::now();
    while app.engines[0].total_lines() < 3 {
        assert!(started.elapsed() < Duration::from_secs(10), "never read");
        app.background_step(&ctx);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(app.engines[0].unseen_severity >= 2, "the alert is counted");
    assert!(app.attention_requested, "and asks for attention once");
}
