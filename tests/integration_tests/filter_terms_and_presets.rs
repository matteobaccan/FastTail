// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_scenario_log};
use fasttail::ansi::AnsiMode;
use fasttail::config::FastTailConfig;
use fasttail::filter_preset::{preset_label, FilterPreset, FilterState, PresetLabel};
use fasttail::log_level::LogLevel;
use fasttail::session::{Session, SESSION_SUFFIX};
use fasttail::tail_engine::TailEngine;
use std::path::{Path, PathBuf};

fn log_with(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    std::fs::write(&path, text).unwrap();
    (dir, path)
}

fn terms(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

fn visible(engine: &TailEngine) -> Vec<usize> {
    (0..engine.total_lines())
        .filter(|i| engine.is_line_visible(*i))
        .collect()
}

const PAYMENTS: &str = "payment timeout retry=1\n\
                            payment timeout healthcheck\n\
                            payment ok\n\
                            timeout only\n\
                            payment timeout retry=0\n\
                            PAYMENT TIMEOUT upper\n";

#[test]
fn include_terms_are_anded_and_exclude_terms_ored() {
    let (_dir, path) = log_with(PAYMENTS);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_filter_terms(
        terms(&["payment", "timeout"]),
        terms(&["healthcheck", "retry=0"]),
    );
    assert_eq!(engine.filtered_lines, vec![0, 5]);
    assert_eq!(visible(&engine), vec![0, 5], "per-line check agrees");
    // The shared toggles apply to every term.
    engine.filter_case_sensitive = true;
    engine.refresh_filters();
    assert_eq!(engine.filtered_lines, vec![0]);
    assert_eq!(engine.include_filter(), "payment");
    assert_eq!(engine.exclude_filter(), "healthcheck");
    assert_eq!(engine.extra_include_terms(), 1);
    assert_eq!(engine.extra_exclude_terms(), 1);
}

#[test]
fn empty_rows_are_ignored_and_the_bar_edits_the_first_term() {
    let (_dir, path) = log_with(PAYMENTS);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_filter_terms(terms(&["", "payment", ""]), terms(&[""]));
    assert!(engine.is_filter_active());
    assert_eq!(engine.filtered_lines, vec![0, 1, 2, 4, 5]);
    assert_eq!(engine.extra_include_terms(), 1, "the +1 badge");
    // Typing in the stream bar's field replaces the first row only.
    engine.set_include_filter("timeout");
    assert_eq!(engine.include_terms(), terms(&["timeout", "payment", ""]));
    assert_eq!(engine.filtered_lines, vec![0, 1, 4, 5]);
    engine.set_filter_terms(terms(&["", ""]), terms(&["", ""]));
    assert!(!engine.is_filter_active(), "only empty rows: no filter");
}

#[test]
fn operator_characters_are_matched_as_typed() {
    let (_dir, path) = log_with("a && !b here\na and b\na && b\n");
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_include_filter("a && !b");
    assert_eq!(engine.filtered_lines, vec![0]);
}

#[test]
fn three_include_terms_show_a_plus_two_badge() {
    let (_dir, path) = log_with(PAYMENTS);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_filter_terms(terms(&["payment", "timeout", "retry"]), vec![]);
    assert_eq!(engine.include_filter(), "payment");
    assert_eq!(engine.extra_include_terms(), 2);
    assert_eq!(engine.filtered_lines, vec![0, 4]);
}

#[test]
fn terms_are_capped_at_eight_per_side() {
    let (_dir, path) = log_with(PAYMENTS);
    let mut engine = TailEngine::open(&path).unwrap();
    let many: Vec<String> = (0..12).map(|i| format!("t{i}")).collect();
    engine.set_filter_terms(many.clone(), many);
    assert_eq!(engine.include_terms().len(), 8);
    assert_eq!(engine.exclude_terms().len(), 8);
}

#[test]
fn continuation_lines_follow_their_parent_unless_excluded() {
    let text = "ERROR payment timeout\n\
                    \x20   at Pay.run(Pay.java:10)\n\
                    \x20   at Pay.retry(Pay.java:20) healthcheck\n\
                    ERROR payment ok\n\
                    \x20   at Other.run(Other.java:5)\n";
    let (_dir, path) = log_with(text);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_filter_terms(terms(&["payment", "timeout"]), terms(&["healthcheck"]));
    assert_eq!(engine.filtered_lines, vec![0, 1]);
    assert_eq!(visible(&engine), vec![0, 1]);
}

#[test]
fn an_invalid_regex_term_is_flagged_and_matches_nothing() {
    let (_dir, path) = log_with(PAYMENTS);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.filter_is_regex = true;
    engine.set_filter_terms(terms(&["payment", "("]), vec![]);
    assert!(!engine.include_term_invalid(0));
    assert!(engine.include_term_invalid(1));
    assert!(
        engine.filtered_lines.is_empty(),
        "an include that cannot match"
    );
    engine.set_filter_terms(terms(&["payment"]), terms(&["(", "ok$"]));
    assert!(engine.exclude_term_invalid(0));
    assert!(!engine.exclude_term_invalid(1));
    assert_eq!(
        engine.filtered_lines,
        vec![0, 1, 4, 5],
        "an invalid exclude hides nothing, the valid one still does"
    );
}

/// A coloured log: every 10th line an error, colour codes around the level.
fn write_coloured_log(path: &Path, lines: usize) {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 0..lines {
        let (colour, level) = match i % 10 {
            0 => (31, "ERROR"),
            1 | 2 => (33, "WARN"),
            _ => (32, "INFO"),
        };
        writeln!(
            f,
            "2026-09-19 10:{:02}:{:02} \x1b[{colour}m{level}\x1b[0m payment svc-{} req={i}",
            (i / 60) % 60,
            i % 60,
            i % 7
        )
        .unwrap();
    }
}

/// Applies the same combined filter (terms, level, time window) to an engine.
fn combined(engine: &mut TailEngine) {
    engine.set_min_level(LogLevel::Warn);
    engine.set_filter_terms(terms(&["payment", "svc-"]), terms(&["svc-3", "req=1"]));
    engine.apply_time_range_text("10:05", "10:40");
}

#[test]
fn background_scans_agree_with_the_synchronous_path() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bg.log");
    write_scenario_log(&log, 40_000);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_filter_terms(terms(&["svc-", "WARN"]), terms(&["svc-3", "req=1"]));
    assert!(sync.scan_progress().is_none());
    let expected = sync.filtered_lines.clone();
    assert!(!expected.is_empty());
    sync.update_search("payload pp");
    let expected_search = sync.search_matches.clone();
    assert!(!expected_search.is_empty());

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.set_filter_terms(terms(&["svc-", "WARN"]), terms(&["svc-3", "req=1"]));
    assert!(bg.scan_progress().is_some(), "large-file path spawns a job");
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, expected);
    bg.update_search("payload pp");
    wait_for_jobs(&mut bg);
    assert_eq!(bg.search_matches, expected_search, "search over the filter");
}

#[test]
fn background_scans_agree_with_ansi_strip_level_and_time_window() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("coloured.log");
    write_coloured_log(&log, 30_000);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_ansi_mode(AnsiMode::Strip);
    combined(&mut sync);
    wait_for_jobs(&mut sync);
    let expected = sync.filtered_lines.clone();
    assert!(!expected.is_empty());
    assert!(sync.is_time_filtered());

    let mut bg = TailEngine::open_with_thresholds(&log, 0, 0).unwrap();
    wait_for_jobs(&mut bg);
    bg.set_ansi_mode(AnsiMode::Strip);
    wait_for_jobs(&mut bg);
    combined(&mut bg);
    wait_for_jobs(&mut bg);
    assert!(bg.is_time_filtered());
    assert_eq!(bg.filtered_lines, expected);
    // The colour code sits between the level and the text: a term spanning it only
    // matches once the codes are stripped.
    bg.set_filter_terms(terms(&["WARN payment"]), terms(&["svc-3"]));
    wait_for_jobs(&mut bg);
    sync.set_filter_terms(terms(&["WARN payment"]), terms(&["svc-3"]));
    wait_for_jobs(&mut sync);
    assert!(!sync.filtered_lines.is_empty());
    assert_eq!(bg.filtered_lines, sync.filtered_lines);
}

fn payment_errors() -> FilterPreset {
    FilterPreset {
        name: "payment errors".to_string(),
        state: FilterState {
            include: terms(&["payment"]),
            exclude: terms(&["healthcheck"]),
            min_level: LogLevel::Warn,
            ..Default::default()
        },
    }
}

#[test]
fn presets_round_trip_through_the_ini_in_order() {
    let mut timed = payment_errors();
    timed.name = "Timed".to_string();
    timed.state.include.push("timeout".to_string());
    timed.state.case_sensitive = true;
    timed.state.is_regex = true;
    timed.state.show_unknown_levels = true;
    timed.state.time = Some(("14:02".to_string(), "14:05".to_string()));
    let cfg = FastTailConfig {
        filter_presets: vec![timed.clone(), payment_errors()],
        ..Default::default()
    };
    let ini = cfg.to_ini();
    let sec = ini.section(Some("filter_preset.0")).unwrap();
    assert_eq!(sec.get("include.2"), Some("timeout"));
    assert_eq!(sec.get("min_level"), Some("WARN"));
    assert_eq!(sec.get("time_from"), Some("14:02"));
    assert!(
        ini.section(Some("filter_preset.1"))
            .unwrap()
            .get("time_from")
            .is_none(),
        "no time range saved: no time keys"
    );

    let loaded = FastTailConfig::from_ini(&ini);
    assert_eq!(loaded.filter_presets, vec![timed, payment_errors()]);
}

#[test]
fn preset_terms_keep_quotes_and_edge_spaces_in_fasttail_ini() {
    let tricky = terms(&[
        "\"status\":500",
        " ERROR ",
        "'user'",
        "it's \"x\" 'y'",
        "''\"",
        "\tpad\\d+\t",
        "a\\\\b",
    ]);
    let mut preset = payment_errors();
    preset.state.include = tricky.clone();
    preset.state.exclude = tricky;
    let cfg = FastTailConfig {
        filter_presets: vec![preset.clone()],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fasttail.ini");
    cfg.save_to(&path).unwrap();
    let loaded = FastTailConfig::from_ini(&ini::Ini::load_from_file(&path).unwrap());
    assert_eq!(loaded.filter_presets, vec![preset]);
}

#[test]
fn preset_sections_take_defaults_and_skip_duplicate_names() {
    let mut ini = ini::Ini::new();
    ini.with_section(Some("filter_preset.0"))
        .set("name", "Errors")
        .set("include.1", "ERROR");
    ini.with_section(Some("filter_preset.1"))
        .set("name", "errors")
        .set("include.1", "other");
    ini.with_section(Some("filter_preset.2"))
        .set("include.1", "nameless");
    ini.with_section(Some("filter_preset.3"))
        .set("name", "Bare");
    let loaded = FastTailConfig::from_ini(&ini);
    let names: Vec<&str> = loaded
        .filter_presets
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["Errors", "Bare"]);
    let bare = &loaded.filter_presets[1].state;
    assert_eq!(bare, &FilterState::default());
}

#[test]
fn applying_a_preset_sets_the_whole_state_and_the_label_follows_edits() {
    let (_dir, path) =
        log_with("WARN payment slow\nERROR payment healthcheck\nINFO payment ok\nERROR other\n");
    let presets = vec![payment_errors()];
    let mut engine = TailEngine::open(&path).unwrap();
    assert_eq!(preset_label(&presets, &engine), PresetLabel::None);

    let generation = engine.filter_generation;
    presets[0].apply_to(&mut engine);
    assert_eq!(
        engine.filter_generation,
        generation.wrapping_add(1),
        "one recomputation"
    );
    assert_eq!(engine.include_filter(), "payment");
    assert_eq!(engine.exclude_filter(), "healthcheck");
    assert_eq!(engine.min_level, LogLevel::Warn);
    assert_eq!(engine.filtered_lines, vec![0]);
    assert_eq!(
        preset_label(&presets, &engine),
        PresetLabel::Matches("payment errors")
    );

    engine.set_filter_terms(terms(&["payment", "timeout"]), terms(&["healthcheck"]));
    assert_eq!(
        preset_label(&presets, &engine),
        PresetLabel::Modified("payment errors")
    );
    // Back to the preset's state by hand: the name returns without the marker.
    engine.set_filter_terms(terms(&["payment", ""]), terms(&["healthcheck"]));
    assert_eq!(
        preset_label(&presets, &engine),
        PresetLabel::Matches("payment errors")
    );

    // A stream never given the preset shows its name once its state equals it.
    let mut other = TailEngine::open(&path).unwrap();
    other.set_min_level(LogLevel::Warn);
    other.set_filter_terms(terms(&["payment"]), terms(&["healthcheck"]));
    assert_eq!(
        preset_label(&presets, &other),
        PresetLabel::Matches("payment errors")
    );
    other.set_include_filter("pay");
    assert_eq!(preset_label(&presets, &other), PresetLabel::None);
}

#[test]
fn applying_to_all_streams_and_the_time_range_rules() {
    let (_dir, a) = log_with("2026-09-20 14:01:00 WARN payment a\n2026-09-20 14:03:00 WARN payment b\n2026-09-20 14:07:00 WARN payment c\n");
    let (_dir2, b) =
        log_with("2026-09-21 14:03:30 ERROR payment x\n2026-09-21 15:00:00 ERROR payment y\n");
    let mut engines = vec![TailEngine::open(&a).unwrap(), TailEngine::open(&b).unwrap()];
    for e in engines.iter_mut() {
        e.apply_time_range_text("14:00", "14:30");
    }

    // Without a time range the windows stay as they are.
    let plain = payment_errors();
    for e in engines.iter_mut() {
        plain.apply_to(e);
    }
    assert_eq!(engines[0].time_from_text, "14:00");
    assert_eq!(engines[0].filtered_lines, vec![0, 1, 2]);
    assert_eq!(engines[1].filtered_lines, vec![0]);

    // With one, a bare time follows the day of each log.
    let mut timed = payment_errors();
    timed.state.time = Some(("14:02".to_string(), "14:05".to_string()));
    for e in engines.iter_mut() {
        timed.apply_to(e);
        assert_eq!(e.applied_preset.as_deref(), Some("payment errors"));
    }
    let ms = |text: &str| {
        fasttail::timestamp::detect_timestamp(text, Default::default())
            .unwrap()
            .0
    };
    assert_eq!(engines[0].time_from, Some(ms("2026-09-20 14:02:00.000")));
    assert_eq!(engines[0].time_to, Some(ms("2026-09-20 14:05:59.999")));
    assert_eq!(engines[0].filtered_lines, vec![1]);
    assert_eq!(engines[1].time_from, Some(ms("2026-09-21 14:02:00.000")));
    assert_eq!(engines[1].filtered_lines, vec![0]);
    assert!(!engines[0].time_range_error);
}

#[test]
fn a_saved_state_captures_the_time_range_only_when_asked() {
    let (_dir, path) = log_with("2026-09-20 14:01:00 WARN payment a\n");
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_filter_terms(terms(&["payment", "", "a"]), terms(&[""]));
    engine.apply_time_range_text(" 14:00 ", "14:30");
    let without = FilterState::of_engine(&engine, false);
    assert_eq!(
        without.include,
        terms(&["payment", "a"]),
        "empty rows dropped"
    );
    assert!(without.exclude.is_empty());
    assert_eq!(without.time, None);
    let with = FilterState::of_engine(&engine, true);
    assert_eq!(with.time, Some(("14:00".to_string(), "14:30".to_string())));
    assert!(with.matches_engine(&engine));
}

#[test]
fn extra_terms_round_trip_and_old_files_load_with_one_term() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(&log, "a\n").unwrap();
    let mut entry = fasttail::session::StreamEntry::new(log.clone());
    entry.include_filter = "payment".to_string();
    entry.include_extra = terms(&["timeout", "eu-"]);
    entry.exclude_filter = "healthcheck".to_string();
    entry.exclude_extra = terms(&["retry=0"]);
    let session = Session {
        streams: vec![entry],
        dock_layout: None,
    };
    let file = dir.path().join(format!("terms{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("include=payment"), "{text}");
    assert!(text.contains("include.2=timeout"), "{text}");
    assert!(text.contains("include.3=eu-"), "{text}");
    assert!(text.contains("exclude.2=retry=0"), "{text}");
    assert_eq!(Session::load_from(&file).unwrap().session, session);

    // A file written before extra terms existed.
    let old = dir.path().join(format!("old{SESSION_SUFFIX}"));
    let mut ini = ini::Ini::new();
    ini.with_section(Some("session")).set("streams", "1");
    ini.with_section(Some("stream_0"))
        .set("path", log.to_string_lossy().to_string())
        .set("include", "ERROR")
        .set("exclude", "ping");
    ini.write_to_file(&old).unwrap();
    let loaded = Session::load_from(&old).unwrap().session;
    assert_eq!(loaded.streams[0].include_filter, "ERROR");
    assert_eq!(loaded.streams[0].exclude_filter, "ping");
    assert!(loaded.streams[0].include_extra.is_empty());
    assert!(loaded.streams[0].exclude_extra.is_empty());
}

#[test]
fn workspace_state_in_the_config_keeps_the_extra_terms() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(&log, "a\n").unwrap();
    let mut cfg = FastTailConfig {
        open_files: vec![log.clone()],
        ..Default::default()
    };
    let mut entry = fasttail::session::StreamEntry::new(log.clone());
    entry.include_filter = "payment".to_string();
    entry.include_extra = terms(&["timeout"]);
    cfg.set_stream_state(entry);
    let loaded = FastTailConfig::from_ini(&cfg.to_ini());
    let state = loaded.stream_state_for(&log).expect("stream state");
    assert_eq!(state.include_filter, "payment");
    assert_eq!(state.include_extra, terms(&["timeout"]));
}

#[test]
fn filters_window_renders_terms_and_presets_and_consumes_the_focus() {
    use fasttail::i18n::Language;
    use fasttail::theme::CyberTheme;
    use fasttail::ui::dock::{render_filters_content, PresetEvents};
    let (_dir, path) = log_with(PAYMENTS);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.filter_is_regex = true;
    engine.set_filter_terms(terms(&["payment", "("]), terms(&["healthcheck"]));
    let mut engines = vec![engine];
    let mut presets = vec![payment_errors()];
    let mut events = PresetEvents::default();
    let mut focus = Some(path.clone());
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(Default::default(), |ui| {
            render_filters_content(
                ui,
                &mut engines,
                &mut presets,
                &mut events,
                Some(&mut focus),
                &CyberTheme::Tron,
                Language::En,
            );
        });
        out.textures_delta.clear();
    }
    assert!(focus.is_none(), "the focused stream is shown once");
    assert!(!events.changed, "rendering alone changes no preset");
    assert_eq!(engines[0].include_terms(), terms(&["payment", "("]));
}
