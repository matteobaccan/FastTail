// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;
use fasttail::log_level::LogLevel;

/// Lines whose level sits at byte 150, after a long prefix: one ERROR in three.
fn write_late_levels(path: &std::path::Path, lines: usize) {
    let prefix = "host=web-01 pid=4242 thread=pool-7-worker-3 class=com.example.billing.Invoice";
    let prefix = format!("{prefix:<150}");
    let mut f = std::fs::File::create(path).unwrap();
    for i in 0..lines {
        let level = if i % 3 == 0 { "ERROR" } else { "INFO" };
        writeln!(f, "{prefix}{level} event {i}").unwrap();
    }
}

#[test]
fn a_level_past_the_window_is_found_once_the_stream_widens_it() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("late.log");
    write_late_levels(&log, 300);
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.level_window(), 96);
    assert_eq!(engine.level_of(0), LogLevel::Unknown);
    assert_eq!(engine.level_count(LogLevel::Unknown), 300);
    engine.set_min_level(LogLevel::Warn);
    assert_eq!(engine.visible_line_count(), 0);

    engine.set_level_override(Some(256));
    assert!(engine.levels_complete());
    assert_eq!(engine.level_of(0), LogLevel::Error);
    assert_eq!(engine.level_count(LogLevel::Error), 100);
    assert_eq!(engine.level_count(LogLevel::Info), 200);
    assert_eq!(engine.level_count(LogLevel::Unknown), 0);
    assert_eq!(
        engine.visible_line_count(),
        100,
        ">= WARN shows the ERROR lines"
    );

    // Back to the setting: unknown again, the filter follows.
    engine.set_level_override(None);
    assert_eq!(engine.level_count(LogLevel::Unknown), 300);
    assert_eq!(engine.visible_line_count(), 0);
}

#[test]
fn a_large_file_detects_again_in_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("late_bg.log");
    write_late_levels(&log, 30_000);
    // Threshold 0: every level scan runs on a worker.
    let mut engine = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut engine);
    assert_eq!(engine.level_count(LogLevel::Unknown), 30_000);
    engine.set_min_level(LogLevel::Error);
    wait_for_jobs(&mut engine);
    assert_eq!(engine.visible_line_count(), 0);

    engine.set_level_override(Some(256));
    assert!(
        !engine.levels_complete(),
        "the old levels are dropped before the new ones arrive"
    );
    wait_for_jobs(&mut engine);
    assert!(engine.levels_complete());
    assert_eq!(engine.level_count(LogLevel::Error), 10_000);
    assert_eq!(engine.level_count(LogLevel::Unknown), 0);
    assert_eq!(engine.visible_line_count(), 10_000);
}

#[test]
fn the_override_is_saved_only_when_it_differs_from_the_setting() {
    use fasttail::workspace::{apply_settings, save_changes};
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    write_late_levels(&a, 3);
    write_late_levels(&b, 3);
    let mut cfg = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..FastTailConfig::default()
    };
    assert_eq!(cfg.level_detect_bytes, 96);
    let mut ea = TailEngine::open(&a).unwrap();
    let mut eb = TailEngine::open(&b).unwrap();
    apply_settings(&mut ea, &cfg);
    apply_settings(&mut eb, &cfg);
    ea.set_level_override(Some(256));
    eb.set_level_override(Some(96));
    assert_eq!(
        eb.level_override(),
        None,
        "the setting's own value is no override"
    );
    assert!(save_changes(&mut ea, &mut cfg, false));
    save_changes(&mut eb, &mut cfg, false);
    assert_eq!(cfg.stream_state_for(&a).unwrap().level_bytes, Some(256));
    assert!(cfg
        .stream_state_for(&b)
        .is_none_or(|s| s.level_bytes.is_none()));
    // The stream sections are written for the open files.
    cfg.open_files = vec![a.clone(), b.clone()];
    let file = dir.path().join("fasttail.ini");
    cfg.save_to(&file).unwrap();
    let ini = std::fs::read_to_string(&file).unwrap();
    assert_eq!(ini.matches("level_bytes=").count(), 1, "{ini}");
    assert!(ini.contains("level_bytes=256"), "{ini}");
    assert!(ini.contains("level_detect_bytes=96"), "{ini}");

    // Read back: the global value and the stream's own.
    let back = from_text(&ini);
    let mut restored = TailEngine::open(&a).unwrap();
    fasttail::workspace::restore_stream(&mut restored, &back, &a);
    assert_eq!(restored.level_window(), 256);
    assert_eq!(restored.level_of(0), LogLevel::Error);

    // A setting raised to the stream's value takes the override's place.
    cfg.level_detect_bytes = 256;
    apply_settings(&mut ea, &cfg);
    assert_eq!((ea.level_window(), ea.level_override()), (256, None));
    assert!(ea.level_bytes_dirty);
}

#[test]
fn the_setting_round_trips_and_is_kept_in_range() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fasttail.ini");
    FastTailConfig {
        level_detect_bytes: 300,
        ..FastTailConfig::default()
    }
    .save_to(&file)
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(from_text(&text).level_detect_bytes, 300);
    let with = |v: &str| text.replace("level_detect_bytes=300", v);
    assert_eq!(
        from_text(&with("level_detect_bytes=8")).level_detect_bytes,
        16
    );
    assert_eq!(
        from_text(&with("level_detect_bytes=99999")).level_detect_bytes,
        4096
    );
    // A file from an older build has no key: today's 96 bytes.
    assert_eq!(from_text(&with("")).level_detect_bytes, 96);
}

fn from_text(text: &str) -> FastTailConfig {
    FastTailConfig::from_ini(&ini::Ini::load_from_str(text).unwrap())
}
