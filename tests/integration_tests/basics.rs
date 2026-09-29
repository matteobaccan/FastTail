// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn test_tail_engine_open_and_stream() {
    let mut tmp = NamedTempFile::new().expect("create temp file");
    writeln!(tmp, "2026-09-16 08:30:00 [INFO] FastTail starting").unwrap();
    writeln!(tmp, "2026-09-16 08:30:01 [WARN] Slow connection detected").unwrap();
    writeln!(tmp, "2026-09-16 08:30:02 [ERROR] Deadlock encountered").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).expect("open tail engine");
    assert_eq!(engine.total_lines(), 3);
    assert_eq!(
        engine.get_line(0).as_deref(),
        Some("2026-09-16 08:30:00 [INFO] FastTail starting")
    );
    assert_eq!(
        engine.get_line(2).as_deref(),
        Some("2026-09-16 08:30:02 [ERROR] Deadlock encountered")
    );

    // Append new line to simulate growing log file
    writeln!(tmp, "2026-09-16 08:30:03 [INFO] Stream recovery ok").unwrap();
    tmp.flush().unwrap();

    // Poll updates
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 4);
    assert_eq!(
        engine.get_line(3).as_deref(),
        Some("2026-09-16 08:30:03 [INFO] Stream recovery ok")
    );
}

#[test]
fn test_tail_engine_atomic_save_replacement_detected() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("atomic_test.log");
    std::fs::write(&file_path, "line 1\nline 2\nline 3\n").unwrap();

    let mut engine = TailEngine::open(&file_path).unwrap();
    assert_eq!(engine.total_lines(), 3);

    // Simulate atomic save: write new file with new lines and replace original file
    let tmp_path = dir.path().join("atomic_test.tmp");
    std::fs::write(&tmp_path, "line 1\nline 2\nline 3\nline 4\nline 5\n").unwrap();
    let _ = std::fs::remove_file(&file_path);
    std::fs::rename(&tmp_path, &file_path).unwrap();

    engine.poll_updates();
    assert_eq!(engine.total_lines(), 5);
    assert_eq!(engine.get_line(3).as_deref(), Some("line 4"));
    assert_eq!(engine.get_line(4).as_deref(), Some("line 5"));
}

#[test]
fn test_tail_engine_filters() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "INFO: Worker 1 OK").unwrap();
    writeln!(tmp, "WARN: High memory").unwrap();
    writeln!(tmp, "ERROR: Crash on thread 3").unwrap();
    writeln!(tmp, "ERROR: healthcheck probe failed").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();

    // Include filter
    engine.set_include_filter("ERROR");
    assert!(!engine.matches_filter(&engine.get_line(0).unwrap()));
    assert!(!engine.matches_filter(&engine.get_line(1).unwrap()));
    assert!(engine.matches_filter(&engine.get_line(2).unwrap()));
    assert!(engine.matches_filter(&engine.get_line(3).unwrap()));

    // Include + Exclude filter (BareTailPro Filter Tail)
    engine.set_exclude_filter("healthcheck");
    assert!(engine.matches_filter(&engine.get_line(2).unwrap()));
    assert!(!engine.matches_filter(&engine.get_line(3).unwrap()));
}

#[test]
fn test_json_and_multiline_intelligence() {
    let json_line = "{\"timestamp\": \"2026-09-16\", \"level\": \"error\", \"code\": 500}";
    let regular_line = "2026-09-16 [INFO] standard text log";
    let stack_line = "  at com.example.Service.process(Service.java:42)";

    assert!(TailEngine::is_json_line(json_line));
    assert!(!TailEngine::is_json_line(regular_line));
    assert!(TailEngine::is_stacktrace_continuation(stack_line));
    assert!(!TailEngine::is_stacktrace_continuation(regular_line));
}

#[test]
fn test_themes_and_palettes() {
    for theme in &[
        CyberTheme::Tron,
        CyberTheme::Matrix,
        CyberTheme::Blade,
        CyberTheme::Light,
    ] {
        let bg = theme.bg_color();
        let border = theme.border_color();
        let accent = theme.accent_color();
        assert_ne!(bg, border);
        assert_ne!(accent, bg);
    }
}

#[test]
fn test_i18n_translations_and_fallback() {
    for lang in &[
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        let title = t(*lang, "follow_tail");
        assert!(!title.is_empty());
        assert_ne!(title, "Unknown");
    }

    // Verify fallback for undefined key
    assert_eq!(t(Language::It, "non_existent_key_xyz"), "Unknown");
}

#[test]
fn test_screensaver_idle_trigger() {
    let mut screensaver = MatrixScreensaver::default();
    assert!(!screensaver.is_active);

    // Inactivity check with 0 timeout (disabled)
    screensaver.check_inactivity(0, true, true);
    assert!(!screensaver.is_active);

    // Simulate idle by setting last_input_time in past
    screensaver.last_input_time = std::time::Instant::now() - Duration::from_secs(601);
    screensaver.check_inactivity(10, true, true);
    assert!(screensaver.is_active);

    // User input resets
    screensaver.on_user_input();
    assert!(!screensaver.is_active);
}

#[test]
fn test_config_serialization() {
    let mut config = FastTailConfig::default();
    config.theme = CyberTheme::Matrix;
    config.language = Language::It;
    config.screensaver_timeout_mins = 5;
    config.highlight_rules.push(HighlightRule::new(
        "CRITICAL",
        [255, 0, 0],
        [0, 0, 0],
        false,
    ));

    let toml_str = toml::to_string(&config).expect("serialize toml");
    let deserialized: FastTailConfig = toml::from_str(&toml_str).expect("deserialize toml");

    assert_eq!(deserialized.theme, CyberTheme::Matrix);
    assert_eq!(deserialized.language, Language::It);
    assert_eq!(deserialized.screensaver_timeout_mins, 5);
    assert!(deserialized
        .highlight_rules
        .iter()
        .any(|r| r.pattern == "CRITICAL"));
    assert_eq!(deserialized.borderless, false);
    assert_eq!(deserialized.show_line_numbers, true);
}

#[test]
fn test_is_watching_pause_and_resume() {
    let mut tmp = NamedTempFile::new().expect("create temp file");
    writeln!(tmp, "Line 1: Initial event").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).expect("open tail engine");
    assert_eq!(engine.total_lines(), 1);
    assert!(engine.is_watching);

    // Stop watching (simulate pause button)
    engine.is_watching = false;

    // Append line while watching is paused
    writeln!(tmp, "Line 2: Ignored while paused").unwrap();
    tmp.flush().unwrap();

    engine.poll_updates();
    // Total lines must still be 1 because watching is paused
    assert_eq!(engine.total_lines(), 1);

    // Resume watching
    engine.is_watching = true;
    engine.poll_updates();
    // Now it updates to 2 lines
    assert_eq!(engine.total_lines(), 2);
    assert_eq!(
        engine.get_line(1).as_deref(),
        Some("Line 2: Ignored while paused")
    );
}

#[test]
fn test_new_i18n_keys() {
    for lang in &[
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        assert_ne!(t(*lang, "borderless"), "Unknown");
        assert_ne!(t(*lang, "show_lines"), "Unknown");
        assert_ne!(t(*lang, "monitor_on"), "Unknown");
        assert_ne!(t(*lang, "monitor_off"), "Unknown");
        assert_ne!(t(*lang, "view_mode_filtered"), "Unknown");
        assert_ne!(t(*lang, "hex_columns"), "Unknown");
        assert_ne!(t(*lang, "sound_alert"), "Unknown");
        assert_ne!(t(*lang, "sound_test_tip"), "Unknown");
        assert_ne!(t(*lang, "tip_size_unit"), "Unknown");
        assert_ne!(t(*lang, "hex_cols_dec"), "Unknown");
        assert_ne!(t(*lang, "hex_cols_inc"), "Unknown");
        assert_ne!(t(*lang, "font_dec_tip"), "Unknown");
        assert_ne!(t(*lang, "font_inc_tip"), "Unknown");
        assert_ne!(t(*lang, "font_reset_tip"), "Unknown");
        assert_ne!(t(*lang, "tip_regex_checkbox"), "Unknown");
        assert_ne!(t(*lang, "case_sensitive"), "Unknown");
        assert_ne!(t(*lang, "case_sensitive_tip"), "Unknown");
    }
}

#[test]
fn test_dock_state_serialization_and_restore() {
    use egui_dock::DockState;
    use fasttail::ui::dock::FastTailTab;

    let path1 = std::path::PathBuf::from(r"C:\logs\service1\app.log");
    let path2 = std::path::PathBuf::from(r"C:\logs\service2\debug.log");
    let mut dock_state = DockState::new(vec![FastTailTab::LogStream(path1.clone())]);
    let [_left, _right] = dock_state.main_surface_mut().split_right(
        egui_dock::NodeIndex::root(),
        0.35,
        vec![FastTailTab::LogStream(path2.clone())],
    );
    let ron_str = ron::to_string(&dock_state).expect("serialize dock state with ron");

    let mut config = FastTailConfig::default();
    config.dock_layout = Some(ron_str.clone());

    let tmp_dir = tempfile::tempdir().unwrap();
    let ini_file = tmp_dir.path().join("test.ini");
    let ini_data = config.to_ini();
    ini_data.write_to_file(&ini_file).expect("write INI");

    let loaded_ini = ini::Ini::load_from_file(&ini_file).expect("read INI");
    let loaded_cfg = FastTailConfig::from_ini(&loaded_ini);

    let restored_ron = loaded_cfg.dock_layout.expect("dock_layout should be Some");
    assert_eq!(ron_str, restored_ron, "RON strings must match exactly");
    let restored_state: DockState<FastTailTab> =
        ron::from_str(&restored_ron).expect("RON from INI must deserialize");
    assert!(restored_state
        .find_tab(&FastTailTab::LogStream(path1))
        .is_some());
    assert!(restored_state
        .find_tab(&FastTailTab::LogStream(path2))
        .is_some());
}

#[test]
fn test_tail_engine_crlf_and_lf_handling() {
    let mut tmp = NamedTempFile::new().unwrap();
    // Windows CRLF
    write!(tmp, "Line with CRLF\r\n").unwrap();
    // Unix LF
    write!(tmp, "Line with LF\n").unwrap();
    // Last line without newline
    write!(tmp, "Last line no newline").unwrap();
    tmp.flush().unwrap();

    let engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 3);
    assert_eq!(engine.get_line(0).as_deref(), Some("Line with CRLF"));
    assert_eq!(engine.get_line(1).as_deref(), Some("Line with LF"));
    assert_eq!(engine.get_line(2).as_deref(), Some("Last line no newline"));
}

#[test]
fn test_tail_engine_empty_file() {
    let tmp = NamedTempFile::new().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 0);
    assert_eq!(engine.get_line(0).as_deref(), None);

    // Append to empty file
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(tmp.path())
        .unwrap();
    writeln!(f, "First line after empty").unwrap();
    f.flush().unwrap();

    engine.poll_updates();
    assert_eq!(engine.total_lines(), 1);
    assert_eq!(
        engine.get_line(0).as_deref(),
        Some("First line after empty")
    );
}

#[test]
fn test_tail_engine_truncation_and_rotation() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "Old Log Line 1").unwrap();
    writeln!(tmp, "Old Log Line 2").unwrap();
    writeln!(tmp, "Old Log Line 3").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 3);

    // The engine keeps a shared read handle: the writer may truncate the file under it.
    std::fs::write(tmp.path(), "Rotated Fresh Line 1\n").unwrap();

    engine.poll_updates();
    assert_eq!(engine.total_lines(), 1);
    assert_eq!(engine.get_line(0).as_deref(), Some("Rotated Fresh Line 1"));
}

#[test]
fn test_tail_engine_multiline_stacktrace_grouping() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-16 [INFO] Normal line").unwrap();
    writeln!(tmp, "2026-09-16 [ERROR] NullPointerException").unwrap();
    writeln!(tmp, "  at com.example.App.main(App.java:15)").unwrap();
    writeln!(tmp, "  at java.base/java.lang.Thread.run(Thread.java:829)").unwrap();
    writeln!(tmp, "2026-09-16 [INFO] Another line").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_include_filter("ERROR");

    // Line 0: INFO -> not visible
    assert!(!engine.is_line_visible(0));
    // Line 1: ERROR -> visible
    assert!(engine.is_line_visible(1));
    // Line 2: Stack trace child -> visible because parent (1) is ERROR
    assert!(engine.is_line_visible(2));
    // Line 3: Stack trace child -> visible because parent (1) is ERROR
    assert!(engine.is_line_visible(3));
    // Line 4: INFO -> not visible
    assert!(!engine.is_line_visible(4));
}

#[test]
fn test_tail_engine_highlights_priority() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "CRITICAL ERROR in payment gateway").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();

    let rule1 = HighlightRule::new("CRITICAL", [255, 0, 0], [0, 0, 0], false);
    let rule2 = HighlightRule::new("ERROR", [255, 255, 0], [10, 10, 10], false);

    engine.set_highlight_rules(vec![rule1, rule2]);

    let style1 = engine
        .match_highlight("CRITICAL ERROR in payment gateway")
        .unwrap();
    // Rule 1 has priority over Rule 2
    assert_eq!(style1.fg, egui::Color32::from_rgb(255, 0, 0));
    assert_eq!(style1.bg, egui::Color32::from_rgb(0, 0, 0));
    assert_eq!(style1.bold, false);
    assert_eq!(style1.italic, false);

    // Disable Rule 1 -> Rule 2 should now match
    let mut disabled_rule1 = HighlightRule::new("CRITICAL", [255, 0, 0], [0, 0, 0], false);
    disabled_rule1.enabled = false;
    let rule2 = HighlightRule::new("ERROR", [255, 255, 0], [10, 10, 10], false);
    engine.set_highlight_rules(vec![disabled_rule1, rule2]);

    let style2 = engine
        .match_highlight("CRITICAL ERROR in payment gateway")
        .unwrap();
    assert_eq!(style2.fg, egui::Color32::from_rgb(255, 255, 0));
    assert_eq!(style2.bg, egui::Color32::from_rgb(10, 10, 10));
}

#[test]
fn test_tail_engine_find_matches() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "Line A: foo bar").unwrap();
    writeln!(tmp, "Line B: hello world").unwrap();
    writeln!(tmp, "Line C: another FOO here").unwrap();
    tmp.flush().unwrap();

    let engine = TailEngine::open(tmp.path()).unwrap();
    let matches = engine.find_matches("foo");
    assert_eq!(matches, vec![0, 2]);

    let empty_matches = engine.find_matches("");
    assert!(empty_matches.is_empty());

    let no_matches = engine.find_matches("nonexistent_string_123");
    assert!(no_matches.is_empty());
}

#[test]
fn test_language_from_code_parsing() {
    assert_eq!(Language::from_code("it-IT"), Language::It);
    assert_eq!(Language::from_code("it_IT"), Language::It);
    assert_eq!(Language::from_code("fr-FR"), Language::Fr);
    assert_eq!(Language::from_code("es-ES"), Language::Es);
    assert_eq!(Language::from_code("zh-CN"), Language::Zh);
    assert_eq!(Language::from_code("en-US"), Language::En);
    assert_eq!(Language::from_code("de-DE"), Language::De);
    assert_eq!(Language::from_code("pt-BR"), Language::PtBr);
    assert_eq!(Language::from_code("pt-PT"), Language::PtBr);
    assert_eq!(Language::from_code("ru"), Language::Ru);
    assert_eq!(Language::from_code("uk-UA"), Language::Uk);
    assert_eq!(Language::from_code("ja-JP"), Language::Ja);
    assert_eq!(Language::from_code("ko_KR"), Language::Ko);
    assert_eq!(Language::from_code("tr-TR"), Language::Tr);
    assert_eq!(Language::from_code("pl"), Language::Pl);
    assert_eq!(Language::from_code("nl-BE"), Language::Nl);
    assert_eq!(Language::from_code("fur-IT"), Language::Fur);
    // Traditional Chinese wins over the generic zh prefix.
    assert_eq!(Language::from_code("zh-TW"), Language::ZhTw);
    assert_eq!(Language::from_code("zh-Hant"), Language::ZhTw);
    assert_eq!(Language::from_code("zh-HK"), Language::ZhTw);
    assert_eq!(Language::from_code("zh"), Language::Zh);
    // Unknown tags, and tags that only start with a known prefix, fall back to English.
    assert_eq!(Language::from_code("sv-SE"), Language::En);
    assert_eq!(Language::from_code("iterable"), Language::En);
}

#[test]
fn test_every_language_translates_every_key() {
    // A language block that misses a key silently falls back to English; catching that
    // here keeps a half-translated language out of a release.
    for lang in Language::ALL {
        for key in ["settings", "language", "renderer", "help", "close_tab"] {
            assert!(!t(*lang, key).is_empty(), "{lang:?} has no text for {key}");
        }
        assert!(!lang.name().is_empty());
        assert_eq!(Language::from_code(lang.code()), *lang, "code round-trip");
    }
}

#[test]
fn test_theme_visuals_generation() {
    let ctx = egui::Context::default();
    for theme in &[CyberTheme::Tron, CyberTheme::Matrix, CyberTheme::Blade] {
        theme.apply(&ctx);
        let visuals = ctx.style_of(ctx.theme()).visuals.clone();
        assert!(visuals.dark_mode);
    }
}

#[test]
fn test_tail_engine_out_of_bounds() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "Single Line").unwrap();
    tmp.flush().unwrap();

    let engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.get_line(0).as_deref(), Some("Single Line"));
    assert_eq!(engine.get_line(1).as_deref(), None);
    assert_eq!(engine.get_line(9999).as_deref(), None);
    assert_eq!(engine.get_line(usize::MAX).as_deref(), None);
}

#[test]
fn test_tail_engine_line_past_file_end_returns_none() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "First line").unwrap();
    writeln!(tmp, "Second line").unwrap();
    tmp.flush().unwrap();

    let engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 2);

    // If underlying file shrinks on disk and cache is cleared:
    std::fs::write(tmp.path(), "Tiny").unwrap();
    engine.source.clear();
    // Getting lines before poll_updates must not panic even if offset is now out of range:
    assert!(engine.get_line(1).is_none());
}

#[test]
fn test_eframe_links_feature_enabled() {
    // The About dialog opens its URLs through egui's OpenUrl output, which egui-winit
    // forwards to the system browser only when eframe is built with the `links`
    // feature. The crate cannot see that feature at compile time, so guard the
    // dependency declaration itself.
    let manifest = include_str!("../../Cargo.toml");
    let eframe_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("eframe"))
        .expect("Cargo.toml declares the eframe dependency");
    assert!(
        eframe_line.contains("\"links\""),
        "the eframe dependency must enable the `links` feature, otherwise the About dialog links cannot open the browser: {eframe_line}"
    );
}

/// The help writes modifier keys in capitals (`CTRL + SHIFT + T`), in every language.
#[test]
fn test_help_modifier_keys_are_uppercase() {
    for lang in Language::ALL {
        for key in ["pin_tip", "help_zoom_wheel"] {
            let text = t(*lang, key);
            for word in text.split(|c: char| !c.is_alphanumeric()) {
                let upper = word.to_uppercase();
                if ["CTRL", "SHIFT", "ALT", "STRG", "UMSCHALT"].contains(&upper.as_str()) {
                    assert_eq!(word, upper, "{lang:?} {key}: {text}");
                }
            }
        }
    }
}

#[test]
fn test_i18n_exhaustive_coverage() {
    let all_keys = [
        "scope_in_selection",
        "scope_from_here",
        "scope_up_to_here",
        "scope_whole_view",
        "scope_lines",
        "search_scope_time",
        "scope_end",
        "scope_in_range",
        "scope_clear",
        "scope_reset",
        "tip_search_scope",
        "tip_scope_lines",
        "tip_find_all_time_scope",
        "find_all_skipped_untimed",
        "bookmark_report_menu",
        "bookmark_report_all",
        "bookmark_report_title",
        "report_all_streams",
        "report_context",
        "report_order",
        "report_order_stream",
        "report_order_time",
        "report_tags",
        "tip_report_tags",
        "report_include_auto",
        "report_summary",
        "report_skipped",
        "report_cancel",
        "report_copy",
        "report_save",
        "report_too_large",
        "report_copied",
        "report_saved",
        "report_save_failed",
        "goto_no_tag",
        "report_stream_closed",
        "time_range_relative",
        "time_range_relative_tip",
        "time_range_live",
        "time_range_live_minute",
        "time_range_live_empty",
        "copy_selected_text",
        "help_desc_charsel",
        "scratch_title",
        "scratch_open",
        "scratch_send",
        "scratch_send_plain",
        "scratch_find_hint",
        "scratch_save_as",
        "scratch_clear",
        "scratch_clear_confirm",
        "scratch_not_found",
        "scratch_tip",
        "scratch_file_not_found",
        "scratch_sent",
        "scratch_too_large",
        "scratch_nothing",
        "scratch_large",
        "filter_tab_open",
        "filter_tab_no_filter",
        "filter_tab_tip",
        "filter_tab_source_closed",
        "filter_tab_full",
        "filter_tab_write_failed",
        "compare_title",
        "compare_empty",
        "compare_ignore",
        "compare_opt_timestamp",
        "compare_opt_numbers",
        "compare_opt_ids",
        "compare_opt_whitespace",
        "compare_opt_case",
        "compare_opt_json",
        "compare_prev",
        "compare_next",
        "compare_copy_unified",
        "compare_identical",
        "compare_changes",
        "compare_coarse",
        "compare_capped",
        "compare_jump_tip",
        "compare_menu",
        "compare_selected",
        "compare_selected_tip",
        "compare_mark",
        "compare_with_mark",
        "compare_marked",
        "tray_show",
        "tray_hide",
        "tray_mute",
        "tray_quit",
        "tray_tip",
        "tray_tip_alerts",
        "tray_icon",
        "tray_icon_tip",
        "tray_unavailable",
        "minimize_to_tray",
        "close_to_tray",
        "close_to_tray_tip",
        "help_desc_quit",
        "app_subtitle",
        "follow_tail",
        "paused",
        "tailing",
        "lines",
        "file_size",
        "throughput",
        "cpu",
        "ram",
        "open_file",
        "close_tab",
        "filter_include",
        "filter_exclude",
        "highlight_rules",
        "filters",
        "telemetry",
        "settings",
        "theme",
        "language",
        "language_system",
        "language_system_tip",
        "screensaver",
        "screensaver_timeout",
        "screensaver_zero_off",
        "lock_section",
        "lock_enable",
        "lock_pin",
        "lock_pin_hint",
        "lock_save_pin",
        "lock_clear_pin",
        "lock_now",
        "lock_now_tip",
        "lock_needs_pin",
        "lock_note",
        "locked_title",
        "locked_prompt",
        "lock_unlock",
        "lock_unlock_tip",
        "lock_cooldown",
        "lock_wrong",
        "ext_tools",
        "ext_tools_placeholders",
        "ext_tools_cookbook",
        "ext_tools_cookbook_tip",
        "ext_tool_name",
        "ext_tool_program",
        "ext_tool_args",
        "ext_tool_shortcut",
        "ext_tool_bad_shortcut",
        "ext_tool_rule",
        "ext_tool_no_rule",
        "ext_tool_rule_missing",
        "ext_tool_match",
        "ext_tool_shell",
        "ext_tool_shell_warn",
        "ext_tool_dropped",
        "ext_tool_add",
        "ext_tool_remove",
        "ext_tool_default_name",
        "ext_tool_run_failed",
        "ext_tools_menu",
        "help_key_tools",
        "help_desc_tools",
        "sound_fx",
        "baretail_title",
        "baretail_desc",
        "baretail_import",
        "baretail_skip",
        "json_expand",
        "json_collapse",
        "search_placeholder",
        "add_rule",
        "no_file_open",
        "file_empty",
        "no_matching_lines",
        "clear",
        "borderless",
        "show_lines",
        "monitor_on",
        "monitor_off",
        "tip_follow_tail",
        "tip_monitor",
        "view_mode_text",
        "view_mode_hex",
        "hex_offset",
        "hex_bytes",
        "active_count",
        "color_filters_desc",
        "visibility_filters_desc",
        "help",
        "recent_files",
        "clear_recent",
        "no_recent_files",
        "shortcuts_title",
        "font_size",
        "zoom",
        "zoom_tip",
        "rules_order_hint",
        "bold",
        "italic",
        "move_up",
        "move_down",
        "view_mode_filtered",
        "hex_columns",
        "sound_alert",
        "sound_test_tip",
        "close_tip",
        "restore_tip",
        "maximize_tip",
        "minimize_tip",
        "drag_tip",
        "open_file_tip",
        "settings_tip",
        "about_tip",
        "help_tip",
        "filter_tip",
        "play_tip",
        "pause_tip",
        "tip_search_box",
        "tip_add_rule",
        "tip_size_unit",
        "hex_cols_dec",
        "hex_cols_inc",
        "font_dec_tip",
        "font_inc_tip",
        "font_reset_tip",
        "tip_regex_checkbox",
        "about",
        "about_version",
        "about_build_date",
        "about_author",
        "about_repo",
        "about_website",
        "status_no_file",
        "about_license",
        "about_tagline",
        "help_cat_zoom",
        "help_zoom_in",
        "help_zoom_out",
        "help_zoom_reset",
        "help_zoom_wheel",
        "help_cat_nav",
        "help_key_space",
        "help_desc_space",
        "help_desc_search",
        "help_desc_find_next",
        "help_desc_f1",
        "help_desc_esc",
        "help_desc_drag_drop",
        "help_cat_filters",
        "help_filter_order",
        "help_filter_reorder",
        "help_filter_styles",
        "help_filter_visibility",
        "help_filter_recent",
        "active_rules_stat",
        "active_stream_stat",
        "hint_rule_pattern",
        "preview",
        "closed",
        "view_mode_all",
        "tip_mode_txt",
        "tip_mode_hex",
        "tip_mode_md",
        "md_search_source",
        "case_sensitive",
        "case_sensitive_tip",
        "delete_rule",
        "clear_search",
        "clear_filter",
        "about_renderer",
        "renderer",
        "renderer_auto",
        "renderer_glow",
        "renderer_wgpu",
        "renderer_software",
        "renderer_note",
        "renderer_tip",
        "software_banner",
        "export_visible",
        "export_matches",
        "export_tip",
        "help_desc_select",
        "help_desc_copy",
        "time_range",
        "time_from_hint",
        "time_to_hint",
        "time_range_clear",
        "time_range_invalid",
        "time_range_unavailable",
        "time_range_unavailable_tip",
        "time_range_pending",
        "time_span",
        "time_range_click",
        "time_range_whole",
        "time_range_first_day",
        "time_range_first_day_tip",
        "time_range_last_day",
        "time_range_last_day_tip",
        "time_range_last_hour",
        "time_range_last_hour_tip",
        "time_clock_tip",
        "cal_months",
        "cal_weekdays",
        "cal_prev_month",
        "cal_next_month",
        "cal_prev_year",
        "cal_next_year",
        "goto_time",
        "goto_label",
        "goto_hint",
        "goto_hidden",
        "goto_invalid",
        "help_desc_goto",
        "captures_only",
        "tip_captures_only",
        "quick_labels",
        "tip_remove_label",
        "label_no_text",
        "help_desc_labels",
        "always_on_top",
        "pin_tip",
        "clear_bookmarks",
        "help_desc_bookmark",
        "flash_on_alert",
        "flash_on_alert_tip",
        "scan_indexing",
        "scan_filtering",
        "scan_searching",
        "scan_timestamps",
        "md_too_large",
        "session_tip",
        "session_save_as",
        "session_save",
        "session_load",
        "session_recent",
        "session_no_recent",
        "session_clear_recent",
        "session_save_default",
        "session_unsaved_title",
        "session_unsaved_body",
        "session_load_anyway",
        "session_cancel",
        "session_missing_title",
        "session_missing_body",
        "session_ok",
        "perf_section",
        "poll_interval",
        "poll_interval_tip",
        "size_check_interval",
        "size_check_interval_tip",
        "max_fps",
        "max_fps_tip",
        "max_fps_software",
        "max_fps_software_tip",
        "mouse_throttle",
        "mouse_throttle_tip",
        "markdown_max_size",
        "markdown_max_size_tip",
        "zip_picker_title",
        "zip_picker_filter",
        "zip_picker_all",
        "zip_picker_none",
        "zip_picker_name",
        "preset_name",
        "zip_picker_size",
        "zip_picker_open",
        "zip_picker_no_selection",
        "zip_entry_encrypted",
        "zip_entry_method",
        "zip_entry_unsafe",
        "zip_entry_duplicate",
        "zip_empty",
        "archive_scanning",
        "archive_scan_stop",
        "archive_scan_partial",
        "archive_scan_stopped",
        "archive_damaged",
        "archive_no_openable",
        "tar_entry_link",
        "tar_entry_sparse",
        "compressed_no_such_entry",
        "compressed_window_too_large",
        "sevenz_entry_dictionary",
        "sevenz_header_encrypted",
        "sevenz_header_too_large",
        "sevenz_too_many_entries",
        "sevenz_list_partial",
        "sevenz_block_size",
        "compressed_open_failed",
        "compressed_no_space",
        "compressed_follow_tip",
        "compressed_decompressing",
        "compressed_cancel",
        "compressed_cancelled",
        "compressed_cap_reached",
        "compressed_disk_full",
        "compressed_tar",
        "compressed_failed",
        "compressed_partial",
        "compressed_reload",
        "compressed_max_size",
        "compressed_max_size_tip",
        "spool_dir",
        "spool_dir_tip",
        "spool_dir_reset",
        "stdin_footer",
        "stdin_ended",
        "stdin_failed",
        "stdin_restarted_cap",
        "stdin_restarted_disk",
        "stdin_spool_max",
        "stdin_spool_max_tip",
        "stdin_not_saved_title",
        "stdin_not_saved",
        "ansi_auto",
        "ansi_render",
        "ansi_strip",
        "ansi_raw",
        "tip_ansi",
        "ansi_switched",
        "tip_search_pane",
        "search_pane_header",
        "search_capped",
        "search_pane_no_matches",
        "search_pane_hex",
        "overview_strip",
        "overview_strip_tip",
        "overview_line",
        "overview_sampled",
        "help_desc_search_pane",
        "tip_time_delta",
        "time_delta_unusable",
        "show_time_delta",
        "default_line_numbers",
        "default_time_delta",
        "default_columns_tip",
        "time_delta_gap",
        "time_anchor_set",
        "time_anchor_clear",
        "selection_elapsed",
        "selection_elapsed_tip",
        "find_results_title",
        "find_all_hint",
        "find_all_run",
        "find_all_stop",
        "find_all_refresh",
        "find_all_summary",
        "find_all_progress",
        "find_all_snapshot",
        "find_all_group_count",
        "find_all_queued",
        "find_all_stopped",
        "find_all_failed",
        "find_all_skipped_hex",
        "find_all_stale",
        "find_all_empty",
        "tip_find_all",
        "tip_find_all_refresh",
        "help_desc_find_all",
        "filter_terms_all_of",
        "filter_terms_none_of",
        "filter_terms_desc",
        "filter_add_term",
        "filter_add_term_tip",
        "global_filter_title",
        "global_filter_enabled",
        "global_filter_tip",
        "global_filter_badge_tip",
        "global_filter_close",
        "help_desc_global_filter",
        "toolbar_play",
        "toolbar_pause",
        "toolbar_help",
        "toolbar_about",
        "help_desc_tabs",
        "help_desc_lock",
        "export_failed",
        "session_save_failed",
        "context_show",
        "context_banner",
        "context_back",
        "context_back_tip",
        "help_desc_context",
        "bookmark_note_menu",
        "bookmark_note_label",
        "bookmark_note_hint",
        "bookmark_remove",
        "rule_auto_bookmark",
        "rule_auto_bookmark_tip",
        "auto_bookmarks_capped",
        "auto_bookmarks_capped_tip",
        "auto_bookmark_max",
        "auto_bookmark_max_tip",
        "scan_auto_bookmarks",
        "collapse_label",
        "collapse_off",
        "collapse_exact",
        "collapse_numbers",
        "tip_collapse",
        "collapse_badge_tip",
        "collapse_badge_expand",
        "collapse_badge_fold",
        "scan_collapsing",
        "collapse_rows",
        "copy_rows",
        "copy_as_shown",
        "help_desc_collapse",
        "context_lines_tip",
        "context_lines_hidden",
        "help_desc_context_lines",
        "filter_extra_terms_tip",
        "filter_remove_term",
        "filter_term_invalid",
        "filter_presets",
        "presets",
        "presets_tip",
        "presets_none",
        "preset_apply_all",
        "preset_apply_all_tip",
        "preset_save_current",
        "preset_update",
        "preset_manage",
        "preset_save_title",
        "preset_name",
        "preset_include_time",
        "preset_exists",
        "preset_overwrite",
        "preset_save",
        "preset_cancel",
        "preset_rename",
        "preset_delete",
        "preset_delete_confirm",
        "preset_name_taken",
        "timeline_tip",
        "timeline_search_lane_tip",
        "timeline_peak",
        "timeline_empty",
        "timeline_no_level",
        "timeline_untimed",
        "timeline_lane_note",
        "already_at_top",
        "already_at_bottom",
        // Command palette: categories, texts, disabled reasons and action names.
        "palette_cat_window",
        "palette_cat_stream",
        "palette_cat_view",
        "palette_cat_search",
        "palette_cat_bookmarks",
        "palette_cat_session",
        "palette_cat_settings",
        "palette_title",
        "palette_hint",
        "palette_no_match",
        "palette_toggle",
        "help_desc_palette",
        "palette_no_stream",
        "palette_not_hex",
        "palette_text_only",
        "palette_no_search",
        "palette_no_row",
        "palette_no_bookmarks",
        "palette_row_not_bookmarked",
        "palette_no_selection",
        "palette_no_filter",
        "palette_not_in_context",
        "palette_no_time_delta",
        "palette_no_anchor",
        "palette_no_session",
        "act_global_filter",
        "act_find_all",
        "act_play_all",
        "act_pause_all",
        "act_open_pattern",
        "act_follow",
        "act_monitor",
        "act_goto",
        "act_select_all",
        "act_view_text",
        "act_view_hex",
        "act_view_md",
        "act_time_delta",
        "act_wrap",
        "act_collapse",
        "act_timeline",
        "act_search_pane",
        "act_search",
        "act_search_next",
        "act_search_prev",
        "act_bookmark_toggle",
        "act_bookmark_next",
        "act_bookmark_prev",
        "act_size_unit",
        "auto_highlight",
        "auto_highlight_tip",
        "auto_hl_ip",
        "auto_hl_uuid",
        "auto_hl_url",
        "auto_hl_duration",
        "auto_hl_path",
        "rules_export",
        "rules_export_tip",
        "rules_import",
        "rules_import_tip",
        "rules_import_preview",
        "rules_import_append",
        "rules_import_append_tip",
        "rules_import_replace",
        "rules_import_replace_tip",
        "rules_replace_confirm",
        "rules_import_cancel",
        "rules_exported",
        "rules_appended",
        "rules_replaced",
        "rules_not_a_set",
        "rules_newer_version",
        "rules_read_failed",
        "selection_hl_menu",
        "selection_hl_clear",
        "rule_next_menu",
        "act_rule_next",
        "act_rule_prev",
        "rule_nav_no_rule",
        "rule_nav_none",
        "rule_nav_seeking",
        "rule_nav_seeking_tip",
        "help_desc_rule_nav",
        "help_key_double_click",
        "help_desc_token_hl",
        "time_display_written",
        "time_display_local",
        "time_display_title",
        "time_display_offset",
        "time_display_offset_tip",
        "time_source_title",
        "time_source_tip",
        "time_display_note",
        "time_display_tip",
        "time_range_zone",
        "act_time_written",
        "act_time_utc",
        "act_time_local",
    ];

    for lang in Language::ALL {
        for key in &all_keys {
            let translation = t(*lang, key);
            assert!(
                !translation.is_empty() && translation != "Unknown",
                "Missing key '{}' for language {:?}",
                key,
                lang
            );
        }
    }

    // An untranslated key silently falls back to English, which the check above cannot
    // see. A real language block differs from English almost everywhere: a handful of
    // keys legitimately match (CPU, RAM, wgpu, OpenGL, OK...), a fallen-back block does
    // not. 20% leaves room for those while still catching a block that never landed.
    for lang in Language::ALL {
        if *lang == Language::En {
            continue;
        }
        let same = all_keys
            .iter()
            .filter(|key| t(*lang, key) == t(Language::En, key))
            .count();
        assert!(
            same * 5 < all_keys.len(),
            "{lang:?} repeats the English text for {same} of {} keys: is its block missing?",
            all_keys.len()
        );
    }

    // Key by key, since one missing key is far below that threshold: a string may equal
    // the English one only where that is the right translation (a product name, an
    // abbreviation, a word several languages share), and those keys are listed here.
    const SAME_AS_ENGLISH: &[&str] = &[
        "cpu",
        "ram",
        "lock_pin",
        "zoom",
        "help",
        "filters",
        "quick_labels",
        "session_ok",
        "ansi_auto",
        "renderer",
        "renderer_wgpu",
        "renderer_glow",
        "renderer_software",
        "hex_bytes",
        "hex_offset",
        "view_mode_text",
        "zip_picker_name",
        "preset_name",
        "ext_tool_name",
        "ext_tool_program",
        "ext_tool_args",
        "about_version",
        "about_renderer",
        "about_repo",
        "about_website",
        "toolbar_pause",
        "toolbar_help",
        // "Stream" (German, Dutch) and "Session" (French, Friulian) are the local words.
        "palette_cat_stream",
        "palette_cat_session",
    ];
    let fallbacks: Vec<String> = Language::ALL
        .iter()
        .filter(|lang| **lang != Language::En)
        .flat_map(|lang| {
            all_keys
                .iter()
                .filter(|key| !SAME_AS_ENGLISH.contains(key))
                .filter(move |key| t(*lang, key) == t(Language::En, key))
                .map(move |key| format!("{lang:?} {key}"))
        })
        .collect();
    assert!(
        fallbacks.is_empty(),
        "keys falling back to English: {fallbacks:?}"
    );
}

#[test]
fn test_move_up_move_down_translations() {
    for lang in [
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        let up = t(lang, "move_up");
        let down = t(lang, "move_down");
        let at_top = t(lang, "already_at_top");
        let at_bottom = t(lang, "already_at_bottom");
        assert!(
            !up.is_empty() && up != "Unknown",
            "move_up missing for {lang:?}"
        );
        assert!(
            !down.is_empty() && down != "Unknown",
            "move_down missing for {lang:?}"
        );
        assert!(
            !at_top.is_empty() && at_top != "Unknown",
            "already_at_top missing for {lang:?}"
        );
        assert!(
            !at_bottom.is_empty() && at_bottom != "Unknown",
            "already_at_bottom missing for {lang:?}"
        );
    }
    assert_eq!(t(Language::En, "move_up"), "Move Up");
    assert_eq!(t(Language::En, "move_down"), "Move Down");
    assert_eq!(t(Language::En, "already_at_top"), "Already at top");
    assert_eq!(t(Language::En, "already_at_bottom"), "Already at bottom");
    assert_eq!(t(Language::It, "move_up"), "Sposta su");
    assert_eq!(t(Language::It, "move_down"), "Sposta giù");
    assert_eq!(t(Language::It, "already_at_top"), "Già in cima");
    assert_eq!(t(Language::It, "already_at_bottom"), "Già in fondo");
    assert_eq!(t(Language::Fr, "move_up"), "Déplacer vers le haut");
    assert_eq!(t(Language::Fr, "move_down"), "Déplacer vers le bas");
    assert_eq!(t(Language::Es, "move_up"), "Mover arriba");
    assert_eq!(t(Language::Es, "move_down"), "Mover abajo");
    assert_eq!(t(Language::Zh, "move_up"), "上移");
    assert_eq!(t(Language::Zh, "move_down"), "下移");
}

#[test]
fn test_filter_naming_and_translation_consistency() {
    // Italian must strictly use "Filtri" and not "Highlights"
    assert_eq!(t(Language::It, "filters"), "Filtri");
    assert_eq!(t(Language::It, "highlight_rules"), "Filtri di Colore");
    assert_eq!(t(Language::It, "add_rule"), "+ Aggiungi Filtro");

    // English
    assert_eq!(t(Language::En, "filters"), "Filters");
    assert_eq!(t(Language::En, "highlight_rules"), "Color Filters");
    assert_eq!(t(Language::En, "add_rule"), "+ Add Filter");

    // French
    assert_eq!(t(Language::Fr, "filters"), "Filtres");
    assert_eq!(t(Language::Fr, "highlight_rules"), "Filtres de Couleur");

    // Spanish
    assert_eq!(t(Language::Es, "filters"), "Filtros");
    assert_eq!(t(Language::Es, "highlight_rules"), "Filtros de Color");
}

#[test]
fn test_config_starts_with_empty_filters() {
    let config = FastTailConfig::default();
    assert!(
        config.highlight_rules.is_empty(),
        "Filters must start empty by default"
    );

    // Creating a new filter rule should start with an empty pattern and regex disabled
    let new_rule = HighlightRule::new("", [255, 255, 255], [0, 100, 200], false);
    assert_eq!(new_rule.pattern, "");
    assert!(!new_rule.is_regex);
}

#[test]
fn test_tail_engine_binary_hex_streaming() {
    let mut tmp = NamedTempFile::new().unwrap();
    // Write 20 binary bytes
    let sample_bytes: [u8; 20] = [
        0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x20, 0x57, 0x6F, // Hello Wo
        0x72, 0x6C, 0x64, 0x21, 0x00, 0xFF, 0xFE, 0x0A, // rld!....
        0xDE, 0xAD, 0xBE, 0xEF, // ....
    ];
    tmp.write_all(&sample_bytes).unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.file_size, 20);
    assert_eq!(engine.total_hex_rows(16), 2);

    let first_row = engine.get_bytes(0, 16).unwrap();
    assert_eq!(first_row.len(), 16);
    assert_eq!(&first_row[0..5], b"Hello");

    let second_row = engine.get_bytes(16, 16).unwrap();
    assert_eq!(second_row.len(), 4);
    assert_eq!(second_row, &[0xDE, 0xAD, 0xBE, 0xEF]);

    // Append 10 more bytes to test live binary streaming
    tmp.write_all(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A])
        .unwrap();
    tmp.flush().unwrap();

    engine.poll_updates();
    assert_eq!(engine.file_size, 30);
    assert_eq!(engine.total_hex_rows(16), 2);

    // Append another 5 bytes -> exceeds 32 bytes -> 3 rows!
    tmp.write_all(&[0x11, 0x22, 0x33, 0x44, 0x55]).unwrap();
    tmp.flush().unwrap();

    engine.poll_updates();
    assert_eq!(engine.file_size, 35);
    assert_eq!(engine.total_hex_rows(16), 3);
}

#[test]
fn test_tail_engine_binary_auto_detection() {
    use fasttail::tail_engine::ViewMode;

    // Normal text file should start in ViewMode::Text
    let mut text_file = NamedTempFile::new().unwrap();
    writeln!(text_file, "This is plain text log line").unwrap();
    text_file.flush().unwrap();
    let text_engine = TailEngine::open(text_file.path()).unwrap();
    assert_eq!(text_engine.view_mode, ViewMode::Text);

    // Binary file containing null byte should auto-detect ViewMode::Hex
    let mut bin_file = NamedTempFile::new().unwrap();
    bin_file
        .write_all(&[0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00])
        .unwrap();
    bin_file.flush().unwrap();
    let bin_engine = TailEngine::open(bin_file.path()).unwrap();
    assert_eq!(bin_engine.view_mode, ViewMode::Hex);
}

#[test]
fn test_tail_engine_encodings() {
    use fasttail::tail_engine::FileEncoding;

    // 1. ASCII
    let mut f_ascii = NamedTempFile::new().unwrap();
    f_ascii.write_all(b"Hello ASCII\nSecond Line\n").unwrap();
    f_ascii.flush().unwrap();
    let mut engine_ascii = TailEngine::open(f_ascii.path()).unwrap();
    engine_ascii.set_encoding(FileEncoding::Ascii);
    assert_eq!(engine_ascii.total_lines(), 2);
    assert_eq!(engine_ascii.get_line(0).as_deref(), Some("Hello ASCII"));
    assert_eq!(engine_ascii.get_line(1).as_deref(), Some("Second Line"));

    // 2. ANSI (Latin-1 / Windows-1252)
    let mut f_ansi = NamedTempFile::new().unwrap();
    // 'C', 'a', 'f', 0xE9 (é in Latin-1), '\n'
    f_ansi
        .write_all(&[b'C', b'a', b'f', 0xE9, b'\n', b'O', b'k', b'\n'])
        .unwrap();
    f_ansi.flush().unwrap();
    let mut engine_ansi = TailEngine::open(f_ansi.path()).unwrap();
    engine_ansi.set_encoding(FileEncoding::Ansi);
    assert_eq!(engine_ansi.total_lines(), 2);
    assert_eq!(engine_ansi.get_line(0).as_deref(), Some("Café"));
    assert_eq!(engine_ansi.get_line(1).as_deref(), Some("Ok"));

    // 3. UTF-8 (with BOM)
    let mut f_utf8 = NamedTempFile::new().unwrap();
    f_utf8.write_all(&[0xEF, 0xBB, 0xBF]).unwrap(); // UTF-8 BOM
    f_utf8
        .write_all("Prima riga UTF-8\nSeconda riga 🚀\n".as_bytes())
        .unwrap();
    f_utf8.flush().unwrap();
    let engine_utf8 = TailEngine::open(f_utf8.path()).unwrap();
    assert_eq!(engine_utf8.encoding, FileEncoding::Utf8);
    assert_eq!(engine_utf8.total_lines(), 2);
    assert_eq!(engine_utf8.get_line(0).as_deref(), Some("Prima riga UTF-8"));
    assert_eq!(engine_utf8.get_line(1).as_deref(), Some("Seconda riga 🚀"));

    // 4. Unicode (UTF-16 LE with BOM)
    let mut f_utf16le = NamedTempFile::new().unwrap();
    let mut u16le_bytes = vec![0xFF, 0xFE]; // BOM
    for c in "Unicode LE Line 1\r\nLine 2".encode_utf16() {
        u16le_bytes.extend_from_slice(&c.to_le_bytes());
    }
    f_utf16le.write_all(&u16le_bytes).unwrap();
    f_utf16le.flush().unwrap();
    let engine_utf16le = TailEngine::open(f_utf16le.path()).unwrap();
    assert_eq!(engine_utf16le.encoding, FileEncoding::UnicodeLe);
    assert_eq!(engine_utf16le.total_lines(), 2);
    assert_eq!(
        engine_utf16le.get_line(0).as_deref(),
        Some("Unicode LE Line 1")
    );
    assert_eq!(engine_utf16le.get_line(1).as_deref(), Some("Line 2"));

    // 5. Unicode Big Endian (UTF-16 BE with BOM)
    let mut f_utf16be = NamedTempFile::new().unwrap();
    let mut u16be_bytes = vec![0xFE, 0xFF]; // BOM
    for c in "Unicode BE Line 1\nLine 2".encode_utf16() {
        u16be_bytes.extend_from_slice(&c.to_be_bytes());
    }
    f_utf16be.write_all(&u16be_bytes).unwrap();
    f_utf16be.flush().unwrap();
    let engine_utf16be = TailEngine::open(f_utf16be.path()).unwrap();
    assert_eq!(engine_utf16be.encoding, FileEncoding::UnicodeBe);
    assert_eq!(engine_utf16be.total_lines(), 2);
    assert_eq!(
        engine_utf16be.get_line(0).as_deref(),
        Some("Unicode BE Line 1")
    );
    assert_eq!(engine_utf16be.get_line(1).as_deref(), Some("Line 2"));
}

#[test]
fn test_highlight_rule_bold_italic_and_reordering() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-16 [CRITICAL] Database connection failed").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();

    // Rule A: match "CRITICAL" -> red fg, black bg, bold=true, italic=false
    let rule_a = HighlightRule::with_style("CRITICAL", [255, 0, 0], [0, 0, 0], false, true, false);
    // Rule B: match "Database" -> blue fg, yellow bg, bold=false, italic=true
    let rule_b =
        HighlightRule::with_style("Database", [0, 0, 255], [255, 255, 0], false, false, true);

    // Initial order: [Rule A, Rule B].
    // Both match "2026-09-16 [CRITICAL] Database connection failed".
    // Evaluation order must be top-down and STOP at the first match!
    engine.set_highlight_rules(vec![rule_a.clone(), rule_b.clone()]);
    let style_first = engine
        .match_highlight("2026-09-16 [CRITICAL] Database connection failed")
        .unwrap();
    assert_eq!(style_first.fg, egui::Color32::from_rgb(255, 0, 0));
    assert!(style_first.bold, "Rule A must apply bold");
    assert!(!style_first.italic, "Rule A is not italic");

    // Reorder: swap rules to [Rule B, Rule A]
    let mut reordered_rules = vec![rule_a, rule_b];
    reordered_rules.swap(0, 1);
    engine.set_highlight_rules(reordered_rules);

    // Now Rule B comes first, so Rule B must win!
    let style_reordered = engine
        .match_highlight("2026-09-16 [CRITICAL] Database connection failed")
        .unwrap();
    assert_eq!(style_reordered.fg, egui::Color32::from_rgb(0, 0, 255));
    assert!(!style_reordered.bold, "Rule B is not bold");
    assert!(style_reordered.italic, "Rule B must apply italic");
}

#[test]
fn test_font_size_and_recent_files_config() {
    use std::path::PathBuf;

    let mut config = FastTailConfig::default();
    assert_eq!(config.font_size, 13.0);

    // Zoom font
    config.font_size = 18.0;
    for i in 0..20 {
        let p = PathBuf::from(format!("C:\\logs\\app_{}.log", i));
        // MRU insertion logic:
        config.recent_files.retain(|x| x != &p);
        config.recent_files.insert(0, p);
        if config.recent_files.len() > 15 {
            config.recent_files.truncate(15);
        }
    }

    assert_eq!(config.recent_files.len(), 15);
    // Most recent is app_19.log
    assert_eq!(
        config.recent_files[0],
        PathBuf::from("C:\\logs\\app_19.log")
    );

    // Serialize and deserialize
    let toml_str = toml::to_string(&config).expect("serialize config");
    let deserialized: FastTailConfig = toml::from_str(&toml_str).expect("deserialize config");
    assert_eq!(deserialized.font_size, 18.0);
    assert_eq!(deserialized.recent_files.len(), 15);
    assert_eq!(
        deserialized.recent_files[0],
        PathBuf::from("C:\\logs\\app_19.log")
    );
}

#[test]
fn test_size_unit_cycling_and_format() {
    use fasttail::tail_engine::SizeUnit;

    let mut tmp = NamedTempFile::new().unwrap();
    // Write 1024 * 1024 bytes (1 MB)
    let payload = vec![b'A'; 1024 * 1024];
    tmp.write_all(&payload).unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.size_unit, SizeUnit::Bytes);
    assert_eq!(engine.format_size(), "1048576 B");

    // Click 1: Bytes -> MB
    engine.next_size_unit();
    assert_eq!(engine.size_unit, SizeUnit::MB);
    assert_eq!(engine.format_size(), "1.00 MB");

    // Click 2: MB -> GB
    engine.next_size_unit();
    assert_eq!(engine.size_unit, SizeUnit::GB);
    assert_eq!(engine.format_size(), "0.001 GB");

    // Click 3: GB -> Hex
    engine.next_size_unit();
    assert_eq!(engine.size_unit, SizeUnit::Hex);
    assert_eq!(engine.format_size(), "0x100000");

    // Click 4: Hex -> Bytes
    engine.next_size_unit();
    assert_eq!(engine.size_unit, SizeUnit::Bytes);
    assert_eq!(engine.format_size(), "1048576 B");
}

#[test]
fn test_view_mode_filtered() {
    use fasttail::tail_engine::{HighlightRule, ViewMode};

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-16 [INFO] System initialized").unwrap();
    writeln!(tmp, "2026-09-16 [WARN] Memory high").unwrap();
    writeln!(tmp, "2026-09-16 [ERROR] NullPointerException").unwrap();
    writeln!(tmp, "    at com.example.App.main(App.java:42)").unwrap();
    writeln!(tmp, "2026-09-16 [INFO] Heartbeat probe").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.view_mode, ViewMode::Text);

    // Switch to filtered view
    engine.view_mode = ViewMode::Filtered;

    // With no filters or highlight rules, empty filters mean no constraint: all lines visible
    assert!(engine.is_line_visible_filtered(0));
    assert!(engine.is_line_visible_filtered(1));
    assert!(engine.is_line_visible_filtered(2));
    assert!(engine.is_line_visible_filtered(3));
    assert!(engine.is_line_visible_filtered(4));

    // With empty include and non-empty exclude filter, excluded lines are hidden
    engine.set_exclude_filter("Heartbeat");
    assert!(engine.is_line_visible_filtered(0));
    assert!(engine.is_line_visible_filtered(1));
    assert!(engine.is_line_visible_filtered(2));
    assert!(engine.is_line_visible_filtered(3));
    assert!(!engine.is_line_visible_filtered(4)); // Excluded

    // Set include filter to "ERROR" (clearing exclude filter)
    engine.set_exclude_filter("");
    engine.set_include_filter("ERROR");
    assert!(!engine.is_line_visible_filtered(0)); // INFO
    assert!(!engine.is_line_visible_filtered(1)); // WARN
    assert!(engine.is_line_visible_filtered(2)); // ERROR matches!
    assert!(engine.is_line_visible_filtered(3)); // Multiline stacktrace continuation matches parent!
    assert!(!engine.is_line_visible_filtered(4)); // INFO

    // Highlight rules only style lines: they never bypass the include filter
    let rule = HighlightRule::new("Memory", [255, 200, 0], [0, 0, 0], false);
    engine.set_highlight_rules(vec![rule]);
    assert!(!engine.is_line_visible_filtered(0));
    assert!(!engine.is_line_visible_filtered(1)); // Matches the highlight rule but not "ERROR"
    assert!(engine.is_line_visible_filtered(2)); // Matches include filter!
}

#[test]
fn test_hex_columns_stepping() {
    let mut tmp = NamedTempFile::new().unwrap();
    // 64 bytes
    let payload = [0xAAu8; 64];
    tmp.write_all(&payload).unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    // Default columns = 16
    assert_eq!(engine.hex_columns, 16);
    assert_eq!(engine.total_hex_rows(engine.hex_columns), 4);

    // Step down to 8
    engine.hex_columns = (engine.hex_columns - 8).max(8);
    assert_eq!(engine.hex_columns, 8);
    assert_eq!(engine.total_hex_rows(engine.hex_columns), 8);

    // Step down cannot go below 8
    engine.hex_columns = if engine.hex_columns > 8 {
        engine.hex_columns - 8
    } else {
        engine.hex_columns
    };
    assert_eq!(engine.hex_columns, 8);

    // Step up to 24
    engine.hex_columns = (engine.hex_columns + 8).min(64);
    assert_eq!(engine.hex_columns, 16);
    engine.hex_columns = (engine.hex_columns + 8).min(64);
    assert_eq!(engine.hex_columns, 24);
    // 64 bytes with 24 bytes per row -> ceil(64 / 24) = 3 rows
    assert_eq!(engine.total_hex_rows(engine.hex_columns), 3);
}

#[test]
fn test_config_size_unit_persistence() {
    use fasttail::tail_engine::SizeUnit;

    let mut config = FastTailConfig::default();
    assert_eq!(config.size_unit, SizeUnit::Bytes);

    config.size_unit = SizeUnit::MB;
    let serialized = toml::to_string(&config).expect("serialize config with size_unit");
    let deserialized: FastTailConfig =
        toml::from_str(&serialized).expect("deserialize config with size_unit");
    assert_eq!(deserialized.size_unit, SizeUnit::MB);

    config.size_unit = SizeUnit::GB;
    let serialized = toml::to_string(&config).expect("serialize config with size_unit");
    let deserialized: FastTailConfig =
        toml::from_str(&serialized).expect("deserialize config with size_unit");
    assert_eq!(deserialized.size_unit, SizeUnit::GB);

    config.size_unit = SizeUnit::Hex;
    let serialized = toml::to_string(&config).expect("serialize config with size_unit");
    let deserialized: FastTailConfig =
        toml::from_str(&serialized).expect("deserialize config with size_unit");
    assert_eq!(deserialized.size_unit, SizeUnit::Hex);
}

#[test]
fn test_txt_vs_hex_and_filtered_submode() {
    use fasttail::tail_engine::ViewMode;

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "Line 1: Normal info").unwrap();
    writeln!(tmp, "Line 2: Target line").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    // Initially in ViewMode::Text
    assert_eq!(engine.view_mode, ViewMode::Text);

    // Toggle Filtered mode (characteristic of TXT)
    engine.view_mode = ViewMode::Filtered;
    assert_eq!(engine.view_mode, ViewMode::Filtered);

    // Toggle back to All lines in TXT
    engine.view_mode = ViewMode::Text;
    assert_eq!(engine.view_mode, ViewMode::Text);

    // Switch to HEX
    engine.view_mode = ViewMode::Hex;
    assert_eq!(engine.view_mode, ViewMode::Hex);
}

#[test]
fn test_config_ini_persistence() {
    use fasttail::tail_engine::SizeUnit;
    use std::path::PathBuf;

    let mut config = FastTailConfig::default();
    config.theme = CyberTheme::Blade;
    // A language chosen by hand: with language_auto on, the stored one is deliberately
    // ignored in favour of the system language (see the language row in Settings).
    config.language = Language::Fr;
    config.language_auto = false;
    config.screensaver_enabled = false;
    config.screensaver_timeout_mins = 15;
    config.telemetry_enabled = false;
    config.sound_enabled = true;
    config.borderless = true;
    config.show_line_numbers = false;
    config.font_size = 16.5;
    config.size_unit = SizeUnit::Hex;
    config.baretail_import = false;
    config.poll_interval_ms = 350;
    config.size_check_interval_ms = 750;
    config.max_fps = 90;
    config.max_fps_software = 20;
    config.open_files = vec![PathBuf::from("open1.log"), PathBuf::from("open2.log")];
    config.recent_files = vec![PathBuf::from("recent1.log"), PathBuf::from("recent2.log")];
    config.dock_layout = Some("LayoutTestRon".to_string());
    config.highlight_rules = vec![HighlightRule {
        pattern: "FATAL".to_string(),
        is_regex: false,
        case_sensitive: true,
        fg_color: [255, 0, 0],
        bg_color: [50, 0, 0],
        bold: true,
        italic: false,
        sound_alert: fasttail::audio::SoundAlertPreset::None,
        enabled: true,
        captures_only: false,
        auto_bookmark: false,
    }];

    let ini_obj = config.to_ini();
    let loaded = FastTailConfig::from_ini(&ini_obj);

    assert_eq!(loaded.theme, CyberTheme::Blade);
    assert_eq!(loaded.language, Language::Fr);
    assert!(!loaded.language_auto);
    assert_eq!(loaded.screensaver_enabled, false);
    assert_eq!(loaded.screensaver_timeout_mins, 15);
    assert_eq!(loaded.telemetry_enabled, false);
    assert_eq!(loaded.sound_enabled, true);
    assert_eq!(loaded.borderless, true);
    assert_eq!(loaded.show_line_numbers, false);
    assert_eq!(loaded.font_size, 16.5);
    assert_eq!(loaded.size_unit, SizeUnit::Hex);
    assert_eq!(loaded.baretail_import, false);
    assert_eq!(loaded.poll_interval_ms, 350);
    assert_eq!(loaded.size_check_interval_ms, 750);
    assert_eq!(loaded.max_fps, 90);
    assert_eq!(loaded.max_fps_software, 20);
    assert_eq!(loaded.dock_layout.as_deref(), Some("LayoutTestRon"));
    assert_eq!(loaded.recent_files.len(), 2);
    assert_eq!(loaded.recent_files[0], PathBuf::from("recent1.log"));
    assert_eq!(loaded.highlight_rules.len(), 1);
    assert_eq!(loaded.highlight_rules[0].pattern, "FATAL");
    assert_eq!(loaded.highlight_rules[0].case_sensitive, true);
    assert_eq!(
        loaded.highlight_rules[0].sound_alert,
        fasttail::audio::SoundAlertPreset::None
    );
}

#[test]
fn test_config_save_only_when_different() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("test_save.ini");

    let mut config = FastTailConfig::default();
    config.font_size = 14.0;

    assert!(
        config.save_to(&cfg_path).unwrap(),
        "first save must write the file"
    );
    let first = std::fs::read(&cfg_path).unwrap();

    // Saving an identical config must leave the file untouched
    assert!(
        !config.save_to(&cfg_path).unwrap(),
        "identical config must not be rewritten"
    );
    assert_eq!(std::fs::read(&cfg_path).unwrap(), first);

    // Saving a modified config must rewrite it
    config.font_size = 18.0;
    assert!(
        config.save_to(&cfg_path).unwrap(),
        "changed config must be rewritten"
    );
    assert_ne!(std::fs::read(&cfg_path).unwrap(), first);
}

#[test]
fn test_config_two_instances_write_only_their_own_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fasttail.ini");

    // Instance A starts first and writes its state.
    let mut a = FastTailConfig::default();
    a.font_size = 14.0;
    let mut a_synced = None;
    assert!(FastTailConfig::write_own_change(&path, &ini_bytes(&a), &mut a_synced).unwrap());

    // Instance B loads that file, then changes a setting and saves.
    let mut b_synced = Some(std::fs::read(&path).unwrap());
    let mut b = a.clone();
    b.font_size = 20.0;
    assert!(FastTailConfig::write_own_change(&path, &ini_bytes(&b), &mut b_synced).unwrap());
    let written_by_b = std::fs::read(&path).unwrap();

    // A's periodic save with nothing changed must not bring its old state back.
    for _ in 0..3 {
        assert!(
            !FastTailConfig::write_own_change(&path, &ini_bytes(&a), &mut a_synced).unwrap(),
            "an unchanged instance must not overwrite another instance's save"
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), written_by_b);

    // B saving again without changes leaves the file alone too.
    assert!(!FastTailConfig::write_own_change(&path, &ini_bytes(&b), &mut b_synced).unwrap());

    // A change of A's own is written (the last instance to change something wins).
    a.zoom_factor = 1.5;
    assert!(FastTailConfig::write_own_change(&path, &ini_bytes(&a), &mut a_synced).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), ini_bytes(&a));
}

#[test]
fn test_config_save_to_keeps_an_external_edit_until_an_own_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fasttail.ini");

    let mut config = FastTailConfig::default();
    config.font_size = 14.0;
    assert!(config.save_to(&path).unwrap());

    // Another instance (or the user) rewrites the file.
    let mut other = config.clone();
    other.font_size = 22.0;
    let external = ini_bytes(&other);
    std::fs::write(&path, &external).unwrap();

    assert!(!config.save_to(&path).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), external);

    config.font_size = 16.0;
    assert!(config.save_to(&path).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), ini_bytes(&config));
}

#[test]
fn test_perf_config_bounds_and_defaults() {
    let cfg = FastTailConfig::default();
    assert_eq!(cfg.poll_interval_ms, 250);
    assert_eq!(cfg.size_check_interval_ms, 500);
    assert_eq!(cfg.max_fps, 60);
    assert_eq!(cfg.max_fps_software, 30);
    assert_eq!(cfg.mouse_throttle_ms, 100);
    assert_eq!(cfg.markdown_max_mb, 1);

    // Test clamped parsing from ini
    let text = "[general]\npoll_interval_ms=10\nsize_check_interval_ms=99999\nmax_fps=1\nmax_fps_software=500\nmouse_throttle_ms=5000\nmarkdown_max_mb=999\n";
    let ini = ini::Ini::load_from_str(text).unwrap();
    let loaded = FastTailConfig::from_ini(&ini);
    assert_eq!(loaded.poll_interval_ms, 50); // clamped to 50
    assert_eq!(loaded.size_check_interval_ms, 10000); // clamped to 10000
    assert_eq!(loaded.max_fps, 5); // clamped to 5
    assert_eq!(loaded.max_fps_software, 120); // clamped to 120
    assert_eq!(loaded.mouse_throttle_ms, 1000); // clamped to 1000
    assert_eq!(loaded.markdown_max_mb, 100); // clamped to 100
}

#[test]
fn test_highlight_rule_sound_alert_ini_persistence() {
    use fasttail::audio::SoundAlertPreset;

    let mut config = FastTailConfig::default();
    config.highlight_rules = vec![
        HighlightRule {
            pattern: "ERR".to_string(),
            is_regex: false,
            case_sensitive: true,
            fg_color: [255, 0, 0],
            bg_color: [50, 0, 0],
            bold: true,
            italic: false,
            sound_alert: SoundAlertPreset::Critical,
            enabled: true,
            captures_only: false,
            auto_bookmark: false,
        },
        HighlightRule {
            pattern: "WARN".to_string(),
            is_regex: false,
            case_sensitive: false,
            fg_color: [255, 200, 0],
            bg_color: [50, 40, 0],
            bold: false,
            italic: true,
            sound_alert: SoundAlertPreset::Warning,
            enabled: true,
            captures_only: false,
            auto_bookmark: false,
        },
        HighlightRule {
            pattern: "INFO".to_string(),
            is_regex: false,
            case_sensitive: false,
            fg_color: [0, 255, 0],
            bg_color: [0, 50, 0],
            bold: false,
            italic: false,
            sound_alert: SoundAlertPreset::Beep,
            enabled: true,
            captures_only: false,
            auto_bookmark: false,
        },
        HighlightRule {
            pattern: "DEBUG".to_string(),
            is_regex: false,
            case_sensitive: false,
            fg_color: [0, 200, 255],
            bg_color: [0, 40, 50],
            bold: false,
            italic: false,
            sound_alert: SoundAlertPreset::Chime,
            enabled: true,
            captures_only: false,
            auto_bookmark: false,
        },
    ];

    let ini = config.to_ini();
    let loaded = FastTailConfig::from_ini(&ini);

    assert_eq!(loaded.highlight_rules.len(), 4);
    assert_eq!(
        loaded.highlight_rules[0].sound_alert,
        SoundAlertPreset::Critical
    );
    assert_eq!(
        loaded.highlight_rules[1].sound_alert,
        SoundAlertPreset::Warning
    );
    assert_eq!(
        loaded.highlight_rules[2].sound_alert,
        SoundAlertPreset::Beep
    );
    assert_eq!(
        loaded.highlight_rules[3].sound_alert,
        SoundAlertPreset::Chime
    );
}

#[test]
fn test_proportional_font_row_height_scaling() {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    let font_id_small = egui::FontId::monospace(8.0);
    let row_height_8 = ctx.fonts_mut(|f| f.row_height(&font_id_small));
    assert!(
        row_height_8 < 16.0,
        "Font size 8.0 row height ({}) should be < 16.0",
        row_height_8
    );

    let font_id_10 = egui::FontId::monospace(10.0);
    let row_height_10 = ctx.fonts_mut(|f| f.row_height(&font_id_10));
    assert!(
        row_height_10 < 16.0,
        "Font size 10.0 row height ({}) should be < 16.0",
        row_height_10
    );
    assert!(
        row_height_10 > row_height_8,
        "Font size 10.0 row height should be greater than font size 8.0"
    );
}

#[test]
fn test_open_files_closed_not_in_open_files() {
    use std::path::PathBuf;

    let mut config = FastTailConfig::default();
    let file1 = PathBuf::from("file1.log");
    let file2 = PathBuf::from("file2.log");

    config.open_files.push(file1.clone());
    config.open_files.push(file2.clone());
    config.recent_files.push(file1.clone());
    config.recent_files.push(file2.clone());

    // User closes file1
    config.open_files.retain(|p| p != &file1);

    assert_eq!(config.open_files.len(), 1);
    assert_eq!(config.open_files[0], file2);
    // Recent files still contains both
    assert_eq!(config.recent_files.len(), 2);
    assert!(config.recent_files.contains(&file1));
    assert!(config.recent_files.contains(&file2));
}

#[test]
fn test_case_sensitive_and_insensitive_filters() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "Line 1: error in module").unwrap();
    writeln!(tmp, "Line 2: ERROR in database").unwrap();
    writeln!(tmp, "Line 3: Error in network").unwrap();
    writeln!(tmp, "Line 4: OK info line").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();

    // Default: non-regex, case-insensitive
    engine.filter_is_regex = false;
    engine.filter_case_sensitive = false;
    engine.set_include_filter("error");

    assert!(engine.is_line_visible(0)); // "error"
    assert!(engine.is_line_visible(1)); // "ERROR"
    assert!(engine.is_line_visible(2)); // "Error"
    assert!(!engine.is_line_visible(3)); // "OK"

    // Switch to case-sensitive
    engine.filter_case_sensitive = true;
    engine.set_include_filter("ERROR");

    assert!(!engine.is_line_visible(0)); // "error" does NOT match "ERROR"
    assert!(engine.is_line_visible(1)); // "ERROR" matches
    assert!(!engine.is_line_visible(2)); // "Error" does NOT match
    assert!(!engine.is_line_visible(3));
}

#[test]
fn test_highlight_rule_case_sensitivity() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "warning: low memory").unwrap();
    writeln!(tmp, "WARNING: high load").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();

    // Case-insensitive rule
    let mut rule_ci = HighlightRule::new("WARNING", [255, 255, 0], [0, 0, 0], false);
    rule_ci.case_sensitive = false;
    engine.set_highlight_rules(vec![rule_ci]);

    assert!(engine.match_highlight("warning: low memory").is_some());
    assert!(engine.match_highlight("WARNING: high load").is_some());

    // Case-sensitive rule
    let mut rule_cs = HighlightRule::new("WARNING", [255, 255, 0], [0, 0, 0], false);
    rule_cs.case_sensitive = true;
    engine.set_highlight_rules(vec![rule_cs]);

    assert!(engine.match_highlight("warning: low memory").is_none());
    assert!(engine.match_highlight("WARNING: high load").is_some());
}

#[test]
fn test_arrow_scrolling_and_scroll_offsets() {
    let mut tmp = NamedTempFile::new().unwrap();
    for i in 0..50 {
        writeln!(tmp, "Line {}: sample log data", i).unwrap();
    }
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.current_scroll_y, 0.0);
    assert_eq!(engine.current_scroll_x, 0.0);

    let row_height = 20.0;
    // Arrow Down simulation
    engine.requested_scroll_y = Some(engine.current_scroll_y + row_height);
    assert_eq!(engine.requested_scroll_y, Some(20.0));

    // Arrow Up simulation
    engine.current_scroll_y = 60.0;
    engine.requested_scroll_y = Some((engine.current_scroll_y - row_height).max(0.0));
    assert_eq!(engine.requested_scroll_y, Some(40.0));

    // Arrow Right simulation
    engine.requested_scroll_x = Some(engine.current_scroll_x + 40.0);
    assert_eq!(engine.requested_scroll_x, Some(40.0));

    // Arrow Left simulation
    engine.current_scroll_x = 50.0;
    engine.requested_scroll_x = Some((engine.current_scroll_x - 40.0).max(0.0));
    assert_eq!(engine.requested_scroll_x, Some(10.0));
}

#[test]
fn test_max_detected_width_and_safe_end_key_scrolling() {
    let mut tmp = NamedTempFile::new().unwrap();
    // Line 1: short
    writeln!(tmp, "Short line 1").unwrap();
    // Line 2: very wide (200 characters)
    let wide_str = "A".repeat(200);
    writeln!(tmp, "{}", wide_str).unwrap();
    // Line 3: short
    writeln!(tmp, "Short line 3").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 3);
    assert_eq!(engine.max_line_bytes, 200);
    assert!(engine.max_detected_width >= 200.0 * 8.0);

    let initial_max_width = engine.max_detected_width;

    // Simulate rendering short lines (width: 150.0) -> max_detected_width should NOT shrink!
    let rendered_short_width = 150.0_f32;
    if rendered_short_width > engine.max_detected_width {
        engine.max_detected_width = rendered_short_width;
    }
    assert_eq!(engine.max_detected_width, initial_max_width);

    // Simulate pressing End key
    let available_width = 800.0_f32;
    let target_x = (engine.max_detected_width - available_width + 100.0).max(0.0);
    engine.requested_scroll_x = Some(target_x);
    assert!(target_x.is_finite());
    assert!(target_x > 0.0);
    assert!(target_x < f32::MAX / 2.0);

    // Simulate pressing Ctrl+End key (vertical jump)
    let row_height = 20.0_f32;
    let max_y = (engine.total_lines() as f32 * row_height).max(0.0);
    engine.requested_scroll_y = Some(max_y);
    assert_eq!(engine.requested_scroll_y, Some(60.0));
    assert!(max_y < f32::MAX / 2.0);
}

#[test]
fn test_crash_handler_git_commit_and_report_generation() {
    use fasttail::crash_handler::{build_crash_report, APP_VERSION, GIT_COMMIT_HASH, GIT_TAG};

    assert!(
        !GIT_COMMIT_HASH.is_empty(),
        "GIT_COMMIT_HASH must not be empty"
    );
    assert!(!GIT_TAG.is_empty(), "GIT_TAG must not be empty");
    assert_eq!(APP_VERSION, env!("CARGO_PKG_VERSION"));

    let bt = std::backtrace::Backtrace::disabled();
    let report = build_crash_report(
        "Explicit panic test message",
        Some("src/ui/dock.rs:364:21"),
        &bt,
    );

    assert!(report.contains("FASTTAIL CRASH REPORT"));
    assert!(report.contains(GIT_COMMIT_HASH));
    assert!(report.contains(GIT_TAG));
    assert!(report.contains("src/ui/dock.rs:364:21"));
    assert!(report.contains("Explicit panic test message"));
    assert!(report.contains("CALLSTACK / BACKTRACE:"));
}

#[test]
fn test_file_not_locked_external_modification_and_rollback() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "Initial log line 1").unwrap();
    writeln!(tmp, "Initial log line 2").unwrap();
    tmp.flush().unwrap();

    let path = tmp.path().to_path_buf();
    let mut engine = TailEngine::open(&path).unwrap();
    assert_eq!(engine.total_lines(), 2);
    assert!(!engine.has_new_data);

    // 1. External process modifies and truncates the file: MUST NOT FAIL OR BE LOCKED
    let mut f_mod = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .expect("External process should not be locked by FastTail!");
    f_mod
        .write_all(b"Modified content line 1\nModified content line 2\nModified line 3\n")
        .unwrap();
    f_mod.flush().unwrap();
    drop(f_mod);

    // FastTail polls updates and detects changes
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 3);
    assert!(engine.has_new_data);
    assert_eq!(engine.get_line(0).unwrap(), "Modified content line 1");

    // Clear new data flag (simulating tab viewed)
    engine.has_new_data = false;

    // 2. External process removes the modification / rewrites original content: MUST NOT FAIL OR BE LOCKED
    let mut f_rollback = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .expect("External rollback should not be locked by FastTail!");
    f_rollback
        .write_all(b"Initial log line 1\nInitial log line 2\n")
        .unwrap();
    f_rollback.flush().unwrap();
    drop(f_rollback);

    // FastTail polls updates and detects rollback
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 2);
    assert!(engine.has_new_data);
    assert_eq!(engine.get_line(0).unwrap(), "Initial log line 1");
    assert_eq!(engine.get_line(1).unwrap(), "Initial log line 2");
}

#[test]
fn test_tab_title_watch_icon_and_new_data_dot() {
    let tmp = NamedTempFile::new().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    let file_name = tmp.path().file_name().unwrap().to_str().unwrap();

    // 1. Watching + No new data: ▶ and ○
    engine.is_watching = true;
    engine.has_new_data = false;
    let watch_icon = if engine.is_watching { "▶" } else { "■" };
    let data_dot = if engine.has_new_data { "●" } else { "○" };
    let title = format!("[#1] {} {} {}", watch_icon, file_name, data_dot);
    assert!(title.contains("▶"));
    assert!(title.contains("○"));

    // 2. Watching + New data arrived: ▶ and ●
    engine.has_new_data = true;
    let watch_icon = if engine.is_watching { "▶" } else { "■" };
    let data_dot = if engine.has_new_data { "●" } else { "○" };
    let title = format!("[#1] {} {} {}", watch_icon, file_name, data_dot);
    assert!(title.contains("▶"));
    assert!(title.contains("●"));

    // 3. Paused + New data: ■ and ●
    engine.is_watching = false;
    let watch_icon = if engine.is_watching { "▶" } else { "■" };
    let data_dot = if engine.has_new_data { "●" } else { "○" };
    let title = format!("[#1] {} {} {}", watch_icon, file_name, data_dot);
    assert!(title.contains("■"));
    assert!(title.contains("●"));
}

#[test]
fn test_screen_based_paging_pageup_pagedown() {
    let row_height = 20.0_f32;
    let visible_lines = 10usize; // screen shows 10 lines

    // Example 1: Currently at line 1 (0-indexed: 0). PageDown must jump to line 11 (0-indexed: 10).
    let current_line_0 = 0usize;
    let pagedown_target_line = current_line_0 + visible_lines;
    assert_eq!(pagedown_target_line, 10); // row 10 in 1-based is Line 11!
    let pagedown_scroll_y = pagedown_target_line as f32 * row_height;
    assert_eq!(pagedown_scroll_y, 200.0);

    // Example 2: Currently at line 103 (0-indexed: 102). PageUp must jump to line 93 (0-indexed: 92).
    let current_line_102 = 102usize; // Line 103
    let pageup_target_line = current_line_102.saturating_sub(visible_lines);
    assert_eq!(pageup_target_line, 92); // row 92 in 1-based is Line 93!
    let pageup_scroll_y = pageup_target_line as f32 * row_height;
    assert_eq!(pageup_scroll_y, 1840.0);
}

#[test]
fn test_filter_virtualization_and_continuous_indexing() {
    let mut tmp = NamedTempFile::new().unwrap();
    // Write 20 lines with varying content
    for i in 0..20 {
        if i % 3 == 0 {
            writeln!(tmp, "Line {}: [ERROR] Serious failure", i).unwrap();
        } else if i % 3 == 1 {
            writeln!(tmp, "Line {}: [INFO] Normal ping probe", i).unwrap();
        } else {
            writeln!(tmp, "Line {}: [DEBUG] Trace telemetry", i).unwrap();
        }
    }
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 20);
    assert!(!engine.is_filter_active());
    assert_eq!(engine.visible_line_count(), 20);

    // 1. Filter by include: [ERROR]
    // Indices: 0, 3, 6, 9, 12, 15, 18 => 7 lines
    engine.set_include_filter("ERROR");
    assert!(engine.is_filter_active());
    assert_eq!(engine.visible_line_count(), 7);
    for row in 0..7 {
        let actual = engine.get_actual_line_idx(row).unwrap();
        assert_eq!(actual, row * 3);
        assert!(engine.get_line(actual).unwrap().contains("[ERROR]"));
        assert_eq!(engine.get_visible_row_of_line(actual), Some(row));
    }

    // 2. Filter by exclude across entire file: discard "ping"
    engine.set_include_filter("");
    engine.set_exclude_filter("ping");
    assert!(engine.is_filter_active());
    // Lines % 3 == 1 contain "ping" (7 lines: 1, 4, 7, 10, 13, 16, 19).
    // Remaining visible lines must be 20 - 7 = 13 lines.
    assert_eq!(engine.visible_line_count(), 13);
    for row in 0..13 {
        let actual = engine.get_actual_line_idx(row).unwrap();
        let content = engine.get_line(actual).unwrap();
        assert!(
            !content.contains("ping"),
            "Excluded line found: {}",
            content
        );
        assert_eq!(engine.get_visible_row_of_line(actual), Some(row));
    }

    // 3. Clear filters: returns to all 20 lines
    engine.set_exclude_filter("");
    assert!(!engine.is_filter_active());
    assert_eq!(engine.visible_line_count(), 20);
}

#[test]
fn test_search_navigation_and_wraparound() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "alpha").unwrap();
    writeln!(tmp, "beta").unwrap();
    writeln!(tmp, "alpha target 1").unwrap();
    writeln!(tmp, "gamma").unwrap();
    writeln!(tmp, "alpha target 2").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.total_lines(), 5);

    // Update search query "alpha"
    engine.update_search("alpha");
    assert_eq!(engine.search_matches, vec![0, 2, 4]);
    assert_eq!(engine.current_match_idx, Some(0));
    assert_eq!(engine.current_search_line(), Some(0));

    // Next match -> 2
    let match2 = engine.search_next(false);
    assert_eq!(match2, Some(2));
    assert_eq!(engine.current_match_idx, Some(1));
    assert_eq!(engine.current_search_line(), Some(2));

    // Next match -> 4
    let match3 = engine.search_next(false);
    assert_eq!(match3, Some(4));
    assert_eq!(engine.current_match_idx, Some(2));
    assert_eq!(engine.current_search_line(), Some(4));

    // Next match wraps around to 0
    let wrapped_match = engine.search_next(false);
    assert_eq!(wrapped_match, Some(0));
    assert_eq!(engine.current_match_idx, Some(0));
    assert_eq!(engine.current_search_line(), Some(0));

    // Prev match wraps around to 4
    let prev_wrapped = engine.search_prev(false);
    assert_eq!(prev_wrapped, Some(4));
    assert_eq!(engine.current_match_idx, Some(2));
    assert_eq!(engine.current_search_line(), Some(4));

    // Prev match back to 2
    let prev_match = engine.search_prev(false);
    assert_eq!(prev_match, Some(2));
    assert_eq!(engine.current_match_idx, Some(1));

    // Empty search clears matches
    engine.update_search("");
    assert!(engine.search_matches.is_empty());
    assert_eq!(engine.current_match_idx, None);
    assert_eq!(engine.current_search_line(), None);
}

#[test]
fn test_search_history_management_and_ini_persistence() {
    let mut config = FastTailConfig::default();
    assert!(config.search_history.is_empty());

    // Add 12 searches: should retain only 10, newest first, deduplicated
    for i in 1..=12 {
        config.add_search_history(&format!("Query {}", i));
    }
    assert_eq!(config.search_history.len(), 10);
    assert_eq!(config.search_history[0], "Query 12");
    assert_eq!(config.search_history[9], "Query 3");

    // Re-adding existing query brings it to front without duplicate
    config.add_search_history("Query 5");
    assert_eq!(config.search_history.len(), 10);
    assert_eq!(config.search_history[0], "Query 5");
    assert_eq!(config.search_history[1], "Query 12");

    // INI serialization & deserialization round-trip
    let ini_obj = config.to_ini();
    assert!(ini_obj.section(Some("search_history")).is_some());
    assert_eq!(
        ini_obj
            .section(Some("search_history"))
            .unwrap()
            .get("query_0"),
        Some("Query 5")
    );

    let reloaded = FastTailConfig::from_ini(&ini_obj);
    assert_eq!(reloaded.search_history.len(), 10);
    assert_eq!(reloaded.search_history[0], "Query 5");
    assert_eq!(reloaded.search_history[1], "Query 12");
}

#[test]
fn test_markdown_mode_detection_and_rendering() {
    let mut tmp = tempfile::Builder::new().suffix(".md").tempfile().unwrap();

    writeln!(
        tmp,
        "# FastTail Documentation\n\nWelcome to **FastTail**!\n\n- Feature 1\n- Feature 2"
    )
    .unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.view_mode, fasttail::tail_engine::ViewMode::Markdown);

    // Switching modes
    engine.view_mode = fasttail::tail_engine::ViewMode::Text;
    assert_eq!(engine.view_mode, fasttail::tail_engine::ViewMode::Text);

    engine.view_mode = fasttail::tail_engine::ViewMode::Hex;
    assert_eq!(engine.view_mode, fasttail::tail_engine::ViewMode::Hex);

    engine.view_mode = fasttail::tail_engine::ViewMode::Markdown;
    assert_eq!(engine.view_mode, fasttail::tail_engine::ViewMode::Markdown);
}

#[test]
fn test_f3_scroll_to_line_target_consistency() {
    let mut tmp = NamedTempFile::new().unwrap();
    for i in 0..100 {
        if i == 15 || i == 45 || i == 85 {
            writeln!(tmp, "Row {}: target token found", i).unwrap();
        } else {
            writeln!(tmp, "Row {}: standard log info line", i).unwrap();
        }
    }
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.update_search("target token");
    assert_eq!(engine.search_matches, vec![15, 45, 85]);

    // F3 -> next match 45
    let next = engine.search_next(false);
    assert_eq!(next, Some(45));
    assert_eq!(engine.scroll_to_line, Some(45));

    // Next F3 -> next match 85
    let next2 = engine.search_next(false);
    assert_eq!(next2, Some(85));
    assert_eq!(engine.scroll_to_line, Some(85));

    // Wrap around to 15
    let wrapped = engine.search_next(false);
    assert_eq!(wrapped, Some(15));
    assert_eq!(engine.scroll_to_line, Some(15));
}

#[test]
fn test_window_bounds_and_dialog_state_ini_persistence() {
    let mut config = FastTailConfig::default();
    config.theme = CyberTheme::Light;
    config.window_x = Some(150.0);
    config.window_y = Some(75.0);
    config.window_width = Some(1600.0);
    config.window_height = Some(950.0);
    config.window_maximized = true;
    config.settings_open = true;
    config.filters_open = true;
    config.about_open = false;
    config.help_open = true;
    config.settings_pos = Some([100.0, 120.0]);
    config.settings_size = Some([480.0, 420.0]);
    config.filters_pos = Some([200.0, 220.0]);
    config.filters_size = Some([560.0, 480.0]);
    config.about_pos = Some([300.0, 320.0]);
    config.about_size = Some([450.0, 350.0]);
    config.help_pos = Some([400.0, 420.0]);
    config.help_size = Some([600.0, 520.0]);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let ini = config.to_ini();
    ini.write_to_file(tmp.path()).unwrap();

    let loaded_ini = ini::Ini::load_from_file(tmp.path()).unwrap();
    let loaded = FastTailConfig::from_ini(&loaded_ini);

    assert_eq!(loaded.theme, CyberTheme::Light);
    assert_eq!(loaded.window_x, Some(150.0));
    assert_eq!(loaded.window_y, Some(75.0));
    assert_eq!(loaded.window_width, Some(1600.0));
    assert_eq!(loaded.window_height, Some(950.0));
    assert!(loaded.window_maximized);
    assert!(loaded.settings_open);
    assert!(loaded.filters_open);
    assert!(!loaded.about_open);
    assert!(loaded.help_open);
    assert_eq!(loaded.settings_pos, Some([100.0, 120.0]));
    assert_eq!(loaded.settings_size, Some([480.0, 420.0]));
    assert_eq!(loaded.filters_pos, Some([200.0, 220.0]));
    assert_eq!(loaded.filters_size, Some([560.0, 480.0]));
    assert_eq!(loaded.about_pos, Some([300.0, 320.0]));
    assert_eq!(loaded.about_size, Some([450.0, 350.0]));
    assert_eq!(loaded.help_pos, Some([400.0, 420.0]));
    assert_eq!(loaded.help_size, Some([600.0, 520.0]));
}

#[test]
fn test_dock_state_floating_window_position_roundtrip() {
    use egui_dock::DockState;
    use fasttail::ui::dock::FastTailTab;
    use std::path::PathBuf;

    let mut dock: DockState<FastTailTab> =
        DockState::new(vec![FastTailTab::LogStream(PathBuf::from("test.log"))]);
    let locator = dock
        .find_tab(&FastTailTab::LogStream(PathBuf::from("test.log")))
        .expect("find tab");
    let surf_idx = dock.detach_tab(
        locator,
        egui::Rect::from_min_size(egui::pos2(120.0, 140.0), egui::vec2(500.0, 350.0)),
    );

    // Set position and size on window state
    if let Some(ws) = dock.get_window_state_mut(surf_idx) {
        ws.set_position(egui::pos2(250.0, 180.0));
        ws.set_size(egui::vec2(720.0, 480.0));
    }

    let ron_str = ron::to_string(&dock).expect("serialize dock state");
    println!("RON:\n{}", ron_str);
    assert!(ron_str.contains("250"));
    assert!(ron_str.contains("180"));
    assert!(ron_str.contains("720"));
    assert!(ron_str.contains("480"));

    let mut loaded: DockState<FastTailTab> =
        ron::from_str(&ron_str).expect("deserialize dock state");
    assert_eq!(loaded.surfaces_count(), 2);

    struct DummyViewer;
    impl egui_dock::TabViewer for DummyViewer {
        type Tab = FastTailTab;
        fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
            egui::Id::new(&*tab)
        }
        fn title(&mut self, _tab: &mut Self::Tab) -> egui::WidgetText {
            "title".into()
        }
        fn ui(&mut self, ui: &mut egui::Ui, _tab: &mut Self::Tab) {
            ui.label("content");
        }
    }

    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let mut viewer = DummyViewer;
        egui_dock::DockArea::new(&mut loaded).show_inside(ui, &mut viewer);
    });
    output.textures_delta.clear();

    let id = egui::Id::new("window SurfaceIndex(1)");
    let rect = ctx.memory(|mem| mem.area_rect(id));
    println!("Restored floating window rect in egui memory: {:?}", rect);
    assert!(rect.is_some());
    let r = rect.unwrap();
    assert_eq!(r.min.x, 250.0);
    assert_eq!(r.min.y, 180.0);
    assert_eq!(r.width(), 720.0);
    assert_eq!(r.height(), 480.0);
}

#[test]
fn test_html_in_markdown_mode_conversion() {
    use fasttail::html_converter::{contains_html, html_to_markdown};

    let sample_html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>System Health</title></head>
        <body>
        <h1>Cluster Status</h1>
        <p>Deployment is <b>HEALTHY</b> and <i>stable</i>.</p>
        <p>View metric graphs at <a href="https://metrics.corp.internal">Dashboard</a>.</p>
        <ul>
            <li>Node 1: UP</li>
            <li>Node 2: UP</li>
        </ul>
        <table>
            <tr><th>Service</th><th>Latency</th></tr>
            <tr><td>Auth</td><td>12ms</td></tr>
            <tr><td>API</td><td>24ms</td></tr>
        </table>
        <hr/>
        <br>
        <pre><code>log_level=debug</code></pre>
        </body>
        </html>
    "#;

    assert!(contains_html(sample_html));
    let md = html_to_markdown(sample_html);
    println!("MD:\n{}", md);

    assert!(md.contains("# Cluster Status") || md.contains("# System Health"));
    assert!(md.contains("**HEALTHY**"));
    assert!(md.contains("*stable*"));
    assert!(md.contains("[Dashboard](https://metrics.corp.internal)"));
    assert!(md.contains("* Node 1: UP"));
    assert!(md.contains("* Node 2: UP"));
    assert!(md.contains("| Service | Latency |"));
    assert!(md.contains("| Auth | 12ms |"));
    assert!(md.contains("---"));
    assert!(md.contains("```\nlog_level=debug\n```"));

    // Verify CommonMarkViewer renders the converted text without panics
    let ctx = egui::Context::default();
    let mut cache = egui_commonmark::CommonMarkCache::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, &md);
    });
    output.textures_delta.clear();
}

#[test]
fn test_light_theme_background_colors_and_visuals() {
    let theme = CyberTheme::Light;
    assert_eq!(theme.bg_color(), egui::Color32::from_rgb(243, 245, 249));
    assert_eq!(theme.panel_bg(), egui::Color32::from_rgb(255, 255, 255));
    assert_eq!(theme.warn_color(), egui::Color32::from_rgb(195, 105, 0));

    let ctx = egui::Context::default();
    theme.apply(&ctx);
    assert!(!ctx.global_style().visuals.dark_mode);
}

#[test]
fn test_ctrl_f_focus_and_search() {
    use egui_dock::DockState;
    use fasttail::i18n::Language;
    use fasttail::tail_engine::TailEngine;
    use fasttail::theme::CyberTheme;
    use fasttail::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut tmp = NamedTempFile::new().unwrap();
    for i in 0..50 {
        writeln!(tmp, "Line {}: sample log content with error and warning", i).unwrap();
    }
    tmp.flush().unwrap();

    let path = tmp.path().to_path_buf();
    let engine = TailEngine::open(&path).expect("open temp file");
    let mut engines = vec![engine];
    let mut open_files = vec![path.clone()];
    let mut theme = CyberTheme::Tron;
    let mut lang = Language::En;
    let mut global_rules = Vec::new();
    let mut screensaver_enabled = false;
    let mut screensaver_timeout_mins = 5;
    let mut telemetry_enabled = false;
    let mut sound_enabled = false;
    let mut borderless = false;
    let mut show_line_numbers = true;
    let mut font_size = 13.0;
    let mut level_colors = true;
    let mut size_unit = fasttail::tail_engine::SizeUnit::Bytes;
    let mut search_history = Vec::new();
    let mut tab_closed = false;
    let mut test_screensaver = false;
    let focused_stream = Some(path.clone());

    let mut dock: DockState<FastTailTab> =
        DockState::new(vec![FastTailTab::LogStream(path.clone())]);

    let ctx = egui::Context::default();

    // Frame 1: Initial render
    let mut out1 = ctx.run_ui(Default::default(), |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out1.textures_delta.clear();

    // Frame 2: Press Ctrl + F
    let raw_input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }],
        ..Default::default()
    };
    let mut out2 = ctx.run_ui(raw_input, |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out2.textures_delta.clear();

    // Frame 3: Next frame where focus is applied
    let mut out3 = ctx.run_ui(Default::default(), |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out3.textures_delta.clear();

    // Frame 4: Type "error" in the tab's search query and run
    engines[0].search_query.push_str("error");
    let mut out4 = ctx.run_ui(Default::default(), |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out4.textures_delta.clear();
    assert_eq!(engines[0].search_matches.len(), 50);

    // Frame 5: Press Enter
    let raw_enter = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut out5 = ctx.run_ui(raw_enter, |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out5.textures_delta.clear();

    // Frame 6: Feed Event::Text("\u{0006}") while focused
    let raw_ctrl_f_char = egui::RawInput {
        events: vec![egui::Event::Text("\u{0006}".to_string())],
        ..Default::default()
    };
    let mut out6 = ctx.run_ui(raw_ctrl_f_char, |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out6.textures_delta.clear();
    println!(
        "search_query after ctrl+f char: {:?}",
        engines[0].search_query
    );
}

#[test]
fn test_search_query_is_per_tab() {
    use egui_dock::{DockState, NodeIndex};
    use fasttail::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};

    let mut tmp1 = NamedTempFile::new().unwrap();
    writeln!(tmp1, "file 1 line 1").unwrap();
    writeln!(tmp1, "file 1 line 2").unwrap();
    tmp1.flush().unwrap();

    let mut tmp2 = NamedTempFile::new().unwrap();
    writeln!(tmp2, "file 2 line 1").unwrap();
    writeln!(tmp2, "file 2 line 2").unwrap();
    tmp2.flush().unwrap();

    let path1 = tmp1.path().to_path_buf();
    let path2 = tmp2.path().to_path_buf();
    let mut engines = vec![
        TailEngine::open(&path1).expect("open file 1"),
        TailEngine::open(&path2).expect("open file 2"),
    ];
    let mut open_files = vec![path1.clone(), path2.clone()];
    let mut theme = CyberTheme::Tron;
    let mut lang = Language::En;
    let mut global_rules = Vec::new();
    let mut screensaver_enabled = false;
    let mut screensaver_timeout_mins = 5;
    let mut telemetry_enabled = false;
    let mut sound_enabled = false;
    let mut borderless = false;
    let mut show_line_numbers = true;
    let mut font_size = 13.0;
    let mut level_colors = true;
    let mut size_unit = fasttail::tail_engine::SizeUnit::Bytes;
    let mut search_history = Vec::new();
    let mut tab_closed = false;
    let mut test_screensaver = false;
    let focused_stream = Some(path1.clone());

    // Both tabs visible at once (side by side) so both stream panels render each frame
    let mut dock: DockState<FastTailTab> =
        DockState::new(vec![FastTailTab::LogStream(path1.clone())]);
    dock.main_surface_mut().split_right(
        NodeIndex::root(),
        0.5,
        vec![FastTailTab::LogStream(path2.clone())],
    );

    let ctx = egui::Context::default();

    // Type a query only in the first tab's search box
    engines[0].search_query.push_str("line 1");

    let mut out = ctx.run_ui(Default::default(), |ui| {
        let dock_ctx = DockContext {
            engines: &mut engines,
            open_files: &mut open_files,
            theme: &mut theme,
            language: &mut lang,
            global_rules: &mut global_rules,
            screensaver_enabled: &mut screensaver_enabled,
            screensaver_timeout_mins: &mut screensaver_timeout_mins,
            telemetry_enabled: &mut telemetry_enabled,
            sound_enabled: &mut sound_enabled,
            borderless: &mut borderless,
            show_line_numbers: &mut show_line_numbers,
            font_size: &mut font_size,
            level_colors: &mut level_colors,
            auto_highlight: &mut false,
            auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
            size_unit: &mut size_unit,
            search_history: &mut search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            language_auto: &mut false,
            lock_enabled: &mut false,
            lock_pin: &mut String::new(),
            lock_now: &mut false,
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
            find_all: &mut fasttail::find_all::FindAllSession::default(),
            scratchpad: &mut Default::default(),
            compare: &mut None,
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out.textures_delta.clear();

    // First tab searched, second tab untouched
    assert_eq!(engines[0].search_query, "line 1");
    assert_eq!(engines[0].search_matches, vec![0]);
    assert!(
        engines[1].search_query.is_empty(),
        "search query leaked into the second tab"
    );
    assert!(
        engines[1].search_matches.is_empty(),
        "search matches leaked into the second tab"
    );
}

#[test]
fn test_fasttail_app_ctrl_f() {
    use fasttail::config::FastTailConfig;
    use fasttail::ui::FastTailApp;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut tmp1 = NamedTempFile::new().unwrap();
    writeln!(tmp1, "file 1 line 1").unwrap();
    writeln!(tmp1, "file 1 line 2").unwrap();
    tmp1.flush().unwrap();

    let mut tmp2 = NamedTempFile::new().unwrap();
    writeln!(tmp2, "file 2 line 1").unwrap();
    writeln!(tmp2, "file 2 line 2").unwrap();
    tmp2.flush().unwrap();

    let mut config = FastTailConfig::default();
    config.open_files = vec![tmp1.path().to_path_buf(), tmp2.path().to_path_buf()];

    let mut app = FastTailApp::from_config(config);
    let ctx = egui::Context::default();

    // Frame 1: Initial render
    let mut out1 = ctx.run_ui(Default::default(), |ui| {
        app.render_ui(ui);
    });
    out1.textures_delta.clear();

    // Frame 2: Press Ctrl + F
    let raw_ctrl_f = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }],
        ..Default::default()
    };
    let mut out2 = ctx.run_ui(raw_ctrl_f, |ui| {
        app.render_ui(ui);
    });
    out2.textures_delta.clear();

    // Frame 3: Next frame (search input focused)
    let mut out3 = ctx.run_ui(Default::default(), |ui| {
        app.render_ui(ui);
    });
    out3.textures_delta.clear();

    // Frame 4: ArrowDown while focused to scroll/navigate
    let raw_down = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut out4 = ctx.run_ui(raw_down, |ui| {
        app.render_ui(ui);
    });
    out4.textures_delta.clear();

    // Frame 5: Escape to release focus
    let raw_esc = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut out5 = ctx.run_ui(raw_esc, |ui| {
        app.render_ui(ui);
    });
    out5.textures_delta.clear();
}

#[test]
fn test_mouse_throttle_interval_software_vs_hardware() {
    use fasttail::ui::app::mouse_throttle_interval_us;

    // Hardware renderer: the user setting in microseconds, verbatim (0 = no throttle).
    assert_eq!(mouse_throttle_interval_us(false, 100), 100_000);
    assert_eq!(mouse_throttle_interval_us(false, 0), 0);
    assert_eq!(mouse_throttle_interval_us(false, 1000), 1_000_000);

    // Software rasterizer: capped at 5 fps (200 ms) but never above the user setting,
    // so 0 keeps meaning "no throttling".
    assert_eq!(mouse_throttle_interval_us(true, 1000), 200_000);
    assert_eq!(mouse_throttle_interval_us(true, 250), 200_000);
    assert_eq!(mouse_throttle_interval_us(true, 100), 100_000);
    assert_eq!(mouse_throttle_interval_us(true, 0), 0);
}

#[test]
fn test_software_renderer_strips_costly_visuals() {
    use fasttail::ui::app::apply_renderer_visuals;

    let ctx = egui::Context::default();

    // Hardware renderer: stock theme visuals.
    apply_renderer_visuals(&ctx, false, CyberTheme::Tron);
    assert!(
        ctx.tessellation_options(|o| o.feathering),
        "hardware rendering keeps feathering"
    );
    let stock_window_shadow = egui::Visuals::dark().window_shadow;
    assert_eq!(
        ctx.style_of(egui::Theme::Dark).visuals.window_shadow,
        stock_window_shadow
    );

    // Software rasterizer: feathering off, shadows and rounded corners stripped.
    apply_renderer_visuals(&ctx, true, CyberTheme::Tron);
    assert!(
        !ctx.tessellation_options(|o| o.feathering),
        "software rendering must disable feathering"
    );
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        assert_eq!(s.visuals.window_shadow, egui::Shadow::NONE);
        assert_eq!(s.visuals.popup_shadow, egui::Shadow::NONE);
        assert_eq!(
            s.visuals.widgets.hovered.corner_radius,
            egui::CornerRadius::same(0)
        );
        assert_eq!(s.visuals.window_corner_radius, egui::CornerRadius::same(0));
    });

    // Switching back to a hardware renderer must restore the stock theme visuals.
    apply_renderer_visuals(&ctx, false, CyberTheme::Tron);
    assert!(
        ctx.tessellation_options(|o| o.feathering),
        "hardware rendering restores feathering"
    );
    assert_eq!(
        ctx.style_of(egui::Theme::Dark).visuals.window_shadow,
        stock_window_shadow
    );
}

#[test]
fn test_a_log_opened_empty_detects_its_encoding_once_it_has_a_sample() {
    use fasttail::tail_engine::{FileEncoding, ViewMode};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("late.log");
    std::fs::write(&path, b"").unwrap();
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    assert!(engine.encoding_pending);

    // UTF-16 LE without a BOM, first under the sample size, then past it.
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect() };
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(&utf16("hi\n")).unwrap();
    f.flush().unwrap();
    engine.poll_updates();
    assert!(engine.encoding_pending, "a few bytes are not a sample");
    f.write_all(&utf16(&"line\n".repeat(100))).unwrap();
    f.flush().unwrap();
    engine.poll_updates();
    assert!(!engine.encoding_pending);
    assert_eq!(engine.encoding, FileEncoding::UnicodeLe);
    assert_eq!(engine.view_mode, ViewMode::Text);
    assert_eq!(engine.total_lines(), 101);
    assert_eq!(engine.get_line(100).as_deref(), Some("line"));

    // A file that had content when it was opened is left alone.
    let full = dir.path().join("full.log");
    std::fs::write(&full, b"already here\n").unwrap();
    assert!(!TailEngine::open(&full).unwrap().encoding_pending);
}

#[test]
fn test_empty_stream_i18n_messages() {
    let lang = Language::En;
    assert_ne!(t(lang, "file_empty"), "Unknown");
    assert_ne!(t(lang, "no_matching_lines"), "Unknown");
    assert!(t(lang, "file_empty").contains("empty"));
    assert!(t(lang, "no_matching_lines").contains("filters"));
}

#[test]
fn test_zip_picker_i18n_keys() {
    for lang in Language::ALL {
        let text = t(*lang, "zip_picker_no_selection");
        assert!(
            !text.is_empty() && text != "Unknown",
            "zip_picker_no_selection missing for {lang:?}"
        );
    }
    assert_eq!(
        t(Language::En, "zip_picker_no_selection"),
        "Select at least one entry to open"
    );
    assert_eq!(
        t(Language::It, "zip_picker_no_selection"),
        "Seleziona almeno una voce da aprire"
    );
}

/// Every Rust source file starts with the project, copyright and licence header.
#[test]
fn every_rust_file_has_the_source_header() {
    const HEADER: &str = "// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.\n\
                          // Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail\n\
                          // SPDX-License-Identifier: MIT\n";
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // The maintainer's own logs live under tests/logs.
                if path.file_name().is_some_and(|n| n != "logs") {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("build.rs")];
    for dir in ["src", "tests", "benches"] {
        walk(&root.join(dir), &mut files);
    }
    let missing: Vec<String> = files
        .iter()
        .filter(|path| path.exists())
        .filter(|path| {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            !text.replace("\r\n", "\n").starts_with(HEADER)
        })
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "files without the source header: {missing:?}"
    );
    assert!(files.len() > 80, "found only {} files", files.len());
}

#[test]
fn tray_options_round_trip_and_default_off() {
    let off = FastTailConfig::default();
    assert!(!off.tray_icon && !off.minimize_to_tray && !off.close_to_tray);
    let on = FastTailConfig {
        tray_icon: true,
        minimize_to_tray: true,
        close_to_tray: true,
        ..FastTailConfig::default()
    };
    let back = FastTailConfig::from_ini(&on.to_ini());
    assert!(back.tray_icon && back.minimize_to_tray && back.close_to_tray);
}
