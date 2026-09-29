// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn test_level_detection_through_the_engine() {
    use fasttail::log_level::LogLevel;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("levels.log");
    write_level_log(&log);
    let engine = TailEngine::open(&log).unwrap();
    assert!(engine.levels_complete(), "small file: cache filled on open");
    let expected = [
        LogLevel::Info,
        LogLevel::Debug,
        LogLevel::Warn,
        LogLevel::Error,
        LogLevel::Unknown,
        LogLevel::Unknown,
        LogLevel::Unknown,
        LogLevel::Fatal,
        LogLevel::Trace,
        LogLevel::Info,
    ];
    for (idx, level) in expected.iter().enumerate() {
        assert_eq!(engine.level_of(idx), *level, "line {idx}");
    }
    assert_eq!(engine.level_count(LogLevel::Info), 2);
    assert_eq!(engine.level_count(LogLevel::Unknown), 3);
    assert_eq!(engine.level_count(LogLevel::Fatal), 1);
    assert_eq!(engine.level_count(LogLevel::Error), 1);
    assert_eq!(engine.level_count(LogLevel::Warn), 1);
    assert_eq!(engine.level_count(LogLevel::Debug), 1);
    assert_eq!(engine.level_count(LogLevel::Trace), 1);
}

#[test]
fn test_level_cache_follows_appends_and_truncation() {
    use fasttail::log_level::LogLevel;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("levels_append.log");
    write_level_log(&log);
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.level_count(LogLevel::Error), 1);

    // A partial last line completes on the next poll: its level is re-detected.
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"2026-09-18 12:00:06 ERR").unwrap();
    f.flush().unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 11);
    assert_eq!(engine.level_of(10), LogLevel::Unknown);
    f.write_all(b"OR late failure\n2026-09-18 12:00:07 WARN late\n")
        .unwrap();
    f.flush().unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 12);
    assert!(engine.levels_complete());
    assert_eq!(engine.level_of(10), LogLevel::Error);
    assert_eq!(engine.level_of(11), LogLevel::Warn);
    assert_eq!(engine.level_count(LogLevel::Error), 2);
    assert_eq!(engine.level_count(LogLevel::Warn), 2);
    assert_eq!(engine.level_count(LogLevel::Unknown), 3);

    // Truncation resets cache and counters together with the index.
    std::fs::write(&log, b"2026-09-18 13:00:00 [FATAL] restart\n").unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 1);
    assert_eq!(engine.level_of(0), LogLevel::Fatal);
    assert_eq!(engine.level_count(LogLevel::Fatal), 1);
    assert_eq!(engine.level_count(LogLevel::Error), 0);
    assert_eq!(engine.level_count(LogLevel::Warn), 0);
    assert_eq!(engine.level_count(LogLevel::Unknown), 0);
}

#[test]
fn test_min_level_filter_and_unknown_toggle() {
    use fasttail::log_level::LogLevel;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("levels_filter.log");
    write_level_log(&log);
    let mut engine = TailEngine::open(&log).unwrap();
    assert!(!engine.is_filter_active());

    // >= WARN: WARN, ERROR (+ its stack trace, following the parent), FATAL.
    engine.set_min_level(LogLevel::Warn);
    assert!(engine.is_filter_active());
    assert_eq!(engine.filtered_lines, vec![2, 3, 4, 5, 7]);
    assert_eq!(engine.visible_line_count(), 5);
    assert!(
        engine.is_line_visible(4),
        "continuation follows its ERROR parent"
    );
    assert!(
        !engine.is_line_visible(6),
        "unknown level hidden by default"
    );

    // Unknown-level lines can be let through.
    engine.set_show_unknown_levels(true);
    assert_eq!(engine.filtered_lines, vec![2, 3, 4, 5, 6, 7]);
    engine.set_show_unknown_levels(false);

    // TRACE threshold: everything with a level, and unknown lines too.
    engine.set_min_level(LogLevel::Trace);
    assert_eq!(engine.visible_line_count(), 10);

    // Combined with include/exclude: exclude wins, include narrows, level narrows again.
    engine.set_min_level(LogLevel::Info);
    engine.set_include_filter("2026");
    engine.set_exclude_filter("payment");
    assert_eq!(engine.filtered_lines, vec![0, 2], "TRACE is below INFO");
    engine.set_min_level(LogLevel::Warn);
    assert_eq!(engine.filtered_lines, vec![2]);

    // Off again: no filter at all.
    engine.set_include_filter("");
    engine.set_exclude_filter("");
    engine.set_min_level(LogLevel::Unknown);
    assert!(!engine.is_filter_active());
    assert_eq!(engine.visible_line_count(), 10);
}

#[test]
fn test_min_level_filter_matches_spec_counts() {
    use fasttail::log_level::LogLevel;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("levels_counts.log");
    let mut f = std::fs::File::create(&log).unwrap();
    for i in 0..107 {
        let level = match i % 107 {
            0..=99 => "INFO",
            100..=104 => "WARN",
            _ => "ERROR",
        };
        writeln!(f, "2026-09-18 12:00:00 [{level}] line {i}").unwrap();
    }
    drop(f);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_min_level(LogLevel::Warn);
    assert_eq!(engine.visible_line_count(), 7);
    assert_eq!(engine.level_count(LogLevel::Warn), 5);
    assert_eq!(engine.level_count(LogLevel::Error), 2);
    assert_eq!(engine.level_count(LogLevel::Info), 100);
}

#[test]
fn test_background_level_filter_equals_synchronous() {
    use fasttail::log_level::LogLevel;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bg_levels.log");
    write_scenario_log(&log, 40_000);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_min_level(LogLevel::Warn);
    sync.set_exclude_filter("svc-3");
    let expected = sync.filtered_lines.clone();
    assert_eq!(
        expected.len(),
        40_000 * 3 / 10 - expected_svc3_warn_or_error(40_000)
    );

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.set_min_level(LogLevel::Warn);
    bg.set_exclude_filter("svc-3");
    assert!(bg.scan_progress().is_some(), "large-file path spawns a job");
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, expected);
    assert_eq!(bg.visible_line_count(), expected.len());
}

#[test]
fn test_background_level_scan_fills_cache_and_counters() {
    use fasttail::log_level::LogLevel;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bg_level_scan.log");
    write_scenario_log(&log, 50_000);

    let sync = TailEngine::open(&log).unwrap();
    assert!(sync.levels_complete());

    // Threshold 0: the level scan runs on a worker thread right after the index.
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert!(
        matches!(
            bg.scan_progress(),
            Some((fasttail::scan_job::ScanKind::Levels, _, _))
        ),
        "large-file path detects levels in the background"
    );
    // A filter typed meanwhile takes over; the level scan resumes afterwards.
    bg.set_include_filter("ERROR");
    wait_for_jobs(&mut bg);
    assert!(bg.levels_complete());
    assert_eq!(bg.level_count(LogLevel::Error), 5_000);
    assert_eq!(bg.level_count(LogLevel::Warn), 10_000);
    assert_eq!(bg.level_count(LogLevel::Info), 35_000);
    assert_eq!(bg.level_count(LogLevel::Unknown), 0);
    for idx in [0usize, 1, 3, 49_999] {
        assert_eq!(bg.level_of(idx), sync.level_of(idx));
    }
    assert_eq!(bg.filtered_lines.len(), 5_000);

    // Background index too: levels arrive after the index job.
    let mut bg2 = TailEngine::open_with_thresholds(&log, 0, 0).unwrap();
    wait_for_jobs(&mut bg2);
    assert!(bg2.levels_complete());
    assert_eq!(bg2.level_count(LogLevel::Error), 5_000);
}

#[test]
fn test_appends_during_a_level_scan_are_detected_afterwards() {
    use fasttail::log_level::LogLevel;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("append_levels.log");
    write_scenario_log(&log, 60_000);

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    assert!(bg.scan_progress().is_some());
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"late FATAL one\nlate INFO two\nlate FATAL three\n")
        .unwrap();
    drop(f);
    bg.poll_updates();
    wait_for_jobs(&mut bg);
    assert_eq!(bg.total_lines(), 60_003);
    assert!(bg.levels_complete());
    assert_eq!(bg.level_count(LogLevel::Fatal), 2);
    assert_eq!(bg.level_of(60_000), LogLevel::Fatal);
    assert_eq!(bg.level_of(60_001), LogLevel::Info);
    let total: u64 = LogLevel::ALL
        .iter()
        .map(|l| bg.level_count(*l))
        .sum::<u64>()
        + bg.level_count(LogLevel::Unknown);
    assert_eq!(total, 60_003);
}

#[test]
fn test_empty_state_rendering_when_no_tabs_open() {
    use fasttail::config::FastTailConfig;
    use fasttail::ui::FastTailApp;

    let mut config = FastTailConfig::default();
    config.open_files.clear();
    config.dock_layout = None;

    let mut app = FastTailApp::from_config(config);
    assert_eq!(app.dock_state.iter_all_tabs().count(), 0);

    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        app.render_ui(ui);
    });
    output.textures_delta.clear();
}

#[test]
fn test_level_i18n_keys() {
    for lang in [
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        for key in [
            "min_level",
            "min_level_off",
            "min_level_tip",
            "show_unknown_levels",
            "show_unknown_levels_tip",
            "level_colors",
            "level_colors_tip",
            "scan_levels",
            "level_counts_tip",
            "help_filter_levels",
        ] {
            assert_ne!(t(lang, key), "Unknown", "{key} missing for {lang:?}");
        }
    }
}
