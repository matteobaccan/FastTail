#![allow(
    clippy::field_reassign_with_default,
    clippy::bool_assert_comparison,
    clippy::write_with_newline
)]

use fasttail::config::FastTailConfig;
use fasttail::i18n::{t, Language};
use fasttail::screensaver::MatrixScreensaver;
use fasttail::tail_engine::{HighlightRule, TailEngine};
use fasttail::theme::CyberTheme;
use std::io::Write;
use std::time::Duration;
use tempfile::NamedTempFile;

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
    let manifest = include_str!("../Cargo.toml");
    let eframe_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("eframe"))
        .expect("Cargo.toml declares the eframe dependency");
    assert!(
        eframe_line.contains("\"links\""),
        "the eframe dependency must enable the `links` feature, otherwise the About dialog links cannot open the browser: {eframe_line}"
    );
}

#[test]
fn test_i18n_exhaustive_coverage() {
    let all_keys = [
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
        "zip_entry_encrypted",
        "zip_entry_method",
        "zip_entry_unsafe",
        "zip_entry_duplicate",
        "zip_empty",
        "zip_no_entry",
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
        assert!(
            !up.is_empty() && up != "Unknown",
            "move_up missing for {lang:?}"
        );
        assert!(
            !down.is_empty() && down != "Unknown",
            "move_down missing for {lang:?}"
        );
    }
    assert_eq!(t(Language::En, "move_up"), "Move Up");
    assert_eq!(t(Language::En, "move_down"), "Move Down");
    assert_eq!(t(Language::It, "move_up"), "Sposta su");
    assert_eq!(t(Language::It, "move_down"), "Sposta giù");
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
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

// ---------------------------------------------------------------------------
// Regression tests for the 2026-09-17 code review findings
// ---------------------------------------------------------------------------

#[test]
fn test_config_test_binary_detection_ignores_release_names() {
    use fasttail::config::is_test_binary;
    use std::path::Path;

    // Cargo test binaries live in target/<profile>/deps and carry a hash suffix
    assert!(is_test_binary(Path::new(
        "target/debug/deps/fasttail-1a2b3c4d.exe"
    )));
    assert!(is_test_binary(Path::new(
        "target/debug/deps/integration_tests-1a2b3c4d"
    )));
    // Release artifacts published by CI must NOT be treated as test binaries
    assert!(!is_test_binary(Path::new(
        "C:/Tools/fasttail-windows-x86_64.exe"
    )));
    assert!(!is_test_binary(Path::new(
        "/usr/local/bin/fasttail-linux-x86_64"
    )));
    assert!(!is_test_binary(Path::new(
        "/Applications/fasttail-macos-arm64"
    )));
    assert!(!is_test_binary(Path::new("C:/Tools/fasttail.exe")));
}

#[test]
fn test_config_legacy_toml_without_search_history_still_loads() {
    let legacy = r#"
theme = "Matrix"
language = "It"
screensaver_enabled = true
screensaver_timeout_mins = 7
telemetry_enabled = false
sound_enabled = true
recent_files = ["old.log"]
highlight_rules = []
baretail_prompt_shown = true
"#;
    let cfg: FastTailConfig = toml::from_str(legacy).expect("legacy toml must still parse");
    assert_eq!(cfg.theme, CyberTheme::Matrix);
    assert_eq!(cfg.screensaver_timeout_mins, 7);
    assert!(cfg.search_history.is_empty());
}

#[test]
fn test_config_default_screensaver_timeout_is_ten_minutes() {
    assert_eq!(FastTailConfig::default().screensaver_timeout_mins, 10);
}

#[test]
fn test_push_search_history_matches_config_helper() {
    use fasttail::config::push_search_history;
    let mut history = vec!["old".to_string()];
    push_search_history(&mut history, "  Error ");
    push_search_history(&mut history, "error");
    assert_eq!(history, vec!["error".to_string(), "old".to_string()]);

    let mut cfg = FastTailConfig::default();
    cfg.search_history = vec!["old".to_string()];
    cfg.add_search_history("  Error ");
    cfg.add_search_history("error");
    assert_eq!(cfg.search_history, history);
}

#[test]
fn test_search_matches_refresh_after_append_and_keep_position() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "ERROR one").unwrap();
    writeln!(tmp, "info").unwrap();
    writeln!(tmp, "ERROR two").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.update_search("ERROR");
    assert_eq!(engine.search_matches, vec![0, 2]);
    engine.search_next(false);
    assert_eq!(engine.current_search_line(), Some(2));

    writeln!(tmp, "ERROR three").unwrap();
    writeln!(tmp, "ERROR four").unwrap();
    tmp.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    engine.poll_updates();
    engine.update_search("ERROR");

    assert_eq!(
        engine.search_matches,
        vec![0, 2, 3, 4],
        "new matches must be picked up"
    );
    assert_eq!(
        engine.current_search_line(),
        Some(2),
        "current match must be preserved"
    );
    assert_eq!(engine.search_next(false), Some(3));
}

#[test]
fn test_search_matches_refresh_when_filter_changes() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "ERROR payment failed").unwrap();
    writeln!(tmp, "ERROR disk full").unwrap();
    writeln!(tmp, "INFO payment ok").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.update_search("ERROR");
    assert_eq!(engine.search_matches, vec![0, 1]);

    engine.set_include_filter("payment");
    engine.update_search("ERROR");
    assert_eq!(
        engine.search_matches,
        vec![0],
        "hidden lines must drop out of the search"
    );
    assert_eq!(engine.current_search_line(), Some(0));

    engine.set_include_filter("");
    engine.update_search("ERROR");
    assert_eq!(engine.search_matches, vec![0, 1]);
}

#[test]
fn test_include_filter_is_not_bypassed_by_highlight_rules() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "ERROR payment failed").unwrap();
    writeln!(tmp, "ERROR disk full").unwrap();
    writeln!(tmp, "INFO payment ok").unwrap();
    writeln!(tmp, "INFO idle").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_highlight_rules(vec![HighlightRule {
        pattern: "ERROR".to_string(),
        is_regex: false,
        case_sensitive: false,
        fg_color: [255, 0, 0],
        bg_color: [0, 0, 0],
        bold: false,
        italic: false,
        sound_alert: fasttail::audio::SoundAlertPreset::None,
        enabled: true,
        captures_only: false,
    }]);
    engine.set_include_filter("payment");

    assert_eq!(
        engine.filtered_lines,
        vec![0, 2],
        "only include-filter matches are visible"
    );
    assert!(
        !engine.is_line_visible(1),
        "highlighted line without 'payment' must be hidden"
    );
    assert_eq!(engine.visible_line_count(), 2);
}

#[test]
fn test_filtered_lines_incremental_update_on_append() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "keep 1").unwrap();
    writeln!(tmp, "drop 1").unwrap();
    write!(tmp, "kee").unwrap(); // partial last line, completed by a later append
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_include_filter("keep");
    assert_eq!(engine.filtered_lines, vec![0]);

    writeln!(tmp, "p 2").unwrap();
    writeln!(tmp, "drop 2").unwrap();
    writeln!(tmp, "keep 3").unwrap();
    tmp.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    engine.poll_updates();

    assert_eq!(engine.total_lines(), 5);
    assert_eq!(engine.filtered_lines, vec![0, 2, 4]);
}

#[test]
fn test_contains_html_ignores_markdown_generics_and_code() {
    use fasttail::html_converter::contains_html;

    assert!(!contains_html("let v: Vec<i32> = Vec::new();"));
    assert!(!contains_html("Returns an Option<bool> for the flag"));
    assert!(!contains_html("Usage: tool <path> [options]"));
    assert!(!contains_html("```html\n<div>inside a fence</div>\n```"));
    assert!(!contains_html("if a < b && c > d { }"));

    assert!(contains_html("<p>Hello <b>world</b></p>"));
    assert!(contains_html("<html><body>x</body></html>"));
    assert!(contains_html("line one<br>line two"));
}

#[test]
fn test_html_converter_entities_pre_blocks_and_empty_links() {
    use fasttail::html_converter::{decode_html_entities, html_to_markdown};

    // Escaped entities must not be double-decoded
    assert_eq!(decode_html_entities("&amp;lt;b&amp;gt;"), "&lt;b&gt;");
    assert_eq!(decode_html_entities("A &amp; B &lt; C"), "A & B < C");

    // Tags inside <pre> are content, not markup
    let md = html_to_markdown("<pre>&lt;div class=\"x\"&gt;hi&lt;/div&gt;</pre>");
    assert!(md.contains("<div class=\"x\">hi</div>"), "got: {md}");

    // Generics inside converted code blocks survive
    let md =
        html_to_markdown("<p>Intro</p><pre><code>let v: Vec&lt;i32&gt; = vec![];</code></pre>");
    assert!(md.contains("Vec<i32>"), "got: {md}");

    // An anchor without text keeps its url
    let md = html_to_markdown(r#"<p>See <a href="https://example.com"></a></p>"#);
    assert!(md.contains("https://example.com"), "got: {md}");
}

#[test]
fn test_markdown_text_is_cached_per_buffer_generation() {
    let mut tmp = tempfile::Builder::new().suffix(".html").tempfile().unwrap();
    writeln!(tmp, "<h1>Title</h1><p>Hello <b>there</b></p>").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    let first = engine.markdown_text().to_string();
    assert!(first.contains("# Title"));
    assert!(first.contains("**there**"));
    let first_ptr = engine.markdown_text().as_ptr();
    assert_eq!(
        first_ptr,
        engine.markdown_text().as_ptr(),
        "same buffer must be served from cache"
    );

    writeln!(tmp, "<p>More <i>text</i></p>").unwrap();
    tmp.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    engine.poll_updates();
    let second = engine.markdown_text().to_string();
    assert!(
        second.contains("*text*"),
        "cache must refresh after the buffer changes: {second}"
    );
}

#[test]
fn test_mode_switch_tooltips_are_localized() {
    for lang in [
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        for key in ["tip_mode_txt", "tip_mode_hex", "tip_mode_md"] {
            let text = t(lang, key);
            assert!(
                !text.is_empty() && text != key,
                "missing translation {key} for {lang:?}"
            );
        }
    }
    assert!(t(Language::En, "tip_mode_txt").contains("Text"));
    assert!(t(Language::It, "tip_mode_txt").contains("Testo"));
}

#[test]
fn test_theme_exposes_tab_backgrounds() {
    for theme in [
        CyberTheme::Tron,
        CyberTheme::Matrix,
        CyberTheme::Blade,
        CyberTheme::Light,
    ] {
        assert_ne!(theme.tab_active_bg(), theme.tab_inactive_bg());
    }
}

#[test]
fn test_pointer_move_counts_as_user_activity_for_screensaver() {
    use fasttail::ui::FastTailApp;

    let mut config = FastTailConfig::default();
    config.screensaver_enabled = true;
    let mut app = FastTailApp::from_config(config);
    let ctx = egui::Context::default();

    let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
    out.textures_delta.clear();

    app.screensaver.last_input_time = std::time::Instant::now() - Duration::from_secs(90);
    let raw = egui::RawInput {
        events: vec![egui::Event::PointerMoved(egui::pos2(100.0, 100.0))],
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_ui(ui));
    out.textures_delta.clear();

    assert!(
        app.screensaver.last_input_time.elapsed() < Duration::from_secs(5),
        "mouse movement must reset the idle timer"
    );
}

#[cfg(windows)]
#[test]
fn test_tab_lookup_tolerates_path_case_differences() {
    use egui_dock::DockState;
    use fasttail::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "hello").unwrap();
    tmp.flush().unwrap();

    let real_path = tmp.path().to_path_buf();
    let upper_path = std::path::PathBuf::from(real_path.to_string_lossy().to_uppercase());
    assert_ne!(real_path, upper_path);

    let mut engines = vec![TailEngine::open(&real_path).unwrap()];
    let mut open_files = vec![real_path.clone()];
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
    let focused_stream = Some(upper_path.clone());
    let mut dock: DockState<FastTailTab> =
        DockState::new(vec![FastTailTab::LogStream(upper_path.clone())]);

    let ctx = egui::Context::default();
    let mut title_text = String::new();
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
        };
        let mut viewer = FastTailTabViewer { ctx: dock_ctx };
        use egui_dock::TabViewer;
        let mut tab = FastTailTab::LogStream(upper_path.clone());
        title_text = viewer.title(&mut tab).text().to_string();
        egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
    });
    out.textures_delta.clear();

    assert!(
        !title_text.contains(t(Language::En, "closed")),
        "tab must resolve its engine: {title_text}"
    );
}

#[test]
fn test_f3_only_advances_the_focused_tab() {
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
    engines[0].search_query.push_str("line");
    engines[1].search_query.push_str("line");
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
    // The first tab is the "current window"
    let focused_stream = Some(path1.clone());

    let mut dock: DockState<FastTailTab> =
        DockState::new(vec![FastTailTab::LogStream(path1.clone())]);
    dock.main_surface_mut().split_right(
        NodeIndex::root(),
        0.5,
        vec![FastTailTab::LogStream(path2.clone())],
    );

    let ctx = egui::Context::default();
    let f3 = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::F3,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };

    // Frame 1 computes the matches, frame 2 presses F3
    for raw in [egui::RawInput::default(), f3] {
        let mut out = ctx.run_ui(raw, |ui| {
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
                filter_presets: &mut Vec::new(),
                preset_events: &mut Default::default(),
            };
            let mut viewer = FastTailTabViewer { ctx: dock_ctx };
            egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
        });
        out.textures_delta.clear();
    }

    assert_eq!(engines[0].search_matches, vec![0, 1]);
    assert_eq!(engines[1].search_matches, vec![0, 1]);
    assert_eq!(
        engines[0].current_match_idx,
        Some(1),
        "F3 must advance the focused tab"
    );
    assert_eq!(
        engines[1].current_match_idx,
        Some(0),
        "F3 must not touch the other tab"
    );
}

#[test]
fn test_hex_search_matches_bytes_across_rows() {
    use fasttail::tail_engine::ViewMode;

    let mut tmp = NamedTempFile::new().unwrap();
    // "needle" starts at byte 15 and spans the 16-byte row boundary
    tmp.write_all(b"AAAAAAAAAAAAAAAneedle rest of data that fills more rows")
        .unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_view_mode(ViewMode::Hex);
    engine.update_search("NEEDLE");

    assert_eq!(engine.search_byte_matches, vec![(15, 6)]);
    assert_eq!(engine.active_match_count(), 1);
    assert!(
        engine.hex_row_matches(0, 16),
        "row 0 holds the first byte of the match"
    );
    assert!(
        engine.hex_row_matches(16, 32),
        "row 1 holds the tail of the match"
    );
    assert!(!engine.hex_row_matches(32, 48));

    // F3 in hex mode navigates byte offsets
    assert_eq!(engine.current_search_byte(), Some((15, 6)));
    assert_eq!(engine.search_next(false), Some(15));

    // A hex byte pattern is searched too ("6E 65" == "ne")
    engine.update_search("6E 65");
    assert_eq!(engine.search_byte_matches, vec![(15, 2)]);
}

#[test]
fn test_switching_view_mode_keeps_search_cursor_valid() {
    use fasttail::tail_engine::ViewMode;

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "abc").unwrap();
    writeln!(tmp, "abc").unwrap();
    writeln!(tmp, "abc").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.update_search("abc");
    engine.search_next(false);
    engine.search_next(false);
    assert_eq!(engine.current_match_idx, Some(2));

    engine.set_view_mode(ViewMode::Hex);
    // 3 byte matches as well ("abc" x3), cursor stays in range
    assert_eq!(engine.active_match_count(), 3);
    assert!(engine.current_match_idx.unwrap() < 3);
    assert!(engine.search_next(false).is_some());
}

#[test]
fn test_tail_engine_rejects_non_regular_files() {
    let tmp_dir = tempfile::tempdir().unwrap();
    match TailEngine::open(tmp_dir.path()) {
        Ok(_) => panic!("Expected TailEngine::open to fail for directory"),
        Err(err) => assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput),
    }
}

#[test]
fn test_prune_floating_window_rects_ignores_stale_surface_index() {
    use fasttail::ui::app::prune_floating_window_rects;

    let mut dock: egui_dock::DockState<String> =
        egui_dock::DockState::new(vec!["main".to_string()]);
    let win = dock.add_window(vec!["floating".to_string()]);
    let mut rects = std::collections::HashMap::new();
    rects.insert(
        win,
        egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 100.0)),
    );
    rects.insert(
        egui_dock::SurfaceIndex::main(),
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1.0, 1.0)),
    );

    // The live floating window is kept, the main surface is not a window.
    prune_floating_window_rects(&dock, &mut rects);
    assert_eq!(rects.len(), 1);
    assert!(rects.contains_key(&win));

    // Closing the window leaves a stale index behind: pruning must not panic
    // (v0.1.0 crashed here with "index out of bounds") and must drop it.
    dock.remove_surface(win);
    prune_floating_window_rects(&dock, &mut rects);
    assert!(rects.is_empty());
}

#[test]
fn test_renderer_choice_persists_in_ini() {
    use fasttail::renderer::RendererChoice;

    let mut cfg = FastTailConfig::default();
    assert_eq!(cfg.renderer, RendererChoice::Auto);

    cfg.renderer = RendererChoice::Wgpu;
    let ini = cfg.to_ini();
    assert_eq!(
        ini.section(Some("general")).and_then(|s| s.get("renderer")),
        Some("wgpu")
    );
    let restored = FastTailConfig::from_ini(&ini);
    assert_eq!(restored.renderer, RendererChoice::Wgpu);

    // Software rendering force survives the ini round trip too.
    cfg.renderer = RendererChoice::Software;
    let ini = cfg.to_ini();
    assert_eq!(
        ini.section(Some("general")).and_then(|s| s.get("renderer")),
        Some("software")
    );
    assert_eq!(
        FastTailConfig::from_ini(&ini).renderer,
        RendererChoice::Software
    );

    // Unknown values fall back to the default instead of failing the whole config.
    let mut bad = cfg.to_ini();
    bad.with_section(Some("general")).set("renderer", "vulkan");
    assert_eq!(
        FastTailConfig::from_ini(&bad).renderer,
        RendererChoice::Auto
    );
}

#[test]
fn test_cli_paths_open_streams_with_filters_and_follow() {
    use fasttail::cli::CliArgs;
    use fasttail::ui::FastTailApp;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("cli.log");
    std::fs::File::create(&log)
        .unwrap()
        .write_all(b"INFO ok\nERROR boom\nERROR healthcheck failed\n")
        .unwrap();
    let missing = dir.path().join("missing.log");

    let cli = CliArgs::parse(
        [
            "--filter",
            "error",
            "--exclude",
            "healthcheck",
            "--no-follow",
            &log.to_string_lossy(),
            &missing.to_string_lossy(),
        ],
        dir.path(),
    )
    .unwrap();

    let mut app = FastTailApp::from_config(FastTailConfig::default());
    app.apply_cli(&cli);

    // The existing file is open once, the missing one is skipped.
    assert_eq!(app.engines.len(), 1);
    let engine = &app.engines[0];
    assert_eq!(engine.include_filter(), "error");
    assert_eq!(engine.exclude_filter(), "healthcheck");
    assert!(!engine.follow_tail);
    assert_eq!(engine.visible_line_count(), 1);

    // Opening the same path again from the command line does not duplicate the stream.
    app.apply_cli(&cli);
    assert_eq!(app.engines.len(), 1);
}

#[test]
fn test_compressed_files_open_as_streams_and_zip_bundles_offer_their_entries() {
    use fasttail::ui::FastTailApp;
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..FastTailConfig::default()
    };
    let mut app = FastTailApp::from_config(config);

    // A gzip without the usual extension opens decompressed, follow locked off.
    let gz = dir.path().join("trace.dat");
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    enc.write_all(b"one\ntwo\n").unwrap();
    std::fs::write(&gz, enc.finish().unwrap()).unwrap();
    app.open_log_file(gz.clone());
    assert_eq!(app.engines.len(), 1);
    assert!(app.engines[0].is_compressed());
    assert_eq!(app.engines[0].path, gz);
    assert!(!app.engines[0].follow_tail);

    // A bundle with two files and a folder: the picker lists the two files.
    let bundle = dir.path().join("bundle.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&bundle).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.add_directory("config/", options).unwrap();
    for name in ["server.log", "worker.log"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(b"started\n").unwrap();
    }
    zip.finish().unwrap();
    app.open_log_file(bundle.clone());
    assert_eq!(app.engines.len(), 1, "nothing opens before a choice");
    let picker = app.zip_picker.take().expect("the entry picker is shown");
    let names: Vec<&str> = picker.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["server.log", "worker.log"]);

    // Each chosen entry is its own stream, keyed by archive + entry.
    for name in names {
        app.open_log_file(fasttail::compressed::entry_path(&bundle, name));
    }
    assert_eq!(app.engines.len(), 3);
    let worker = fasttail::compressed::entry_path(&bundle, "worker.log");
    let engine = app.engines.iter().find(|e| e.path == worker).unwrap();
    assert_eq!(
        engine.compressed.as_ref().unwrap().title(),
        "bundle.zip › worker.log"
    );
    assert_eq!(engine.source_file(), bundle);
    let spool = engine
        .compressed
        .as_ref()
        .unwrap()
        .spool_path()
        .to_path_buf();
    // Opening it again selects the existing tab.
    app.open_log_file(worker.clone());
    assert_eq!(app.engines.len(), 3);

    // Closing a stream deletes its spool.
    assert!(spool.exists());
    app.engines.retain(|e| e.path != worker);
    assert!(!spool.exists());

    // An empty zip says so instead of opening anything.
    let empty = dir.path().join("empty.zip");
    zip::ZipWriter::new(std::fs::File::create(&empty).unwrap())
        .finish()
        .unwrap();
    app.open_log_file(empty);
    assert_eq!(app.engines.len(), 2);
    assert!(app.open_notice.is_some());
}

fn write_lines(path: &std::path::Path, lines: &[&str]) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

#[test]
fn test_row_selection_range_toggle_and_select_all_under_filter() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("sel.log");
    write_lines(
        &log,
        &[
            "a ERROR 0",
            "b INFO 1",
            "c ERROR 2",
            "d INFO 3",
            "e ERROR 4",
        ],
    );
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_include_filter("ERROR"); // visible: 0, 2, 4

    engine.select_row(0);
    engine.extend_selection_to(4);
    assert_eq!(
        engine.selected_lines(),
        vec![0, 2, 4],
        "hidden rows stay unselected"
    );

    engine.toggle_row(2);
    assert_eq!(engine.selected_lines(), vec![0, 4]);
    engine.toggle_row(2);
    assert_eq!(engine.selected_lines(), vec![0, 2, 4]);

    engine.select_all_visible();
    assert!(engine.is_selected(4) && !engine.is_selected(1));
    assert_eq!(engine.selected_lines(), vec![0, 2, 4]);
    engine.toggle_row(4);
    assert_eq!(engine.selected_lines(), vec![0, 2]);

    engine.clear_selection();
    assert!(!engine.has_selection());
}

#[test]
fn test_copy_selection_text_is_plain_lines_in_file_order() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("copy.log");
    write_lines(&log, &["first", "second", "third"]);
    let mut engine = TailEngine::open(&log).unwrap();

    assert_eq!(engine.copy_selection_text(), None);

    engine.toggle_row(2);
    engine.toggle_row(0);
    assert_eq!(
        engine.copy_selection_text().as_deref(),
        Some("first\nthird")
    );

    // No selection: the current search hit is copied.
    engine.clear_selection();
    engine.update_search("second");
    assert_eq!(engine.copy_selection_text().as_deref(), Some("second"));
}

#[test]
fn test_selection_cleared_on_truncation() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("trunc.log");
    write_lines(&log, &["one", "two", "three"]);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.select_row(2);
    assert!(engine.has_selection());

    std::fs::File::create(&log)
        .unwrap()
        .write_all(b"x\n")
        .unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 1);
    assert!(!engine.has_selection());
}

#[test]
fn test_export_visible_and_search_matches() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("exp.log");
    write_lines(&log, &["ERROR a", "INFO b", "ERROR healthcheck", "WARN c"]);
    let mut engine = TailEngine::open(&log).unwrap();

    engine.set_exclude_filter("healthcheck");
    let mut out = Vec::new();
    let n = engine.export_visible(&mut out).unwrap();
    assert_eq!(n, 3);
    assert_eq!(String::from_utf8(out).unwrap(), "ERROR a\nINFO b\nWARN c\n");

    engine.update_search("ERROR");
    let mut out = Vec::new();
    let n = engine.export_search_matches(&mut out).unwrap();
    assert_eq!(n, 1, "only lines visible under the filters are searchable");
    assert_eq!(String::from_utf8(out).unwrap(), "ERROR a\n");
}

#[test]
fn test_export_rejects_non_regular_files() {
    use fasttail::ui::dock::create_export_file;

    let dir = tempfile::tempdir().unwrap();
    let target_dir = dir.path().join("sub_directory");
    std::fs::create_dir(&target_dir).unwrap();

    let result = create_export_file(&target_dir);
    assert!(
        result.is_err(),
        "create_export_file for directory must fail"
    );

    #[cfg(unix)]
    {
        let fifo_path = dir.path().join("test_fifo");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo_path)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            let res = create_export_file(&fifo_path);
            let err = res.expect_err("create_export_file for FIFO must fail");
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        }
    }
}

#[test]
fn test_goto_line_exact_clamped_hidden_and_relative() {
    use fasttail::tail_engine::GotoTarget;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("goto.log");
    write_lines(&log, &["ERROR 1", "INFO 2", "INFO 3", "ERROR 4", "INFO 5"]);
    let mut engine = TailEngine::open(&log).unwrap();

    // Exact, 1-based in, 0-based out.
    assert_eq!(
        engine.resolve_goto("3", 0),
        Some(GotoTarget {
            requested: 2,
            line: 2,
            hidden: false,
            waiting: false
        })
    );
    // Beyond the end clamps to the last line.
    assert_eq!(engine.resolve_goto("500", 0).unwrap().line, 4);
    // Relative to the current line.
    assert_eq!(engine.resolve_goto("+2", 1).unwrap().line, 3);
    assert_eq!(engine.resolve_goto("-9", 1).unwrap().line, 0);
    // Not a number, or line 0.
    assert_eq!(engine.resolve_goto("abc", 0), None);
    assert_eq!(engine.resolve_goto("0", 0), None);

    // Under a filter a hidden line resolves to the next visible one and says so.
    engine.set_include_filter("ERROR"); // visible: 0, 3
    assert_eq!(
        engine.resolve_goto("2", 0),
        Some(GotoTarget {
            requested: 1,
            line: 3,
            hidden: true,
            waiting: false
        })
    );
    // Past the last visible line: the last visible line.
    assert_eq!(engine.resolve_goto("5", 0).unwrap().line, 3);
}

#[test]
fn test_always_on_top_persists_in_ini() {
    let mut cfg = FastTailConfig::default();
    assert!(!cfg.always_on_top);
    cfg.always_on_top = true;
    let ini = cfg.to_ini();
    assert_eq!(
        ini.section(Some("general"))
            .and_then(|s| s.get("always_on_top")),
        Some("true")
    );
    assert!(FastTailConfig::from_ini(&ini).always_on_top);
}

#[test]
fn test_bookmarks_toggle_navigate_and_wrap_under_filter() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bm.log");
    write_lines(&log, &["a", "b KEEP", "c", "d", "e", "f KEEP", "g"]);
    let mut engine = TailEngine::open(&log).unwrap();

    engine.toggle_bookmark(1);
    engine.toggle_bookmark(3);
    engine.toggle_bookmark(5);
    assert!(engine.has_bookmarks() && engine.is_bookmarked(3));
    engine.toggle_bookmark(3);
    assert!(!engine.is_bookmarked(3));
    engine.toggle_bookmark(3);
    engine.bookmark_cursor = None;

    // Row 3 is hidden by the filter: navigation skips it but keeps it.
    engine.set_exclude_filter("d"); // hides only row 3 ("d")
    assert!(!engine.is_line_visible(3));
    assert_eq!(engine.bookmark_next(0), Some(1));
    assert_eq!(engine.bookmark_next(0), Some(5));
    assert_eq!(engine.bookmark_next(0), Some(1), "wraps around");
    assert_eq!(engine.bookmark_prev(0), Some(5), "wraps backwards");
    assert!(engine.is_bookmarked(3), "hidden bookmark is kept");

    engine.clear_bookmarks();
    assert!(!engine.has_bookmarks());
    assert_eq!(engine.bookmark_next(0), None);
}

#[test]
fn test_bookmarks_cleared_on_truncation() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bmt.log");
    write_lines(&log, &["one", "two", "three"]);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.toggle_bookmark(2);
    std::fs::File::create(&log)
        .unwrap()
        .write_all(b"x\n")
        .unwrap();
    engine.poll_updates();
    assert!(!engine.has_bookmarks());
}

#[test]
fn test_bookmarks_persist_in_config_with_caps() {
    use fasttail::config::{MAX_BOOKMARKS_PER_FILE, MAX_BOOKMARK_FILES};
    let mut cfg = FastTailConfig::default();
    let path = std::path::PathBuf::from(if cfg!(windows) {
        r"C:\logs\app.log"
    } else {
        "/logs/app.log"
    });

    cfg.set_bookmarks(&path, &[200, 10]);
    let restored = FastTailConfig::from_ini(&cfg.to_ini());
    assert_eq!(restored.bookmarks_for(&path, 300), Some(vec![10, 200]));
    assert_eq!(
        restored.bookmarks_for(&path, 50),
        None,
        "file shrank below the largest index"
    );

    // Empty list removes the entry.
    cfg.set_bookmarks(&path, &[]);
    assert!(cfg.bookmarks_for(&path, 1000).is_none());

    // Per-file and file-count caps.
    let many: Vec<usize> = (0..MAX_BOOKMARKS_PER_FILE + 500).collect();
    cfg.set_bookmarks(&path, &many);
    assert_eq!(
        cfg.bookmarks_for(&path, usize::MAX).unwrap().len(),
        MAX_BOOKMARKS_PER_FILE
    );
    for i in 0..MAX_BOOKMARK_FILES + 10 {
        cfg.set_bookmarks(&path.with_file_name(format!("f{i}.log")), &[1]);
    }
    assert_eq!(cfg.bookmarks.len(), MAX_BOOKMARK_FILES);
    assert!(
        cfg.bookmarks_for(&path, usize::MAX).is_none(),
        "oldest entry evicted"
    );
}

#[test]
fn test_unseen_lines_counted_only_while_hidden() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("badge.log");
    write_lines(&log, &["start"]);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_highlight_rules(vec![HighlightRule::new(
        "ERROR",
        [255, 0, 0],
        [0, 0, 0],
        false,
    )]);

    // Hidden tab: appended lines are counted, a rule match raises the severity.
    engine.displayed = false;
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"plain\nERROR boom\n").unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.unseen_lines, 2);
    assert_eq!(engine.unseen_severity, 1);

    // Displayed: the viewer clears the counter and nothing accrues.
    engine.mark_seen();
    assert_eq!(engine.unseen_lines, 0);
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"more\n").unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.unseen_lines, 0);
}

#[test]
fn test_incremental_index_completes_partial_line_across_polls() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("partial.log");
    std::fs::File::create(&log)
        .unwrap()
        .write_all(b"one\ntwo\nabc")
        .unwrap();
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.total_lines(), 3);
    assert_eq!(engine.get_line(2).unwrap(), "abc");

    // The writer completes the partial line and adds two more.
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"def\nghi\njkl\n").unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 5);
    assert_eq!(engine.get_line(2).unwrap(), "abcdef");
    assert_eq!(engine.get_line(3).unwrap(), "ghi");
    assert_eq!(engine.get_line(4).unwrap(), "jkl");
    assert_eq!(engine.max_line_bytes, 6);

    // A second append keeps the earlier offsets intact.
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"a much longer line\n").unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 6);
    assert_eq!(engine.get_line(0).unwrap(), "one");
    assert_eq!(engine.get_line(5).unwrap(), "a much longer line");
    assert_eq!(engine.max_line_bytes, 18);
}

#[test]
fn test_incremental_index_utf16_append() {
    use fasttail::tail_engine::FileEncoding;
    use std::io::Write;
    fn utf16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("u16.log");
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(utf16le("uno\ndue\n"));
    std::fs::File::create(&log)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.encoding, FileEncoding::UnicodeLe);
    assert_eq!(engine.total_lines(), 2);

    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(&utf16le("tre\nquattro\n")).unwrap();
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 4);
    assert_eq!(engine.get_line(0).unwrap(), "uno");
    assert_eq!(engine.get_line(2).unwrap(), "tre");
    assert_eq!(engine.get_line(3).unwrap(), "quattro");
}

#[test]
fn test_reset_and_regrow_with_same_header_reloads_from_start() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("reset.log");
    // 100 lines with an identical 64+ byte header prefix, as a formatted logger would write.
    let header = "2026-09-19 10:00:00.000 [INFO] service-1 req=000000001 message ";
    let mut f = std::fs::File::create(&log).unwrap();
    for i in 0..100 {
        writeln!(f, "{header}old-{i}").unwrap();
    }
    drop(f);
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.total_lines(), 100);

    // Between two polls the file is reset and refilled past its old size with the same header.
    let mut f = std::fs::File::create(&log).unwrap();
    for i in 0..150 {
        writeln!(f, "{header}new-{i}").unwrap();
    }
    drop(f);
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 150);
    assert_eq!(engine.get_line(0).unwrap(), format!("{header}new-0"));
    assert_eq!(engine.get_line(149).unwrap(), format!("{header}new-149"));
    assert!(
        (0..150).all(|i| !engine.get_line(i).unwrap().contains("old-")),
        "no old content spliced with the new file"
    );
}

#[test]
fn test_every_line_reads_back_across_block_boundaries() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("blocks.log");
    let mut f = std::io::BufWriter::new(std::fs::File::create(&log).unwrap());
    // ~700 KB: spans three 256 KB cache blocks, with lines of uneven length.
    for i in 0..7000 {
        writeln!(f, "line {i:05} {}", "x".repeat(60 + (i % 37))).unwrap();
    }
    drop(f);
    let content = std::fs::read_to_string(&log).unwrap();
    let engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.total_lines(), 7000);
    for (i, expected) in content.lines().enumerate() {
        assert_eq!(engine.get_line(i).as_deref(), Some(expected), "line {i}");
    }
    // The cache stayed bounded while walking the whole file.
    assert!(
        engine.source.cached_bytes()
            <= fasttail::file_source::MAX_BLOCKS * fasttail::file_source::BLOCK_SIZE
    );
}

#[test]
fn test_long_line_is_truncated_with_marker() {
    use fasttail::tail_engine::{MAX_LINE_BYTES, TRUNCATED_LINE_MARKER};
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("long.log");
    let mut f = std::fs::File::create(&log).unwrap();
    f.write_all(b"short\n").unwrap();
    f.write_all(&vec![b'y'; MAX_LINE_BYTES + 500_000]).unwrap();
    f.write_all(b"\nafter\n").unwrap();
    drop(f);
    let engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.total_lines(), 3);
    let long = engine.get_line(1).unwrap();
    assert!(long.ends_with(TRUNCATED_LINE_MARKER));
    assert_eq!(long.len(), MAX_LINE_BYTES + TRUNCATED_LINE_MARKER.len());
    assert_eq!(engine.get_line(2).as_deref(), Some("after"));
}

#[test]
fn test_truncation_drops_cache_and_index_memory() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("shrink.log");
    let mut f = std::io::BufWriter::new(std::fs::File::create(&log).unwrap());
    for i in 0..50_000 {
        writeln!(f, "row {i} {}", "z".repeat(40)).unwrap();
    }
    drop(f);
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.total_lines(), 50_000);
    let _ = engine.get_line(49_999);
    assert!(engine.source.cached_bytes() > 0);
    assert!(engine.line_offsets.capacity() >= 50_000);

    std::fs::File::create(&log)
        .unwrap()
        .write_all(b"fresh\n")
        .unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 1);
    assert_eq!(engine.get_line(0).as_deref(), Some("fresh"));
    assert!(
        engine.line_offsets.capacity() < 1_000,
        "index memory returned"
    );
}

#[test]
fn test_markdown_mode_refuses_files_over_the_cap() {
    use fasttail::tail_engine::{ViewMode, MARKDOWN_MAX_BYTES};
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("big.md");
    let mut f = std::io::BufWriter::new(std::fs::File::create(&log).unwrap());
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..(MARKDOWN_MAX_BYTES / (1024 * 1024) + 1) {
        f.write_all(&chunk).unwrap();
        f.write_all(b"\n").unwrap();
    }
    drop(f);
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.view_mode, ViewMode::Text);
    assert!(engine.markdown_too_large());
    assert_eq!(engine.markdown_text(), "");
    engine.set_view_mode(ViewMode::Markdown);
    assert_eq!(engine.view_mode, ViewMode::Text);

    // If limit is raised, switching to Markdown becomes possible
    engine.set_markdown_max_bytes(10 * 1024 * 1024);
    assert!(!engine.markdown_too_large());
    engine.set_view_mode(ViewMode::Markdown);
    assert_eq!(engine.view_mode, ViewMode::Markdown);
    assert!(!engine.markdown_text().is_empty());
}

/// Polls the engine until no background scan is running (or 30 s passed).
fn wait_for_jobs(engine: &mut TailEngine) {
    let start = std::time::Instant::now();
    loop {
        engine.poll_updates();
        if engine.scan_progress().is_none() && !engine.index_pending {
            return;
        }
        assert!(
            start.elapsed().as_secs() < 30,
            "background scan did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn write_scenario_log(path: &std::path::Path, lines: usize) {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 0..lines {
        let level = match i % 10 {
            0 => "ERROR",
            1 | 2 => "WARN",
            _ => "INFO",
        };
        writeln!(
            f,
            "2026-09-19 10:00:{:02} [{level}] svc-{} req={i} payload {}",
            i % 60,
            i % 7,
            "p".repeat(i % 50)
        )
        .unwrap();
    }
}

#[test]
fn test_background_filter_equals_synchronous_filter() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bg.log");
    write_scenario_log(&log, 40_000);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_include_filter("ERROR");
    sync.set_exclude_filter("svc-3");
    assert!(sync.scan_progress().is_none());
    let expected = sync.filtered_lines.clone();
    assert!(!expected.is_empty());

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.set_include_filter("ERROR");
    bg.set_exclude_filter("svc-3");
    assert!(bg.scan_progress().is_some(), "large-file path spawns a job");
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, expected);
    assert_eq!(bg.visible_line_count(), expected.len());
}

#[test]
fn test_newer_filter_cancels_the_running_job() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("cancel.log");
    write_scenario_log(&log, 60_000);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_include_filter("WARN");
    let expected = sync.filtered_lines.clone();

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.set_include_filter("ERROR");
    bg.set_include_filter("WARN"); // replaces the ERROR job before it finishes
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, expected);
    assert!(
        bg.filtered_lines.windows(2).all(|w| w[0] < w[1]),
        "results in order"
    );
}

#[test]
fn test_appends_during_a_filter_job_are_applied_afterwards() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("append_job.log");
    write_scenario_log(&log, 60_000);

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.set_include_filter("ERROR");
    assert!(bg.scan_progress().is_some());
    // Lines arrive while the job scans: two match, one does not.
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    f.write_all(b"late ERROR one\nlate INFO two\nlate ERROR three\n")
        .unwrap();
    drop(f);
    bg.poll_updates(); // notices the growth while the job is still running
    wait_for_jobs(&mut bg);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_include_filter("ERROR");
    assert_eq!(bg.total_lines(), 60_003);
    assert_eq!(bg.filtered_lines, sync.filtered_lines);
    assert!(bg.filtered_lines.contains(&60_000) && bg.filtered_lines.contains(&60_002));
    assert!(!bg.filtered_lines.contains(&60_001));
}

#[test]
fn test_background_search_matches_synchronous_search() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("search_job.log");
    write_scenario_log(&log, 40_000);

    let mut sync = TailEngine::open(&log).unwrap();
    sync.set_include_filter("WARN");
    sync.update_search("svc-5");
    let expected = sync.search_matches.clone();
    assert!(!expected.is_empty());

    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    bg.set_include_filter("WARN");
    wait_for_jobs(&mut bg);
    bg.update_search("svc-5");
    assert!(bg.scan_progress().is_some());
    wait_for_jobs(&mut bg);
    assert_eq!(bg.search_matches, expected);
    assert_eq!(bg.current_match_idx, Some(0));
    assert_eq!(bg.search_next(false), Some(expected[1]));
}

#[test]
fn test_index_job_builds_the_same_index_as_the_synchronous_path() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("index_job.log");
    write_scenario_log(&log, 50_000);

    let sync = TailEngine::open(&log).unwrap();
    let mut bg = TailEngine::open_with_thresholds(&log, 0, 0).unwrap();
    assert!(bg.index_pending);
    wait_for_jobs(&mut bg);
    assert!(!bg.index_pending);
    assert_eq!(bg.total_lines(), sync.total_lines());
    assert_eq!(bg.line_offsets, sync.line_offsets);
    assert_eq!(bg.max_line_bytes, sync.max_line_bytes);
    assert_eq!(bg.get_line(49_999), sync.get_line(49_999));

    // Filters queued behind the index job run once it is done.
    bg.set_include_filter("ERROR");
    wait_for_jobs(&mut bg);
    let mut sync2 = TailEngine::open(&log).unwrap();
    sync2.set_include_filter("ERROR");
    assert_eq!(bg.filtered_lines, sync2.filtered_lines);
}

// ---------------------------------------------------------------------------
// Log level detection, level cache, minimum-level filter and counters
// ---------------------------------------------------------------------------

fn write_level_log(path: &std::path::Path) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    writeln!(f, "2026-09-18 12:00:00 [INFO] service up").unwrap();
    writeln!(f, "2026-09-18 12:00:01 [DEBUG] cache warm").unwrap();
    writeln!(f, "2026-09-18 12:00:02 [WARN] slow query").unwrap();
    writeln!(f, "2026-09-18 12:00:03 [ERROR] payment failed").unwrap();
    writeln!(f, "    at com.example.Pay(Pay.java:42)").unwrap();
    writeln!(f, "    at com.example.Main(Main.java:7)").unwrap();
    writeln!(f, "plain line without a level").unwrap();
    writeln!(f, "<2>kernel: out of memory").unwrap();
    writeln!(f, "2026-09-18 12:00:05 [TRACE] tick").unwrap();
    writeln!(f, "INFO user typed \"error\" in the search box").unwrap();
}

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

fn expected_svc3_warn_or_error(lines: usize) -> usize {
    (0..lines).filter(|i| i % 10 < 3 && i % 7 == 3).count()
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

// ---------------------------------------------------------------------------
// Line wrap: per-stream toggle persisted in the workspace, i18n keys.
// ---------------------------------------------------------------------------

#[test]
fn test_wrap_persists_in_config_with_cap() {
    use fasttail::config::MAX_WRAPPED_FILES;
    let mut cfg = FastTailConfig::default();
    let path = std::path::PathBuf::from(if cfg!(windows) {
        r"C:\logs\app.log"
    } else {
        "/logs/app.log"
    });
    assert!(!cfg.wrap_for(&path), "off by default");

    cfg.set_wrap(&path, true);
    let restored = FastTailConfig::from_ini(&cfg.to_ini());
    assert!(restored.wrap_for(&path), "wrap survives the INI round trip");
    assert!(
        !restored.wrap_for(&path.with_file_name("other.log")),
        "other files stay unwrapped"
    );

    // Turning it off removes the entry.
    cfg.set_wrap(&path, false);
    assert!(!cfg.wrap_for(&path));
    assert!(cfg.wrapped_files.is_empty());
    assert!(cfg.to_ini().section(Some("wrapped_files")).is_none());

    // Toggling twice keeps a single entry; the file count is capped.
    cfg.set_wrap(&path, true);
    cfg.set_wrap(&path, true);
    assert_eq!(cfg.wrapped_files.len(), 1);
    for i in 0..MAX_WRAPPED_FILES + 10 {
        cfg.set_wrap(&path.with_file_name(format!("f{i}.log")), true);
    }
    assert_eq!(cfg.wrapped_files.len(), MAX_WRAPPED_FILES);
    assert!(
        !cfg.wrap_for(&path),
        "the oldest entry is dropped past the cap"
    );
}

#[test]
fn test_engine_wrap_toggle_keeps_top_row_and_marks_dirty() {
    use fasttail::wrap_layout::WrapScroll;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrap.log");
    std::fs::write(&path, "a\nb\nc\nd\n").unwrap();
    let mut engine = TailEngine::open(&path).unwrap();
    assert!(!engine.wrap_lines);
    assert!(!engine.wrap_dirty);

    engine.follow_tail = false;
    engine.set_wrap_lines(true, 2);
    assert!(engine.wrap_lines && engine.wrap_dirty);
    assert_eq!(engine.wrap_anchor.row, 2, "the viewport keeps its top row");
    assert_eq!(engine.wrap_request, None);

    engine.wrap_dirty = false;
    engine.set_wrap_lines(true, 3);
    assert!(!engine.wrap_dirty, "no-op toggle does not mark dirty");

    // Back to extend mode marks dirty again; the renderer re-derives the scroll offset.
    engine.set_wrap_lines(false, 5);
    assert!(!engine.wrap_lines && engine.wrap_dirty);

    // With follow mode on, enabling wrap asks the renderer for the bottom.
    engine.follow_tail = true;
    engine.set_wrap_lines(true, 0);
    assert_eq!(engine.wrap_request, Some(WrapScroll::Bottom));
    assert!(engine.wrap_at_bottom);
}

#[test]
fn test_wrap_i18n_keys() {
    for lang in [
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        for key in [
            "tip_wrap",
            "help_desc_wrap",
            "open_pattern",
            "open_pattern_tip",
            "open_pattern_desc",
            "open_pattern_hint",
            "open_pattern_invalid",
            "open_pattern_go",
            "open_pattern_cancel",
            "pattern_waiting",
            "pattern_switched",
        ] {
            assert_ne!(t(lang, key), "Unknown", "{key} missing for {lang:?}");
        }
    }
}

mod capture_group_highlight {
    use fasttail::tail_engine::{HighlightRule, QuickLabel, SpanStyle, TailEngine, MAX_ROW_SPANS};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn engine() -> TailEngine {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "2026-09-18 req=1234 sess-8f3a payment ok").unwrap();
        tmp.flush().unwrap();
        TailEngine::open(tmp.path()).unwrap()
    }

    fn ranges(spans: &[fasttail::tail_engine::HighlightSpan]) -> Vec<(usize, usize)> {
        spans.iter().map(|s| (s.start, s.end)).collect()
    }

    #[test]
    fn captures_only_paints_the_captured_group() {
        let mut e = engine();
        e.set_highlight_rules(vec![HighlightRule::captures(
            r"req=(\d+)",
            [0, 255, 255],
            [0, 0, 0],
        )]);
        assert!(e.has_span_rules());
        let line = "2026-09-18 req=1234 sess-8f3a req=99 payment ok";
        let hl = e.match_highlight_spans(line);
        assert_eq!(ranges(&hl.spans), vec![(15, 19), (34, 36)]);
        assert_eq!(&line[15..19], "1234");
        match hl.spans[0].style {
            SpanStyle::Rule(style) => assert_eq!(style.fg, egui::Color32::from_rgb(0, 255, 255)),
            other => panic!("unexpected style {other:?}"),
        }
        assert!(
            hl.rest.is_none(),
            "captures-only rules never colour the whole row"
        );
        // the whole-row path ignores captures-only rules
        assert!(e.match_highlight(line).is_none());
    }

    #[test]
    fn captures_only_without_groups_paints_the_whole_match() {
        let mut e = engine();
        e.set_highlight_rules(vec![HighlightRule::captures(
            r"sess-[0-9a-f]+",
            [1, 2, 3],
            [0, 0, 0],
        )]);
        let hl = e.match_highlight_spans("a sess-8f3a b SESS-00 c");
        assert_eq!(ranges(&hl.spans), vec![(2, 11), (14, 21)]);
    }

    #[test]
    fn whole_row_rules_keep_colouring_the_row_and_the_fast_path_is_used() {
        let mut e = engine();
        e.set_highlight_rules(vec![HighlightRule::new(
            "payment",
            [0, 255, 0],
            [0, 0, 0],
            false,
        )]);
        assert!(
            !e.has_span_rules(),
            "no span rule or label: the renderer keeps match_highlight"
        );
        let hl = e.match_highlight_spans("req=1 payment ok");
        assert!(hl.spans.is_empty());
        assert_eq!(
            hl.rest.map(|s| s.fg),
            Some(egui::Color32::from_rgb(0, 255, 0))
        );
    }

    #[test]
    fn first_rule_wins_per_byte() {
        let mut e = engine();
        // 1: captures rule claims the digits; 2: whole-row rule colours the rest
        e.set_highlight_rules(vec![
            HighlightRule::captures(r"req=(\d+)", [9, 9, 9], [0, 0, 0]),
            HighlightRule::new("payment", [0, 255, 0], [0, 0, 0], false),
        ]);
        let hl = e.match_highlight_spans("req=1234 payment");
        assert_eq!(ranges(&hl.spans), vec![(4, 8)]);
        assert_eq!(
            hl.rest.map(|s| s.fg),
            Some(egui::Color32::from_rgb(0, 255, 0))
        );

        // whole-row rule first: it claims every byte, later captures paint nothing
        e.set_highlight_rules(vec![
            HighlightRule::new("payment", [0, 255, 0], [0, 0, 0], false),
            HighlightRule::captures(r"req=(\d+)", [9, 9, 9], [0, 0, 0]),
        ]);
        let hl = e.match_highlight_spans("req=1234 payment");
        assert!(hl.spans.is_empty());
        assert!(hl.rest.is_some());

        // two captures rules overlapping: the first keeps its bytes, the second gets the rest
        e.set_highlight_rules(vec![
            HighlightRule::captures(r"(1234)", [1, 1, 1], [0, 0, 0]),
            HighlightRule::captures(r"req=(\d+) pay", [2, 2, 2], [0, 0, 0]),
        ]);
        let hl = e.match_highlight_spans("req=01234 payment");
        assert_eq!(ranges(&hl.spans), vec![(4, 5), (5, 9)]);
        assert!(
            matches!(hl.spans[0].style, SpanStyle::Rule(s) if s.fg == egui::Color32::from_rgb(2, 2, 2))
        );
        assert!(
            matches!(hl.spans[1].style, SpanStyle::Rule(s) if s.fg == egui::Color32::from_rgb(1, 1, 1))
        );
    }

    #[test]
    fn spans_are_capped_per_row() {
        let mut e = engine();
        e.set_highlight_rules(vec![HighlightRule::captures(r"(x)", [1, 1, 1], [0, 0, 0])]);
        let line = "x ".repeat(200);
        let hl = e.match_highlight_spans(&line);
        assert_eq!(hl.spans.len(), MAX_ROW_SPANS);
        let mut labels = Vec::new();
        QuickLabel::toggle(&mut labels, "x", 3);
        e.set_highlight_rules(Vec::new());
        e.set_quick_labels(&labels);
        assert_eq!(e.match_highlight_spans(&line).spans.len(), MAX_ROW_SPANS);
    }

    #[test]
    fn quick_labels_toggle_and_rank_below_rules() {
        let mut labels = Vec::new();
        assert!(
            !QuickLabel::toggle(&mut labels, "   ", 2),
            "blank text is ignored"
        );
        assert!(QuickLabel::toggle(&mut labels, "sess-8f3a", 2));
        assert_eq!(labels.len(), 1);
        // same text, other colour: recoloured, not duplicated
        assert!(QuickLabel::toggle(&mut labels, "SESS-8F3A", 5));
        assert_eq!(
            labels,
            vec![QuickLabel {
                text: "sess-8f3a".into(),
                color: 5
            }]
        );
        // same text and colour: removed
        assert!(QuickLabel::toggle(&mut labels, "sess-8f3a", 5));
        assert!(labels.is_empty());

        QuickLabel::toggle(&mut labels, "Sess-8f3a", 2);
        let mut e = engine();
        e.set_quick_labels(&labels);
        assert!(e.has_span_rules());
        assert_eq!(e.quick_labels(), labels);
        let hl = e.match_highlight_spans("x SESS-8f3a y sess-8f3a");
        assert_eq!(ranges(&hl.spans), vec![(2, 11), (14, 23)]);
        assert!(matches!(hl.spans[0].style, SpanStyle::Label(2)));

        // a whole-row rule matching the row wins over the label
        e.set_highlight_rules(vec![HighlightRule::new(
            "sess",
            [7, 7, 7],
            [0, 0, 0],
            false,
        )]);
        let hl = e.match_highlight_spans("x SESS-8f3a y");
        assert!(hl.spans.is_empty());
        assert_eq!(
            hl.rest.map(|s| s.fg),
            Some(egui::Color32::from_rgb(7, 7, 7))
        );
        // a captures rule claims its bytes first, the label gets the rest
        e.set_highlight_rules(vec![HighlightRule::captures(
            r"(8f3a)",
            [7, 7, 7],
            [0, 0, 0],
        )]);
        let hl = e.match_highlight_spans("x SESS-8f3a y");
        assert_eq!(ranges(&hl.spans), vec![(2, 7), (7, 11)]);
        assert!(matches!(hl.spans[0].style, SpanStyle::Label(2)));
        assert!(matches!(hl.spans[1].style, SpanStyle::Rule(_)));

        // non-ASCII label text: byte offsets map back to the original characters
        let mut labels = Vec::new();
        QuickLabel::toggle(&mut labels, "ÜBER", 1);
        e.set_highlight_rules(Vec::new());
        e.set_quick_labels(&labels);
        let line = "über ÜBER x";
        let hl = e.match_highlight_spans(line);
        assert_eq!(ranges(&hl.spans), vec![(0, 5), (6, 11)]);
        assert_eq!(&line[6..11], "ÜBER");
    }

    #[test]
    fn captures_only_round_trips_through_the_ini() {
        let mut config = fasttail::config::FastTailConfig::default();
        config.highlight_rules = vec![
            HighlightRule::captures(r"req=(\d+)", [0, 255, 255], [0, 0, 0]),
            HighlightRule::new("ERROR", [255, 0, 0], [0, 0, 0], false),
        ];
        let loaded = fasttail::config::FastTailConfig::from_ini(&config.to_ini());
        assert_eq!(loaded.highlight_rules.len(), 2);
        assert!(loaded.highlight_rules[0].captures_only);
        assert!(loaded.highlight_rules[0].is_regex);
        assert!(!loaded.highlight_rules[1].captures_only);
    }
}

// ----- Pattern streams (directory wildcard tail) -----

fn write_file_with_mtime(path: &std::path::Path, content: &str, secs_ago: u64) {
    std::fs::write(path, content).unwrap();
    let when = std::time::SystemTime::now() - Duration::from_secs(secs_ago);
    let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(when).unwrap();
}

#[test]
fn test_wildcard_matcher_public_api() {
    use fasttail::wildcard::{is_pattern_path, wildcard_match};
    use std::path::Path;
    assert!(wildcard_match("app-*.log", "app-2026-09-18.log"));
    assert!(wildcard_match("app-????-??-??.log", "app-2026-09-18.log"));
    assert!(!wildcard_match("app-*.log", "other-2026-09-18.log"));
    assert!(!wildcard_match("app-?.log", "app-10.log"));
    assert!(is_pattern_path(Path::new("logs/app-*.log")));
    assert!(!is_pattern_path(Path::new("logs/app.log")));
}

#[test]
fn test_resolve_newest_by_mtime_then_name() {
    use fasttail::wildcard::resolve_newest;
    let dir = tempfile::tempdir().unwrap();
    write_file_with_mtime(&dir.path().join("app-2026-09-17.log"), "old\n", 300);
    write_file_with_mtime(&dir.path().join("app-2026-09-18.log"), "mid\n", 200);
    write_file_with_mtime(&dir.path().join("app-2026-09-19.log"), "new\n", 100);
    write_file_with_mtime(&dir.path().join("other-2026-09-20.log"), "x\n", 0);
    std::fs::create_dir(dir.path().join("app-2026-09-30.log")).unwrap(); // a directory, ignored
    let newest = resolve_newest(dir.path(), "app-*.log").unwrap();
    assert_eq!(newest, dir.path().join("app-2026-09-19.log"));

    // Same modification time: the greater name wins.
    write_file_with_mtime(&dir.path().join("app-2026-09-21.log"), "a\n", 100);
    write_file_with_mtime(&dir.path().join("app-2026-09-20.log"), "b\n", 100);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(dir.path().join("app-2026-09-19.log"))
        .unwrap();
    let t = std::fs::metadata(dir.path().join("app-2026-09-21.log"))
        .unwrap()
        .modified()
        .unwrap();
    f.set_modified(t).unwrap();
    for name in ["app-2026-09-20.log", "app-2026-09-19.log"] {
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join(name))
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    let newest = resolve_newest(dir.path(), "app-*.log").unwrap();
    assert_eq!(newest, dir.path().join("app-2026-09-21.log"));

    assert!(resolve_newest(dir.path(), "nothing-*.log").is_none());
    assert!(resolve_newest(&dir.path().join("missing"), "*.log").is_none());
}

#[test]
fn test_pattern_stream_switches_to_newer_file_keeping_filters() {
    let dir = tempfile::tempdir().unwrap();
    write_file_with_mtime(
        &dir.path().join("app-2026-09-18.log"),
        "INFO start\nERROR one\nINFO middle\nERROR two\n",
        100,
    );
    let pattern = dir.path().join("app-*.log");
    let mut engine = TailEngine::open_pattern(&pattern).unwrap();
    engine.pattern_scan_interval = Duration::from_secs(0);
    assert!(engine.is_pattern());
    assert_eq!(engine.path, pattern);
    assert_eq!(
        engine.current_file_name().as_deref(),
        Some("app-2026-09-18.log")
    );
    assert_eq!(engine.total_lines(), 4);
    assert!(engine.switch_notice.is_none(), "opening is not a switch");

    engine.set_include_filter("ERROR");
    engine.update_search("two");
    engine.wrap_lines = true;
    engine.toggle_bookmark(1);
    engine.select_row(3);
    assert_eq!(engine.visible_line_count(), 2);
    assert!(engine.has_bookmarks());
    assert!(engine.has_selection());

    // A newer file appears: the stream follows it within one poll.
    write_file_with_mtime(
        &dir.path().join("app-2026-09-19.log"),
        "ERROR fresh\nINFO quiet\nERROR two again\n",
        0,
    );
    engine.poll_updates();
    assert_eq!(
        engine.current_file_name().as_deref(),
        Some("app-2026-09-19.log")
    );
    assert_eq!(
        engine.path, pattern,
        "the stream identity stays the pattern"
    );
    assert_eq!(engine.total_lines(), 3);
    assert_eq!(
        engine.include_filter(),
        "ERROR",
        "filters survive the switch"
    );
    assert_eq!(
        engine.visible_line_count(),
        2,
        "the include filter is re-applied"
    );
    assert_eq!(
        engine.last_searched_query, "two",
        "the search query survives"
    );
    assert_eq!(
        engine.search_matches,
        vec![2],
        "search is re-run on the new file"
    );
    assert!(engine.wrap_lines, "wrap survives");
    assert!(!engine.has_bookmarks(), "bookmarks reset");
    assert!(!engine.has_selection(), "selection reset");
    assert_eq!(engine.unseen_lines, 0);
    assert_eq!(engine.active_switch_notice(), Some("app-2026-09-19.log"));

    // The same newest file does not switch again, and appends are still followed.
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 3);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join("app-2026-09-19.log"))
        .unwrap();
    writeln!(f, "ERROR appended").unwrap();
    f.flush().unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 4);
    assert_eq!(engine.visible_line_count(), 3);
}

#[test]
fn test_pattern_stream_waits_for_first_match() {
    let dir = tempfile::tempdir().unwrap();
    let pattern = dir.path().join("svc-*.log");
    let mut engine = TailEngine::open_pattern(&pattern).unwrap();
    engine.pattern_scan_interval = Duration::from_secs(0);
    assert!(engine.is_pattern());
    assert!(engine.current_file.is_none());
    assert_eq!(engine.total_lines(), 0);
    engine.set_include_filter("WARN");
    engine.poll_updates();
    assert!(engine.current_file.is_none(), "still nothing to tail");

    write_file_with_mtime(&dir.path().join("svc-1.log"), "WARN a\nINFO b\n", 0);
    engine.poll_updates();
    assert_eq!(engine.current_file_name().as_deref(), Some("svc-1.log"));
    assert_eq!(engine.total_lines(), 2);
    assert_eq!(engine.visible_line_count(), 1);
    assert_eq!(engine.active_switch_notice(), Some("svc-1.log"));

    // Pattern with no directory part is rejected only when the directory is missing.
    assert!(TailEngine::open_pattern(dir.path().join("missing").join("*.log")).is_err());
    assert!(TailEngine::open_pattern(dir.path().join("plain.log")).is_err());
}

#[test]
fn test_pattern_entries_persist_in_ini() {
    use std::path::PathBuf;
    let dir = tempfile::tempdir().unwrap();
    let pattern = dir.path().join("app-*.log");
    let plain_missing = dir.path().join("gone.log");
    let mut config = FastTailConfig::default();
    config.open_files = vec![pattern.clone(), plain_missing.clone()];
    config.recent_files = vec![pattern.clone(), PathBuf::from("recent.log")];

    let ini = config.to_ini();
    let loaded = FastTailConfig::from_ini(&ini);
    assert_eq!(
        loaded.open_files,
        vec![pattern.clone()],
        "the pattern is kept, the missing plain file is dropped"
    );
    assert_eq!(
        loaded.recent_files,
        vec![pattern, PathBuf::from("recent.log")]
    );
}

// ===== External tools =====

#[cfg(windows)]
fn quick_exit_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "cmd", "/c exit")
}
#[cfg(not(windows))]
fn quick_exit_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "true", "")
}

#[cfg(windows)]
fn slow_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "ping", "-n 3 127.0.0.1")
}
#[cfg(not(windows))]
fn slow_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "sleep", "2")
}

#[test]
fn test_external_tools_ini_round_trip() {
    use fasttail::external_tools::ExternalTool;

    let mut config = FastTailConfig::default();
    let mut editor = ExternalTool::new("Editor", "code", "-g \"{file}:{lineno}\"");
    editor.shortcut = Some("Ctrl+Shift+E".to_string());
    let mut notify = ExternalTool::new("Notify", "notify-send", "\"{line}\"");
    notify.bound_rule = Some("FATAL".to_string());
    notify.use_shell = true;
    notify.match_pattern = Some("req=([0-9]+)".to_string());
    config.external_tools = vec![editor.clone(), notify.clone()];

    let ini = config.to_ini();
    let tool0 = ini.section(Some("tool.0")).expect("tool.0 section");
    assert_eq!(tool0.get("name"), Some("Editor"));
    assert_eq!(tool0.get("program"), Some("code"));
    assert_eq!(tool0.get("shortcut"), Some("Ctrl+Shift+E"));
    assert_eq!(tool0.get("shell"), Some("false"));
    let tool1 = ini.section(Some("tool.1")).expect("tool.1 section");
    assert_eq!(tool1.get("rule"), Some("FATAL"));
    assert_eq!(tool1.get("match"), Some("req=([0-9]+)"));

    let restored = FastTailConfig::from_ini(&ini);
    assert_eq!(restored.external_tools, vec![editor, notify]);

    // A section without name or program is skipped, the rest is kept.
    let mut broken = ini.clone();
    broken.with_section(Some("tool.0")).set("program", "");
    let restored = FastTailConfig::from_ini(&broken);
    assert_eq!(restored.external_tools.len(), 1);
    assert_eq!(restored.external_tools[0].name, "Notify");
}

#[test]
fn test_external_tool_expansion_one_argv_entry_per_argument() {
    use fasttail::external_tools::{build_command, expanded_args, ExternalTool, ToolContext};
    use std::path::Path;

    let tool = ExternalTool::new("Editor", "code", "-g \"{file}:{lineno}\" --line {line}");
    let ctx = ToolContext::for_row(Path::new("/var/log/app.log"), 120, "hello world", None);
    let args = expanded_args(&tool, &ctx);
    assert_eq!(
        args,
        vec![
            "-g".to_string(),
            format!("{}:120", Path::new("/var/log/app.log").display()),
            "--line".to_string(),
            "hello world".to_string(),
        ]
    );

    // The command carries the program and exactly those argv entries, no shell.
    let cmd = build_command(&tool, &ctx);
    assert_eq!(cmd.get_program(), "code");
    let argv: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, args);

    // {dir} and {selection}: the selection defaults to the row itself.
    let tool = ExternalTool::new("T", "x", "{dir} {selection}");
    let args = expanded_args(&tool, &ctx);
    assert_eq!(args[0], Path::new("/var/log").display().to_string());
    assert_eq!(args[1], "hello world");
    let ctx_sel = ToolContext::for_row(Path::new("a.log"), 1, "row", Some("row\nnext"));
    assert_eq!(expanded_args(&tool, &ctx_sel)[1], "row\nnext");
}

#[test]
fn test_external_tool_hostile_line_is_a_single_literal_argument() {
    use fasttail::external_tools::{build_command, ExternalTool, ToolContext};
    use std::path::Path;

    let hostile = "x; rm -rf / && del *.* | shutdown";
    let tool = ExternalTool::new("Echo", "echo", "{line}");
    let ctx = ToolContext::for_row(Path::new("app.log"), 7, hostile, None);
    let cmd = build_command(&tool, &ctx);
    assert_eq!(
        cmd.get_program(),
        "echo",
        "no shell in front of the program"
    );
    let argv: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        argv,
        vec![hostile.to_string()],
        "the whole line is one argument"
    );

    // Shell mode is the explicit opt-in: the line is handed to cmd /c or sh -c.
    let mut shell_tool = tool.clone();
    shell_tool.use_shell = true;
    let cmd = build_command(&shell_tool, &ctx);
    let program = cmd.get_program().to_string_lossy().into_owned();
    assert!(program == "cmd" || program == "sh", "{program}");
}

#[test]
fn test_external_tool_match_placeholder() {
    use fasttail::external_tools::{expanded_args, match_value, ExternalTool, ToolContext};
    use std::path::Path;

    let mut tool = ExternalTool::new("Ticket", "open", "https://tracker/{match}");
    tool.match_pattern = Some(r"ticket=([A-Z]+-\d+)".to_string());
    let line = "2026-09-18 ERROR ticket=FT-42 payment failed";
    assert_eq!(match_value(&tool, line), "FT-42");
    let ctx = ToolContext::for_row(Path::new("app.log"), 1, line, None);
    assert_eq!(expanded_args(&tool, &ctx), vec!["https://tracker/FT-42"]);

    // No group: the whole match; no match or no pattern: empty.
    tool.match_pattern = Some(r"FT-\d+".to_string());
    assert_eq!(match_value(&tool, line), "FT-42");
    assert_eq!(match_value(&tool, "nothing here"), "");
    tool.match_pattern = None;
    assert_eq!(match_value(&tool, line), "");
    tool.match_pattern = Some("(".to_string());
    assert_eq!(
        match_value(&tool, line),
        "",
        "an invalid regex expands to nothing"
    );
}

#[test]
fn test_external_tool_runner_throttle_and_cap() {
    use fasttail::external_tools::{RunOutcome, ToolContext, ToolRunner};
    use std::path::Path;

    let ctx = ToolContext::for_row(Path::new("app.log"), 1, "FATAL boom", None);

    // Throttle: three matches within a second run the tool once, two are dropped.
    let mut runner = ToolRunner::with_limits(Duration::from_secs(1), 10);
    let tool = quick_exit_tool("Notify");
    assert_eq!(runner.run_bound(&tool, &ctx), RunOutcome::Spawned);
    assert_eq!(runner.run_bound(&tool, &ctx), RunOutcome::Throttled);
    assert_eq!(runner.run_bound(&tool, &ctx), RunOutcome::Throttled);
    assert_eq!(runner.dropped_for("Notify"), 2);
    assert_eq!(runner.dropped_for("Other"), 0);
    // Another tool has its own throttle window.
    let other = quick_exit_tool("Other");
    assert_eq!(runner.run_bound(&other, &ctx), RunOutcome::Spawned);

    // Cap: with no throttle and a cap of 2, the third long-running child is refused.
    let mut runner = ToolRunner::with_limits(Duration::ZERO, 2);
    let slow = slow_tool("Slow");
    assert_eq!(runner.run_bound(&slow, &ctx), RunOutcome::Spawned);
    assert_eq!(runner.run_bound(&slow, &ctx), RunOutcome::Spawned);
    assert_eq!(runner.running(), 2);
    assert_eq!(runner.run_bound(&slow, &ctx), RunOutcome::CapReached);
    assert_eq!(runner.dropped_for("Slow"), 1);

    // A missing program is reported, not panicked on (fresh runner: the cap above is full).
    let mut runner = ToolRunner::default();
    let missing =
        fasttail::external_tools::ExternalTool::new("Missing", "no-such-program-fasttail", "");
    assert!(matches!(
        runner.run_bound(&missing, &ctx),
        RunOutcome::Failed(_)
    ));
    assert!(runner.run_manual(&missing, &ctx).is_err());
    assert!(runner.last_error.is_some());
    assert!(runner.run_manual(&quick_exit_tool("Ok"), &ctx).is_ok());
    assert!(runner.last_error.is_none());
}

#[test]
fn test_engine_queues_hits_of_tool_bound_rules_on_append() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "INFO start").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_highlight_rules(vec![
        HighlightRule::new("FATAL", [255, 0, 0], [0, 0, 0], false),
        HighlightRule::new("WARN", [255, 255, 0], [0, 0, 0], false),
    ]);
    // Only FATAL has a tool bound to it.
    engine.tool_bound_rules = ["FATAL".to_string()].into_iter().collect();

    writeln!(tmp, "FATAL first").unwrap();
    writeln!(tmp, "WARN ignored").unwrap();
    writeln!(tmp, "fatal lower case matches too").unwrap();
    tmp.flush().unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 4);
    assert_eq!(
        engine.pending_tool_hits,
        vec![("FATAL".to_string(), 1), ("FATAL".to_string(), 3)]
    );

    // The app drains the queue; the row context carries the file and 1-based line.
    let hits = std::mem::take(&mut engine.pending_tool_hits);
    let ctx = fasttail::ui::dock::tool_context_for_row(&engine, hits[0].1).unwrap();
    assert_eq!(ctx.lineno, 2);
    assert_eq!(ctx.line, "FATAL first");
    assert_eq!(ctx.file, tmp.path().display().to_string());
    assert_eq!(ctx.selection, "FATAL first");

    // Without bound rules nothing is queued.
    engine.tool_bound_rules.clear();
    writeln!(tmp, "FATAL again").unwrap();
    tmp.flush().unwrap();
    engine.poll_updates();
    assert!(engine.pending_tool_hits.is_empty());
}

#[test]
fn test_engine_current_row_prefers_selection_then_search_hit_then_last_line() {
    let mut tmp = NamedTempFile::new().unwrap();
    for i in 0..5 {
        writeln!(tmp, "line {i} {}", if i == 2 { "needle" } else { "" }).unwrap();
    }
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.current_row(), Some(4), "last line by default");

    engine.update_search("needle");
    assert_eq!(engine.current_row(), Some(2), "the current search hit");

    engine.select_row(1);
    assert_eq!(engine.current_row(), Some(1), "the clicked row wins");
    engine.clear_selection();
    assert_eq!(engine.current_row(), Some(2));
}

// ---------------------------------------------------------------------------
// Named sessions
// ---------------------------------------------------------------------------

mod named_sessions {
    use fasttail::cli::CliArgs;
    use fasttail::config::FastTailConfig;
    use fasttail::i18n::{t, Language};
    use fasttail::session::{Session, StreamEntry, SESSION_SUFFIX};
    use std::path::{Path, PathBuf};

    fn entry(path: PathBuf) -> StreamEntry {
        StreamEntry {
            path,
            include_filter: "ERROR".to_string(),
            exclude_filter: "health".to_string(),
            include_extra: vec!["payment".to_string(), "timeout".to_string()],
            exclude_extra: vec!["retry=0".to_string()],
            search_query: "timeout".to_string(),
            wrap: true,
            encoding: Some("ANSI".to_string()),
            ansi: Some("strip".to_string()),
            bookmarks: vec![3, 7, 42],
            archive_entry: None,
        }
    }

    #[test]
    fn every_session_field_round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        std::fs::write(&log, "a\nb\n").unwrap();
        let pattern = dir.path().join("app-*.log");
        let mut session = Session {
            streams: vec![entry(log.clone()), StreamEntry::new(pattern.clone())],
            dock_layout: Some("(layout)".to_string()),
        };
        session.streams[1].wrap = false;
        let file = dir.path().join(format!("incident{SESSION_SUFFIX}"));
        session.save_to(&file).unwrap();

        let loaded = Session::load_from(&file).unwrap();
        assert!(loaded.missing.is_empty());
        assert!(!loaded.relocated);
        assert_eq!(loaded.session, session);
        assert_eq!(Session::name_of(&file), "incident");
    }

    #[test]
    fn filter_terms_and_search_keep_quotes_and_edge_spaces_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        std::fs::write(
            &log, "a
",
        )
        .unwrap();
        let tricky: Vec<String> = TRICKY.iter().map(|t| t.to_string()).collect();
        let mut stream = entry(log);
        stream.include_filter = tricky[0].clone();
        stream.exclude_filter = tricky[1].clone();
        stream.search_query = tricky[2].clone();
        stream.include_extra = tricky[3..].to_vec();
        stream.exclude_extra = tricky[3..].to_vec();
        let session = Session {
            streams: vec![stream],
            dock_layout: None,
        };
        let file = dir.path().join(format!("quotes{SESSION_SUFFIX}"));
        session.save_to(&file).unwrap();
        assert_eq!(Session::load_from(&file).unwrap().session, session);
    }

    const TRICKY: [&str; 7] = [
        "\"status\":500",
        " ERROR ",
        "'user'",
        "it's \"x\" 'y'",
        "''\"",
        "\tpad\\d+\t",
        "a\\\\b",
    ];

    /// A zip holding `server.log` and `logs/worker.log`.
    fn write_bundle(path: &Path) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for name in ["server.log", "logs/worker.log"] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"started\n").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn compressed_streams_are_saved_as_the_archive_and_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle.zip");
        write_bundle(&bundle);
        let gz = dir.path().join("app.log.1.gz");
        std::fs::write(&gz, [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        let worker = fasttail::compressed::entry_path(&bundle, "logs/worker.log");
        let mut zipped = entry(worker.clone());
        zipped.archive_entry = Some("logs/worker.log".to_string());
        let session = Session {
            streams: vec![zipped, entry(gz.clone())],
            dock_layout: None,
        };
        let file = dir.path().join(format!("bundle{SESSION_SUFFIX}"));
        session.save_to(&file).unwrap();

        // The file names the archive, never the entry path, plus the entry.
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("entry=logs/worker.log"), "{text}");
        assert!(text.contains("rel=bundle.zip"), "{text}");
        let loaded = Session::load_from(&file).unwrap();
        assert!(loaded.missing.is_empty());
        assert_eq!(loaded.session, session);

        // The bundle and its session move together: the entry follows the archive.
        let moved = dir.path().join("moved");
        std::fs::create_dir_all(&moved).unwrap();
        std::fs::rename(&bundle, moved.join("bundle.zip")).unwrap();
        std::fs::rename(&gz, moved.join("app.log.1.gz")).unwrap();
        let moved_file = moved.join(format!("bundle{SESSION_SUFFIX}"));
        std::fs::rename(&file, &moved_file).unwrap();
        let loaded = Session::load_from(&moved_file).unwrap();
        assert!(loaded.missing.is_empty());
        assert_eq!(
            loaded.session.streams[0].path,
            fasttail::compressed::entry_path(&moved.join("bundle.zip"), "logs/worker.log")
        );
        assert_eq!(
            loaded.session.streams[0].archive_entry.as_deref(),
            Some("logs/worker.log")
        );
    }

    #[test]
    fn a_backslash_zip_entry_survives_a_session_round_trip() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("win.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&bundle).unwrap());
        zip.start_file("dir\\file.log", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"started\n").unwrap();
        zip.finish().unwrap();
        let settings = fasttail::compressed::Settings {
            spool_dir: dir.path().join("spool"),
            limits: fasttail::compressed::Limits::default(),
        };
        let engine =
            fasttail::compressed::open_engine(&bundle, Some("dir/file.log"), &settings, None)
                .unwrap();
        // What the app saves for the stream: its path and its entry identity.
        let mut zipped = entry(engine.path.clone());
        zipped.archive_entry = engine.compressed.as_ref().unwrap().entry.clone();
        let session = Session {
            streams: vec![zipped],
            dock_layout: None,
        };
        let file = dir.path().join(format!("win{SESSION_SUFFIX}"));
        session.save_to(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("entry=dir/file.log"), "{text}");
        let loaded = Session::load_from(&file).unwrap();
        assert!(loaded.missing.is_empty(), "{:?}", loaded.missing);
        assert_eq!(loaded.session, session);

        // A session written by 0.10.0 saved the archive spelling: it still resolves.
        let mut old = session.clone();
        old.streams[0].archive_entry = Some("dir\\file.log".to_string());
        old.save_to(&file).unwrap();
        let loaded = Session::load_from(&file).unwrap();
        assert!(loaded.missing.is_empty(), "{:?}", loaded.missing);
        assert_eq!(loaded.session.streams[0].path, engine.path);
    }

    #[test]
    fn the_default_session_keeps_zip_entries_open() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle.zip");
        write_bundle(&bundle);
        let server = fasttail::compressed::entry_path(&bundle, "server.log");
        let mut cfg = FastTailConfig::default();
        cfg.open_files = vec![server.clone()];
        let loaded = FastTailConfig::from_ini(&cfg.to_ini());
        assert_eq!(loaded.open_files, vec![server.clone()]);
        assert_eq!(
            Session::from_config(&loaded).streams[0]
                .archive_entry
                .as_deref(),
            Some("server.log")
        );
        // Once the archive is gone the entry is dropped like any missing file.
        std::fs::remove_file(&bundle).unwrap();
        assert!(FastTailConfig::from_ini(&cfg.to_ini())
            .open_files
            .is_empty());
    }

    #[test]
    fn files_that_no_longer_exist_are_listed_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept.log");
        let gone = dir.path().join("gone.log");
        std::fs::write(&kept, "a\n").unwrap();
        std::fs::write(&gone, "b\n").unwrap();
        // A pattern whose directory disappears counts as missing too.
        let gone_dir = dir.path().join("rotated");
        std::fs::create_dir_all(&gone_dir).unwrap();
        let pattern = gone_dir.join("app-*.log");
        let file = dir.path().join(format!("partial{SESSION_SUFFIX}"));
        Session {
            streams: vec![
                entry(kept.clone()),
                entry(gone.clone()),
                StreamEntry::new(pattern.clone()),
            ],
            dock_layout: Some("(layout)".to_string()),
        }
        .save_to(&file)
        .unwrap();

        std::fs::remove_file(&gone).unwrap();
        std::fs::remove_dir_all(&gone_dir).unwrap();

        let loaded = Session::load_from(&file).unwrap();
        let opened: Vec<&Path> = loaded
            .session
            .streams
            .iter()
            .map(|s| s.path.as_path())
            .collect();
        assert_eq!(
            opened,
            vec![kept.as_path()],
            "only the file still on disk is opened"
        );
        assert_eq!(loaded.missing.len(), 2, "{:?}", loaded.missing);
        let named = |p: &Path| {
            loaded
                .missing
                .iter()
                .any(|m| m.file_name() == p.file_name())
        };
        assert!(named(&gone), "the deleted file is reported");
        assert!(
            named(&pattern),
            "the pattern of a deleted directory is reported"
        );
        // The surviving stream keeps what was saved for it.
        assert_eq!(loaded.session.streams[0], entry(kept));
    }

    #[test]
    fn a_moved_bundle_opens_through_the_relative_path() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("bundle");
        std::fs::create_dir_all(bundle.join("logs")).unwrap();
        let log = bundle.join("logs").join("app.log");
        std::fs::write(&log, "x\n").unwrap();
        let file = bundle.join(format!("dev{SESSION_SUFFIX}"));
        Session {
            streams: vec![entry(log.clone())],
            dock_layout: Some("(layout)".to_string()),
        }
        .save_to(&file)
        .unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("rel=logs/app.log"), "{text}");

        // Move the whole bundle: the absolute path no longer exists.
        let moved = root.path().join("moved");
        std::fs::rename(&bundle, &moved).unwrap();
        let loaded = Session::load_from(&moved.join(format!("dev{SESSION_SUFFIX}"))).unwrap();
        assert!(loaded.missing.is_empty());
        assert_eq!(loaded.session.streams.len(), 1);
        assert_eq!(
            loaded.session.streams[0].path,
            moved.join("logs").join("app.log")
        );
        assert_eq!(loaded.session.streams[0].include_filter, "ERROR");
        assert!(loaded.relocated);
        assert_eq!(
            loaded.session.dock_layout, None,
            "a layout naming the old paths is dropped"
        );
    }

    #[test]
    fn missing_files_are_skipped_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("present.log");
        std::fs::write(&present, "x\n").unwrap();
        let gone = dir.path().join("gone.log");
        let no_dir_pattern = dir.path().join("nowhere").join("*.log");
        let file = dir.path().join(format!("s{SESSION_SUFFIX}"));
        Session {
            streams: vec![
                StreamEntry::new(present.clone()),
                StreamEntry::new(gone.clone()),
                StreamEntry::new(no_dir_pattern.clone()),
            ],
            dock_layout: None,
        }
        .save_to(&file)
        .unwrap();
        let loaded = Session::load_from(&file).unwrap();
        assert_eq!(loaded.session.streams.len(), 1);
        assert_eq!(loaded.session.streams[0].path, present);
        assert_eq!(loaded.missing, vec![gone, no_dir_pattern]);
    }

    #[test]
    fn serialized_text_detects_changes() {
        let a = Session {
            streams: vec![entry(PathBuf::from("C:/x/a.log"))],
            dock_layout: None,
        };
        let mut b = a.clone();
        assert_eq!(a.serialized(None), b.serialized(None));
        b.streams[0].bookmarks.push(99);
        assert_ne!(a.serialized(None), b.serialized(None));
        let mut c = a.clone();
        c.dock_layout = Some("(other)".to_string());
        assert_ne!(a.serialized(None), c.serialized(None));
    }

    #[test]
    fn default_session_is_embedded_in_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a.log");
        std::fs::write(&log, "x\n").unwrap();
        let mut cfg = FastTailConfig::default();
        cfg.open_files = vec![log.clone()];
        let mut e = entry(log.clone());
        e.wrap = false;
        e.bookmarks.clear();
        cfg.set_stream_state(e);
        cfg.set_wrap(&log, true);
        cfg.set_bookmarks(&log, &[3, 7, 42]);
        cfg.current_session = Some(dir.path().join(format!("cur{SESSION_SUFFIX}")));
        cfg.add_recent_session(&dir.path().join(format!("one{SESSION_SUFFIX}")));
        cfg.add_recent_session(&dir.path().join(format!("two{SESSION_SUFFIX}")));

        let ini = cfg.to_ini();
        let loaded = FastTailConfig::from_ini(&ini);
        assert_eq!(loaded.open_files, vec![log.clone()]);
        let state = loaded
            .stream_state_for(&log)
            .expect("stream state persisted");
        assert_eq!(state.include_filter, "ERROR");
        assert_eq!(state.exclude_filter, "health");
        assert_eq!(state.search_query, "timeout");
        assert_eq!(state.encoding.as_deref(), Some("ANSI"));
        assert!(loaded.wrap_for(&log));
        assert_eq!(loaded.bookmarks_for(&log, 100), Some(vec![3, 7, 42]));
        assert_eq!(loaded.current_session, cfg.current_session);
        assert_eq!(loaded.recent_sessions.len(), 2);
        assert_eq!(
            Session::name_of(&loaded.recent_sessions[0]),
            "two",
            "most recent first"
        );

        // The default session assembled from the config carries everything.
        let default = Session::from_config(&loaded);
        assert_eq!(default.streams.len(), 1);
        assert!(default.streams[0].wrap);
        assert_eq!(default.streams[0].bookmarks, vec![3, 7, 42]);
    }

    #[test]
    fn config_without_session_sections_still_loads() {
        let text =
            "[general]\ntheme=Tron\nlanguage=en\n\n[open_files]\nfile_0=missing-for-sure.log\n";
        let ini = ini::Ini::load_from_str(text).unwrap();
        let cfg = FastTailConfig::from_ini(&ini);
        assert!(cfg.streams.is_empty());
        assert!(cfg.current_session.is_none());
        assert!(cfg.recent_sessions.is_empty());
    }

    #[test]
    fn apply_to_config_replaces_the_default_workspace() {
        let mut cfg = FastTailConfig::default();
        cfg.open_files = vec![PathBuf::from("C:/old/x.log")];
        cfg.dock_layout = Some("(old)".to_string());
        let session = Session {
            streams: vec![entry(PathBuf::from("C:/new/a.log"))],
            dock_layout: Some("(new)".to_string()),
        };
        session.apply_to_config(&mut cfg);
        assert_eq!(cfg.open_files, vec![PathBuf::from("C:/new/a.log")]);
        assert_eq!(cfg.dock_layout.as_deref(), Some("(new)"));
        assert!(cfg.wrap_for(Path::new("C:/new/a.log")));
        assert_eq!(
            cfg.stream_state_for(Path::new("C:/new/a.log"))
                .map(|s| s.include_filter.clone()),
            Some("ERROR".to_string())
        );
    }

    #[test]
    fn session_suffix_and_cli_option() {
        assert_eq!(
            Session::with_suffix(Path::new("C:/s/incident")),
            PathBuf::from(format!("C:/s/incident{SESSION_SUFFIX}"))
        );
        let cwd = if cfg!(windows) {
            PathBuf::from(r"C:\work")
        } else {
            PathBuf::from("/work")
        };
        let args = CliArgs::parse(["--session", "dev.fasttail-session.ini"], &cwd).unwrap();
        assert_eq!(args.session, Some(cwd.join("dev.fasttail-session.ini")));
        assert!(fasttail::cli::USAGE.contains("--session <FILE>"));
    }

    #[test]
    fn session_i18n_keys_exist_in_every_language() {
        for lang in [
            Language::En,
            Language::It,
            Language::Fr,
            Language::Es,
            Language::Zh,
        ] {
            for key in [
                "session_tip",
                "session_save_as",
                "session_load",
                "session_unsaved_body",
                "session_missing_title",
            ] {
                let text = t(lang, key);
                assert_ne!(text, key, "{key} missing for {lang:?}");
            }
            assert!(t(lang, "session_unsaved_body").contains("{name}"));
        }
    }
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

mod timestamp_range {
    use fasttail::tail_engine::TailEngine;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// A log whose entries are one minute apart, with a stack trace hanging off the second
    /// one: the continuation lines carry no time of their own.
    fn sample_log() -> NamedTempFile {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "2026-09-18T14:01:00.000Z INFO starting").unwrap();
        writeln!(tmp, "2026-09-18T14:02:05.123Z ERROR boom").unwrap();
        writeln!(tmp, "    at Foo.bar(Foo.java:10)").unwrap();
        writeln!(tmp, "    at Foo.baz(Foo.java:20)").unwrap();
        writeln!(tmp, "2026-09-18T14:03:00.000Z INFO recovered").unwrap();
        writeln!(tmp, "2026-09-18T14:06:00.000Z INFO done").unwrap();
        tmp.flush().unwrap();
        tmp
    }

    fn millis(iso: &str) -> i64 {
        fasttail::timestamp::detect_timestamp(iso, Default::default())
            .expect("test timestamp")
            .0
    }

    fn visible(engine: &TailEngine) -> Vec<usize> {
        (0..engine.total_lines())
            .filter(|i| engine.is_line_visible(*i))
            .collect()
    }

    #[test]
    fn continuation_lines_inherit_the_entry_time() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();

        let boom = millis("2026-09-18T14:02:05.123Z");
        assert_eq!(engine.line_timestamp(1), Some(boom));
        assert_eq!(engine.line_timestamp(2), Some(boom), "stack frame");
        assert_eq!(engine.line_timestamp(3), Some(boom), "stack frame");
        assert_eq!(
            engine.line_timestamp(4),
            Some(millis("2026-09-18T14:03:00.000Z"))
        );
    }

    #[test]
    fn a_window_keeps_the_entry_together_with_its_stack_trace() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.set_time_range(
            Some(millis("2026-09-18T14:02:00.000Z")),
            Some(millis("2026-09-18T14:05:00.000Z")),
        );
        // The error, both of its stack frames, and the recovery - not the 14:01 or 14:06.
        assert_eq!(visible(&engine), vec![1, 2, 3, 4]);
    }

    #[test]
    fn open_ended_windows() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();

        engine.set_time_range(Some(millis("2026-09-18T14:03:00.000Z")), None);
        assert_eq!(visible(&engine), vec![4, 5], "from only");

        engine.set_time_range(None, Some(millis("2026-09-18T14:02:05.123Z")));
        assert_eq!(visible(&engine), vec![0, 1, 2, 3], "to only");

        engine.set_time_range(None, None);
        assert_eq!(visible(&engine), vec![0, 1, 2, 3, 4, 5], "no window");
    }

    #[test]
    fn lines_before_the_first_timestamp_are_hidden_by_a_window() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "==== FastTail log banner ====").unwrap();
        writeln!(tmp, "2026-09-18T14:02:00.000Z INFO first timed line").unwrap();
        tmp.flush().unwrap();

        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();
        assert_eq!(engine.line_timestamp(0), None, "nothing to inherit yet");

        engine.set_time_range(Some(millis("2026-09-18T14:00:00.000Z")), None);
        assert_eq!(
            visible(&engine),
            vec![1],
            "a line that cannot be placed in time is out of the window"
        );
    }

    #[test]
    fn go_to_time_bisects_an_ordered_log() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();

        assert_eq!(
            engine.goto_time(millis("2026-09-18T14:02:05.123Z")),
            Some(1)
        );
        assert_eq!(
            engine.goto_time(millis("2026-09-18T14:02:30.000Z")),
            Some(4),
            "between two entries lands on the next one"
        );
        assert_eq!(
            engine.goto_time(millis("2026-09-18T00:00:00.000Z")),
            Some(0)
        );
        assert_eq!(
            engine.goto_time(millis("2026-09-18T23:00:00.000Z")),
            None,
            "past the end of the log"
        );
    }

    #[test]
    fn go_to_time_scans_when_the_log_jumps_back() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "2026-09-18T14:05:00.000Z INFO late first").unwrap();
        writeln!(tmp, "2026-09-18T14:01:00.000Z INFO earlier").unwrap();
        writeln!(tmp, "2026-09-18T14:09:00.000Z INFO last").unwrap();
        tmp.flush().unwrap();

        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();
        // A bisection over these would miss line 1; the linear fallback does not.
        assert_eq!(
            engine.goto_time(millis("2026-09-18T14:00:00.000Z")),
            Some(0)
        );
        assert_eq!(
            engine.goto_time(millis("2026-09-18T14:06:00.000Z")),
            Some(2)
        );
        assert_eq!(
            engine.goto_time(millis("2026-09-18T14:02:00.000Z")),
            Some(0)
        );
    }

    #[test]
    fn a_log_we_cannot_time_disables_the_controls() {
        let mut untimed = NamedTempFile::new().unwrap();
        for i in 0..300 {
            writeln!(untimed, "plain line {i} with no timestamp at all").unwrap();
        }
        untimed.flush().unwrap();
        let mut engine = TailEngine::open(untimed.path()).unwrap();
        engine.ensure_timestamps();
        assert_eq!(engine.timestamp_rate(), Some(0.0));
        assert!(!engine.timestamps_usable());

        let mut timed = NamedTempFile::new().unwrap();
        for i in 0..300 {
            writeln!(timed, "2026-09-18T14:0{}:00.000Z line {i}", i % 10).unwrap();
        }
        timed.flush().unwrap();
        let mut engine = TailEngine::open(timed.path()).unwrap();
        engine.ensure_timestamps();
        assert_eq!(engine.timestamp_rate(), Some(1.0));
        assert!(engine.timestamps_usable());
    }

    #[test]
    fn a_log_mostly_made_of_stack_traces_can_be_timed() {
        // One timestamped entry followed by a three-line stack trace: a quarter of the lines
        // carry a timestamp of their own, every line inherits one, so a window places them all.
        let mut tmp = NamedTempFile::new().unwrap();
        for i in 0..100 {
            writeln!(tmp, "2026-09-18 14:{:02}:00 ERROR failure {i}", i % 60).unwrap();
            writeln!(tmp, "java.lang.IllegalStateException: boom").unwrap();
            writeln!(tmp, "\tat com.example.Service.run(Service.java:42)").unwrap();
            writeln!(tmp, "\tat java.base/java.lang.Thread.run(Thread.java:1583)").unwrap();
        }
        tmp.flush().unwrap();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();
        assert!(engine.timestamps_usable());

        // Most lines before the first timestamp: a window would hide them, the hint stays.
        let mut late = NamedTempFile::new().unwrap();
        for i in 0..300 {
            writeln!(late, "banner line {i}").unwrap();
        }
        writeln!(late, "2026-09-18 14:00:00 INFO started").unwrap();
        late.flush().unwrap();
        let mut engine = TailEngine::open(late.path()).unwrap();
        engine.ensure_timestamps();
        assert!(!engine.timestamps_usable());
    }

    #[test]
    fn a_date_alone_covers_the_whole_day() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "2026-09-17T23:59:59.000Z before").unwrap();
        writeln!(tmp, "2026-09-18T00:00:00.000Z first").unwrap();
        writeln!(tmp, "2026-09-18T12:00:00.000Z middle").unwrap();
        writeln!(tmp, "2026-09-18T23:59:59.500Z last").unwrap();
        writeln!(tmp, "2026-09-19T00:00:00.000Z after").unwrap();
        tmp.flush().unwrap();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        assert_eq!(
            engine.apply_time_range_text("2026-09-18", "2026-09-18"),
            (true, true)
        );
        // From 00:00:00.000 through 23:59:59.999 of that day.
        assert_eq!(visible(&engine), vec![1, 2, 3]);
    }

    #[test]
    fn the_visible_span_follows_the_window() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();
        assert_eq!(
            engine.visible_time_span(),
            Some((
                millis("2026-09-18T14:01:00.000Z"),
                millis("2026-09-18T14:06:00.000Z")
            ))
        );

        engine.set_time_range(
            Some(millis("2026-09-18T14:02:00.000Z")),
            Some(millis("2026-09-18T14:05:00.000Z")),
        );
        assert_eq!(
            engine.visible_time_span(),
            Some((
                millis("2026-09-18T14:02:05.123Z"),
                millis("2026-09-18T14:03:00.000Z")
            ))
        );
    }

    #[test]
    fn the_go_to_box_takes_a_time_as_well_as_a_line() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.ensure_timestamps();

        // A time, in the shapes a person types.
        let by_clock = engine.resolve_goto("14:03", 0).expect("HH:MM");
        assert_eq!(by_clock.line, 4);
        let by_second = engine.resolve_goto("14:02:05", 0).expect("HH:MM:SS");
        assert_eq!(by_second.line, 1);
        let by_stamp = engine
            .resolve_goto("2026-09-18T14:06:00.000Z", 0)
            .expect("a timestamp copied out of the log");
        assert_eq!(by_stamp.line, 5);

        // Line numbers still work, and still mean lines.
        assert_eq!(engine.resolve_goto("3", 0).unwrap().line, 2);
        assert_eq!(engine.resolve_goto("+2", 1).unwrap().line, 3);
        // A time nothing reaches, and plain rubbish.
        assert!(engine.resolve_goto("23:59", 0).is_none());
        assert!(engine.resolve_goto("later", 0).is_none());
    }

    #[test]
    fn appended_lines_are_timed_too() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        engine.set_time_range(Some(millis("2026-09-18T14:00:00.000Z")), None);
        let before = engine.total_lines();

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(tmp.path())
            .unwrap();
        writeln!(file, "2026-09-18T14:10:00.000Z INFO appended").unwrap();
        file.flush().unwrap();
        engine.poll_updates();

        assert_eq!(engine.total_lines(), before + 1);
        assert_eq!(
            engine.line_timestamp(before),
            Some(millis("2026-09-18T14:10:00.000Z")),
            "a line that arrived after the window was set must still be placed in time"
        );
        assert!(engine.is_line_visible(before));
    }

    #[test]
    fn go_to_time_works_on_a_stream_nobody_filtered_by_time() {
        // No window was ever set, so nothing has built the timestamp cache yet.
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        let target = engine
            .resolve_goto("14:03", 0)
            .expect("a time on a fresh stream");
        assert_eq!(target.line, 4);
    }

    #[test]
    fn the_visible_span_shows_without_a_time_window() {
        let tmp = sample_log();
        let engine = TailEngine::open(tmp.path()).unwrap();
        assert_eq!(
            engine.visible_time_span(),
            Some((
                millis("2026-09-18T14:01:00.000Z"),
                millis("2026-09-18T14:06:00.000Z")
            ))
        );
    }

    #[test]
    fn a_date_and_minute_is_a_valid_bound() {
        let tmp = sample_log();
        let mut engine = TailEngine::open(tmp.path()).unwrap();
        let (from_ok, to_ok) = engine.apply_time_range_text("2026-09-18 14:02", "2026-09-18 14:03");
        assert!(from_ok && to_ok, "YYYY-MM-DD HH:MM on both sides");
        // Through 14:03:59.999: the error, its stack trace and the recovery.
        assert_eq!(visible(&engine), vec![1, 2, 3, 4]);
    }
}

// ---------------------------------------------------------------------------
// Background timestamp scan: the time range and go-to-time on large streams.
// ---------------------------------------------------------------------------

mod background_timestamps {
    use super::wait_for_jobs;
    use fasttail::scan_job::ScanKind;
    use fasttail::tail_engine::TailEngine;
    use std::io::Write;

    /// A banner line, then one entry per second from 10:00:00 with a stack trace under
    /// every seventh one and an entry stamped a minute early every 500 lines, so the log
    /// has continuation lines, untimed lines and goes back in time.
    pub(super) fn write_timed_log(path: &std::path::Path, entries: usize) {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        writeln!(f, "==== service starting, no timestamp here ====").unwrap();
        for i in 0..entries {
            let secs = if i % 500 == 499 { i - 60 } else { i };
            let level = if i % 10 == 0 { "ERROR" } else { "INFO" };
            writeln!(
                f,
                "2026-09-19T{:02}:{:02}:{:02}.000Z {level} svc-{} req={i}",
                10 + secs / 3600,
                (secs / 60) % 60,
                secs % 60,
                i % 7
            )
            .unwrap();
            if i % 7 == 0 {
                writeln!(f, "    at com.example.Handler.run(Handler.java:{i})").unwrap();
            }
        }
    }

    fn timed_log(entries: usize) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("timed.log");
        write_timed_log(&log, entries);
        (dir, log)
    }

    fn running(engine: &TailEngine) -> Option<ScanKind> {
        engine.scan_progress().map(|(kind, _, _)| kind)
    }

    fn assert_same_cache(bg: &TailEngine, sync: &TailEngine) {
        let (values, parsed, unordered, hint) = bg.timestamp_cache();
        let (want_values, want_parsed, want_unordered, want_hint) = sync.timestamp_cache();
        assert_eq!(values.len(), want_values.len(), "every line timed");
        assert!(values == want_values, "same effective timestamps");
        assert_eq!(
            parsed, want_parsed,
            "same count of lines with a time of their own"
        );
        assert_eq!(unordered, want_unordered, "same out-of-order flag");
        assert!(unordered, "the sample log goes back in time");
        assert_eq!(hint, want_hint, "same final format hint");
    }

    #[test]
    fn background_and_synchronous_timing_fill_the_same_cache() {
        let (_dir, log) = timed_log(40_000);
        let mut sync = TailEngine::open(&log).unwrap();
        sync.ensure_timestamps();

        // Threshold 0: the level scan starts on open, the timestamp request preempts it.
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert_eq!(running(&bg), Some(ScanKind::Levels));
        let target = bg.resolve_goto("10:05", 0).expect("a time");
        assert!(target.waiting, "the jump waits for the background timing");
        assert_eq!(running(&bg), Some(ScanKind::Timestamps));
        wait_for_jobs(&mut bg);
        assert!(bg.timestamps_complete());
        assert_same_cache(&bg, &sync);
        assert!(bg.levels_complete(), "the level scan resumed afterwards");
    }

    #[test]
    fn a_timing_scan_preempted_by_a_filter_resumes_from_its_prefix() {
        let (_dir, log) = timed_log(150_000);
        let mut sync = TailEngine::open(&log).unwrap();
        sync.ensure_timestamps();
        sync.set_include_filter("ERROR");

        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert!(bg.resolve_goto("10:30", 0).unwrap().waiting);
        // Let part of the file be timed, then type a filter: with no window involved the
        // filter takes over and the timing waits for it.
        let started = std::time::Instant::now();
        let mut timed = 0;
        while timed == 0 && started.elapsed().as_secs() < 30 {
            bg.poll_updates();
            timed = match bg.scan_progress() {
                Some((ScanKind::Timestamps, _, hits)) => hits,
                _ => break,
            };
        }
        bg.set_include_filter("ERROR");
        if timed > 0 && timed < bg.total_lines() {
            assert_eq!(running(&bg), Some(ScanKind::Filter), "the filter preempts");
            assert!(
                bg.line_timestamp(timed - 1).is_some(),
                "the timed prefix is kept, not thrown away"
            );
        }
        wait_for_jobs(&mut bg);
        assert_same_cache(&bg, &sync);
        assert_eq!(bg.filtered_lines, sync.filtered_lines);
        // The jump that waited lands where the synchronous one does.
        let want = sync.resolve_goto("10:30", 0).unwrap();
        assert_eq!(bg.take_goto_time_result(), Some(Some(want)));
    }

    #[test]
    fn a_window_typed_during_the_scan_is_held_then_applied() {
        let (_dir, log) = timed_log(40_000);
        let mut sync = TailEngine::open(&log).unwrap();
        sync.apply_time_range_text("10:20", "10:40");
        sync.set_include_filter("svc-3");
        assert!(!sync.time_range_pending());

        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        wait_for_jobs(&mut bg); // the level scan
        let (from_ok, to_ok) = bg.apply_time_range_text("10:20", "10:40");
        assert!(from_ok && to_ok);
        assert!(bg.time_range_pending());
        assert!(
            !bg.is_time_filtered(),
            "nothing is hidden before the cache is complete"
        );
        assert_eq!(bg.visible_line_count(), bg.total_lines());
        assert_eq!(running(&bg), Some(ScanKind::Timestamps));
        // A filter typed meanwhile waits for the timing instead of cancelling it.
        bg.set_include_filter("svc-3");
        assert_eq!(running(&bg), Some(ScanKind::Timestamps));
        wait_for_jobs(&mut bg);

        assert!(!bg.time_range_pending());
        assert!(bg.is_time_filtered());
        assert_eq!((bg.time_from, bg.time_to), (sync.time_from, sync.time_to));
        assert_eq!(bg.filtered_lines, sync.filtered_lines);
        assert!(!bg.filtered_lines.is_empty());
    }

    #[test]
    fn editing_or_clearing_a_held_window() {
        let (_dir, log) = timed_log(40_000);
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        bg.apply_time_range_text("10:20", "");
        assert!(bg.time_range_pending());
        // Editing replaces the held window; clearing drops it, the timing goes on.
        bg.apply_time_range_text("10:25", "");
        assert!(bg.time_range_pending());
        assert_eq!(bg.time_from_text, "10:25");
        bg.apply_time_range_text("", "");
        assert!(!bg.time_range_pending());
        assert_eq!(running(&bg), Some(ScanKind::Timestamps));
        wait_for_jobs(&mut bg);
        assert!(bg.timestamps_complete());
        assert!(!bg.is_time_filtered());
        assert_eq!(bg.visible_line_count(), bg.total_lines());

        // An unreadable side is flagged at once, while the window is held.
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert_eq!(bg.apply_time_range_text("10:20", "soon"), (true, false));
        bg.clear_time_range();
        assert!(!bg.time_range_pending());
    }

    #[test]
    fn go_to_time_waits_for_the_scan_and_lands_like_the_synchronous_path() {
        let (_dir, log) = timed_log(40_000);
        let mut sync = TailEngine::open(&log).unwrap();
        let want = sync.resolve_goto("10:12:30", 0).expect("synchronous jump");
        assert!(!want.waiting);

        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        bg.follow_tail = true;
        assert!(bg.resolve_goto("10:12:30", 0).unwrap().waiting);
        assert!(bg.goto_time_waiting());
        wait_for_jobs(&mut bg);
        assert!(!bg.goto_time_waiting());
        assert_eq!(bg.take_goto_time_result(), Some(Some(want)));
        assert_eq!(bg.take_goto_time_result(), None, "handed over once");
        assert_eq!(bg.scroll_to_line, Some(want.line));
        assert!(!bg.follow_tail, "a jump pauses follow");

        // A time past the end resolves to nothing once the scan is done.
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert!(bg.resolve_goto("23:59", 0).unwrap().waiting);
        wait_for_jobs(&mut bg);
        assert_eq!(bg.take_goto_time_result(), Some(None));
    }

    #[test]
    fn a_cancelled_time_jump_does_not_move_the_view_but_the_timing_goes_on() {
        let (_dir, log) = timed_log(40_000);
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert!(bg.resolve_goto("10:12:30", 0).unwrap().waiting);
        bg.cancel_goto_time();
        assert!(!bg.goto_time_waiting());
        assert_eq!(running(&bg), Some(ScanKind::Timestamps));
        wait_for_jobs(&mut bg);
        assert_eq!(bg.take_goto_time_result(), None);
        assert_eq!(bg.scroll_to_line, None);
        assert!(bg.timestamps_complete());

        // Entering another target drops the waiting jump too.
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert!(bg.resolve_goto("10:12:30", 0).unwrap().waiting);
        assert_eq!(bg.resolve_goto("5", 0).unwrap().line, 4);
        assert!(!bg.goto_time_waiting());
    }

    #[test]
    fn filter_and_search_jobs_respect_the_window_above_the_threshold() {
        let (_dir, log) = timed_log(40_000);
        let mut sync = TailEngine::open(&log).unwrap();
        sync.apply_time_range_text("10:20", "10:40");
        let window_only = sync.filtered_lines.clone();
        sync.set_include_filter("ERROR");
        sync.update_search("svc-5");
        assert!(!sync.filtered_lines.is_empty() && !sync.search_matches.is_empty());

        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        bg.apply_time_range_text("10:20", "10:40");
        wait_for_jobs(&mut bg);
        assert!(bg.is_time_filtered());
        assert_eq!(bg.filtered_lines, window_only, "a window alone");

        bg.set_include_filter("ERROR");
        assert_eq!(
            running(&bg),
            Some(ScanKind::Filter),
            "the window no longer forces the filter onto the interface thread"
        );
        wait_for_jobs(&mut bg);
        assert_eq!(bg.filtered_lines, sync.filtered_lines);

        bg.update_search("svc-5");
        assert_eq!(running(&bg), Some(ScanKind::Search));
        wait_for_jobs(&mut bg);
        assert_eq!(bg.search_matches, sync.search_matches);
        assert!(bg.search_matches.iter().all(|&idx| bg.in_time_range(idx)));
    }

    #[test]
    fn a_rewrite_during_the_scan_times_the_new_content() {
        let (_dir, log) = timed_log(150_000);
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        bg.size_check_interval = std::time::Duration::ZERO;
        bg.apply_time_range_text("10:00:30", "");
        assert_eq!(running(&bg), Some(ScanKind::Timestamps));

        // The file is truncated and rewritten with other times while the scan runs.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_timed_log(&log, 3_000);
        wait_for_jobs(&mut bg);
        bg.poll_updates();
        wait_for_jobs(&mut bg);

        let mut sync = TailEngine::open(&log).unwrap();
        sync.apply_time_range_text("10:00:30", "");
        assert_eq!(bg.total_lines(), sync.total_lines());
        assert_same_cache(&bg, &sync);
        assert!(!bg.time_range_pending());
        assert_eq!(bg.filtered_lines, sync.filtered_lines);
    }

    #[test]
    fn background_scans_of_a_pattern_stream_read_the_matched_file() {
        // `path` is the pattern itself: the worker must open the file it resolved to.
        let dir = tempfile::tempdir().unwrap();
        write_timed_log(&dir.path().join("app-1.log"), 5_000);
        let pattern = dir.path().join("app-*.log");
        let mut bg = TailEngine::open_pattern_with_thresholds(&pattern, 0, u64::MAX).unwrap();
        wait_for_jobs(&mut bg);
        bg.apply_time_range_text("10:10", "");
        assert!(bg.time_range_pending());
        wait_for_jobs(&mut bg);
        assert!(bg.timestamps_complete());
        assert!(bg.is_time_filtered());

        let mut sync = TailEngine::open(dir.path().join("app-1.log")).unwrap();
        sync.apply_time_range_text("10:10", "");
        assert_eq!(bg.filtered_lines, sync.filtered_lines);
        assert!(!bg.filtered_lines.is_empty());
    }

    #[test]
    fn lines_appended_under_a_window_are_timed_before_they_are_filtered() {
        let (_dir, log) = timed_log(100);
        let mut engine = TailEngine::open(&log).unwrap();
        engine.size_check_interval = std::time::Duration::ZERO;
        engine.apply_time_range_text("10:00:30", "");
        let before = engine.filtered_lines.len();
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(f, "2026-09-19T11:00:00.000Z INFO appended").unwrap();
        drop(f);
        engine.poll_updates();
        let last = engine.total_lines() - 1;
        assert_eq!(engine.filtered_lines.len(), before + 1);
        assert_eq!(engine.filtered_lines.last(), Some(&last));
    }
}

// ----- ANSI escape sequences (render / strip / raw) -----

mod ansi_escape_codes {
    use super::wait_for_jobs;
    use fasttail::ansi::{AnsiColor, AnsiMode};
    use fasttail::config::FastTailConfig;
    use fasttail::log_level::LogLevel;
    use fasttail::session::{Session, StreamEntry};
    use fasttail::tail_engine::{
        HighlightRule, QuickLabel, SpanStyle, TailEngine, ViewMode, MAX_ROW_SPANS,
    };
    use std::io::Write;
    use std::path::{Path, PathBuf};

    const COLOURED: &str = "\x1b[32mINFO \x1b[0mstarted\n\
                            \x1b[31mERROR\x1b[0m payment failed\n\
                            plain line\n";

    fn log_with(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker.log");
        std::fs::write(&path, text).unwrap();
        (dir, path)
    }

    fn append(path: &Path, text: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    /// A coloured container log: every 10th line an error, colour codes around the
    /// level and the timestamp, like `docker logs` of a colourised logger.
    fn write_coloured_log(path: &Path, lines: usize) {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        for i in 0..lines {
            let (colour, level) = match i % 10 {
                0 => (31, "ERROR"),
                1 | 2 => (33, "WARN"),
                _ => (32, "INFO"),
            };
            writeln!(
                f,
                "\x1b[2m2026-09-19T10:{:02}:{:02}.000Z\x1b[0m \x1b[{colour}m{level}\x1b[0m payment svc-{} req={i}",
                (i / 60) % 60,
                i % 60,
                i % 7
            )
            .unwrap();
        }
    }

    #[test]
    fn coloured_log_resolves_to_render_and_hides_the_codes() {
        let (_dir, path) = log_with(COLOURED);
        let engine = TailEngine::open(&path).unwrap();
        assert_eq!(engine.ansi_mode, AnsiMode::Auto);
        assert_eq!(engine.ansi_effective(), AnsiMode::Render);
        assert_eq!(engine.get_line(0).as_deref(), Some("INFO started"));
        assert_eq!(engine.get_line(1).as_deref(), Some("ERROR payment failed"));
        let row = engine.get_row(1).unwrap();
        assert_eq!(row.line, "ERROR payment failed");
        assert_eq!(row.ansi.len(), 1);
        assert_eq!((row.ansi[0].start, row.ansi[0].end), (0, 5));
        assert_eq!(row.ansi[0].style.fg, Some(AnsiColor::Indexed(1)));
        assert!(!row.visible_escapes);
        let spans = engine.match_row_spans(&row);
        assert_eq!(spans.spans.len(), 1);
        assert!(matches!(spans.spans[0].style, SpanStyle::Ansi(_)));
        let (shown, _) = row.display(Some(spans));
        assert_eq!(shown, "ERROR payment failed");
    }

    #[test]
    fn plain_log_stays_auto_and_raw() {
        let (_dir, path) = log_with("2026-09-19 INFO one\n2026-09-19 ERROR two\n");
        let engine = TailEngine::open(&path).unwrap();
        assert_eq!(engine.ansi_mode, AnsiMode::Auto);
        assert_eq!(engine.ansi_effective(), AnsiMode::Raw);
        let row = engine.get_row(1).unwrap();
        assert!(row.ansi.is_empty() && !row.visible_escapes);
        assert_eq!(row.line, "2026-09-19 ERROR two");
        assert_eq!(engine.level_of(1), LogLevel::Error);
        // A non-SGR sequence (a window title) does not switch auto to render.
        let (_dir, path) = log_with("\x1b]0;title\x07 INFO x\n");
        assert_eq!(
            TailEngine::open(&path).unwrap().ansi_effective(),
            AnsiMode::Raw
        );
    }

    #[test]
    fn level_and_filters_see_the_stripped_line() {
        let (_dir, path) = log_with(COLOURED);
        let mut engine = TailEngine::open(&path).unwrap();
        assert_eq!(engine.level_of(0), LogLevel::Info);
        assert_eq!(engine.level_of(1), LogLevel::Error);
        assert_eq!(engine.level_count(LogLevel::Error), 1);
        engine.set_min_level(LogLevel::Warn);
        assert_eq!(engine.filtered_lines, vec![1]);
        engine.set_min_level(LogLevel::Unknown);
        engine.filter_is_regex = true;
        engine.set_include_filter(r"\bERROR\b");
        assert_eq!(engine.filtered_lines, vec![1]);
        // Search across the position of a removed sequence.
        engine.set_include_filter("");
        engine.update_search("error payment");
        assert_eq!(engine.search_matches, vec![1]);
    }

    #[test]
    fn raw_mode_shows_and_matches_the_escape_bytes() {
        let (_dir, path) = log_with(COLOURED);
        let mut engine = TailEngine::open(&path).unwrap();
        let generation = engine.buffer_generation;
        engine.set_ansi_mode(AnsiMode::Raw);
        assert!(engine.ansi_dirty);
        assert!(
            engine.buffer_generation != generation,
            "caches keyed by it rebuild"
        );
        assert_eq!(
            engine.get_line(1).as_deref(),
            Some("\x1b[31mERROR\x1b[0m payment failed")
        );
        // `[31mERROR` glues the token to `m`: raw text has no level any more.
        assert_eq!(engine.level_of(1), LogLevel::Unknown);
        let row = engine.get_row(1).unwrap();
        assert!(row.visible_escapes && row.ansi.is_empty());
        engine.set_quick_labels(&[QuickLabel {
            text: "payment".into(),
            color: 2,
        }]);
        let spans = engine.match_row_spans(&row);
        let (shown, spans) = row.display(Some(spans));
        assert_eq!(shown, "␛[31mERROR␛[0m payment failed");
        let sp = spans.unwrap().spans[0];
        assert_eq!(
            &shown[sp.start..sp.end],
            "payment",
            "spans follow the glyphs"
        );
        engine.filter_is_regex = true;
        engine.set_include_filter(r"\x1b\[31m");
        assert_eq!(engine.filtered_lines, vec![1]);
        // Strip: codes hidden, nothing painted.
        engine.set_include_filter("");
        engine.set_ansi_mode(AnsiMode::Strip);
        let row = engine.get_row(1).unwrap();
        assert_eq!(row.line, "ERROR payment failed");
        assert!(row.ansi.is_empty() && !row.visible_escapes);
        assert_eq!(engine.level_of(1), LogLevel::Error);
    }

    #[test]
    fn copy_and_export_follow_the_mode() {
        let (_dir, path) = log_with(COLOURED);
        let mut engine = TailEngine::open(&path).unwrap();
        engine.select_row(0);
        engine.extend_selection_to(1);
        let copied = engine.copy_selection_text().unwrap();
        assert_eq!(copied, "INFO started\nERROR payment failed");
        let mut out = Vec::new();
        engine.export_visible(&mut out).unwrap();
        assert!(!out.contains(&0x1b));
        let ctx = fasttail::ui::dock::tool_context_for_row(&engine, 1).unwrap();
        assert_eq!(ctx.line, "ERROR payment failed");

        engine.set_ansi_mode(AnsiMode::Strip);
        engine.select_row(1);
        assert_eq!(
            engine.copy_selection_text().unwrap(),
            "ERROR payment failed"
        );
        engine.set_ansi_mode(AnsiMode::Raw);
        engine.select_row(1);
        assert_eq!(
            engine.copy_selection_text().unwrap(),
            "\x1b[31mERROR\x1b[0m payment failed",
            "raw copies exactly the file's text"
        );
    }

    #[test]
    fn colours_deep_in_a_large_append_are_detected() {
        let (_dir, path) = log_with("started\n");
        let mut engine = TailEngine::open(&path).unwrap();
        engine.size_check_interval = std::time::Duration::ZERO;
        // One append far larger than the detection sample, the colour at its end.
        let mut burst: String = (0..20_000).map(|i| format!("plain line {i}\n")).collect();
        assert!(burst.len() > 4 * fasttail::ansi::DETECT_SAMPLE_BYTES);
        burst.push_str("\x1b[31mERROR\x1b[0m payment failed\n");
        append(&path, &burst);
        engine.poll_updates();
        wait_for_jobs(&mut engine);
        assert_eq!(engine.ansi_effective(), AnsiMode::Render);
        assert_eq!(
            engine.get_line(20_001).as_deref(),
            Some("ERROR payment failed")
        );

        // Opened empty, like a decompression spool: the first append is all new bytes.
        let (_dir, path) = log_with("");
        let mut engine = TailEngine::open(&path).unwrap();
        engine.size_check_interval = std::time::Duration::ZERO;
        append(&path, &burst);
        engine.poll_updates();
        wait_for_jobs(&mut engine);
        assert_eq!(engine.ansi_effective(), AnsiMode::Render);

        // At open only the head is sampled: a file is not read end to end to decide.
        let (_dir, path) = log_with(&burst);
        let engine = TailEngine::open(&path).unwrap();
        assert_eq!(engine.ansi_effective(), AnsiMode::Raw);
    }

    #[test]
    fn colours_after_the_banner_switch_auto_once_and_rescan() {
        let banner: String = (0..10_000).map(|i| format!("banner {i}\n")).collect();
        let (_dir, path) = log_with(&banner);
        let mut engine = TailEngine::open(&path).unwrap();
        engine.size_check_interval = std::time::Duration::ZERO;
        engine.set_include_filter("ERROR payment");
        engine.update_search("error payment");
        assert!(engine.filtered_lines.is_empty());
        assert_eq!(engine.ansi_effective(), AnsiMode::Raw);

        append(&path, "\x1b[31mERROR\x1b[0m payment failed\n");
        engine.poll_updates();
        assert_eq!(engine.ansi_effective(), AnsiMode::Render);
        assert!(engine.ansi_switch_notice());
        assert_eq!(engine.filtered_lines, vec![10_000]);
        assert_eq!(engine.search_matches, vec![10_000]);
        assert_eq!(engine.level_of(10_000), LogLevel::Error);
        assert!(engine.levels_complete());
        assert_eq!(engine.level_count(LogLevel::Error), 1);
        assert_eq!(engine.ansi_mode, AnsiMode::Auto, "resolved, not chosen");
        assert!(!engine.ansi_dirty);
    }

    #[test]
    fn background_scans_match_the_synchronous_path() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("coloured.log");
        write_coloured_log(&log, 30_000);

        let mut sync = TailEngine::open(&log).unwrap();
        assert_eq!(sync.ansi_effective(), AnsiMode::Render);
        sync.set_include_filter("ERROR payment");
        assert!(sync.scan_progress().is_none());
        let expected = sync.filtered_lines.clone();
        assert_eq!(expected.len(), 3_000);
        sync.set_include_filter("");
        sync.update_search("warn payment");
        let expected_search = sync.search_matches.clone();
        assert_eq!(expected_search.len(), 6_000);
        sync.ensure_timestamps();

        let mut bg = TailEngine::open_with_thresholds(&log, 0, 0).unwrap();
        wait_for_jobs(&mut bg);
        assert_eq!(bg.ansi_effective(), AnsiMode::Render);
        assert_eq!(
            bg.level_count(LogLevel::Error),
            3_000,
            "background level scan"
        );
        bg.set_include_filter("ERROR payment");
        assert!(bg.scan_progress().is_some(), "large-file path spawns a job");
        wait_for_jobs(&mut bg);
        assert_eq!(bg.filtered_lines, expected);
        bg.set_include_filter("");
        wait_for_jobs(&mut bg);
        bg.update_search("warn payment");
        wait_for_jobs(&mut bg);
        assert_eq!(bg.search_matches, expected_search);
        // Timestamps behind a colour code, timed on a worker thread.
        bg.resolve_goto("10:05:00", 0);
        wait_for_jobs(&mut bg);
        assert_eq!(bg.timestamp_cache(), sync.timestamp_cache());
        assert!(bg.timestamps_usable());

        // A mode change restarts the scans on the new text.
        bg.update_search("");
        bg.set_include_filter("ERROR payment");
        wait_for_jobs(&mut bg);
        bg.set_ansi_mode(AnsiMode::Raw);
        wait_for_jobs(&mut bg);
        assert!(
            bg.filtered_lines.is_empty(),
            "raw text has codes in between"
        );
        assert_eq!(bg.level_count(LogLevel::Error), 0);
        bg.set_ansi_mode(AnsiMode::Render);
        wait_for_jobs(&mut bg);
        assert_eq!(bg.filtered_lines, expected);
        assert_eq!(bg.level_count(LogLevel::Error), 3_000);
    }

    #[test]
    fn user_rules_and_labels_rank_above_ansi_colours() {
        let (_dir, path) =
            log_with("\x1b[31mERROR\x1b[0m payment sess-8f3a \x1b[33mreq=42 slow\x1b[0m\n");
        let mut engine = TailEngine::open(&path).unwrap();
        let row = engine.get_row(0).unwrap();
        assert_eq!(row.line, "ERROR payment sess-8f3a req=42 slow");

        // A whole-row rule colours the whole row.
        engine.set_highlight_rules(vec![HighlightRule::new(
            "payment",
            [0, 255, 0],
            [0, 0, 0],
            false,
        )]);
        let hl = engine.match_row_spans(&row);
        assert!(hl.spans.is_empty());
        assert_eq!(
            hl.rest.map(|s| s.fg),
            Some(egui::Color32::from_rgb(0, 255, 0))
        );

        // A capture inside a yellow ANSI span, and a quick label: both win on their bytes.
        engine.set_highlight_rules(vec![HighlightRule::captures(
            r"req=(\d+)",
            [0, 255, 255],
            [0, 0, 0],
        )]);
        engine.set_quick_labels(&[QuickLabel {
            text: "sess-8f3a".into(),
            color: 5,
        }]);
        let hl = engine.match_row_spans(&row);
        assert!(hl.rest.is_none());
        let text = &row.line;
        let pieces: Vec<(&str, &str)> = hl
            .spans
            .iter()
            .map(|s| {
                let kind = match s.style {
                    SpanStyle::Rule(_) => "rule",
                    SpanStyle::Label(_) => "label",
                    SpanStyle::Ansi(_) => "ansi",
                };
                (&text[s.start..s.end], kind)
            })
            .collect();
        assert_eq!(
            pieces,
            vec![
                ("ERROR", "ansi"),
                ("sess-8f3a", "label"),
                ("req=", "ansi"),
                ("42", "rule"),
                (" slow", "ansi"),
            ]
        );
    }

    #[test]
    fn ansi_spans_share_the_row_budget() {
        let line: String = (0..200)
            .map(|i| format!("\x1b[3{}mx{i} ", i % 8))
            .collect::<String>()
            + "\n";
        let (_dir, path) = log_with(&line);
        let mut engine = TailEngine::open(&path).unwrap();
        let row = engine.get_row(0).unwrap();
        assert!(row.ansi.len() > MAX_ROW_SPANS);
        assert_eq!(engine.match_row_spans(&row).spans.len(), MAX_ROW_SPANS);
        // A label claims first; the ANSI runs fill what is left of the budget.
        engine.set_quick_labels(&[QuickLabel {
            text: "x199".into(),
            color: 1,
        }]);
        let hl = engine.match_row_spans(&row);
        assert_eq!(hl.spans.len(), MAX_ROW_SPANS);
        assert!(hl.spans.iter().any(|s| s.style == SpanStyle::Label(1)));
    }

    #[test]
    fn search_cursor_is_carried_to_the_file_bytes_in_hex() {
        let text = "payment payment ok\n\x1b[1;31mERROR\x1b[0m \x1b[4mpayment\x1b[0m failed\n";
        let (_dir, path) = log_with(text);
        let mut engine = TailEngine::open(&path).unwrap();
        engine.update_search("payment");
        assert_eq!(engine.search_matches, vec![0, 1]);
        engine.search_next(false);
        assert_eq!(engine.current_search_line(), Some(1));
        engine.set_view_mode(ViewMode::Hex);
        let expected = text.rfind("payment").unwrap();
        assert_eq!(engine.current_search_byte(), Some((expected, 7)));
        assert_eq!(engine.scroll_to_byte, Some(expected));
        // HEX shows the file bytes, escapes included.
        let bytes = engine.get_bytes(19, 4).unwrap();
        assert_eq!(bytes, b"\x1b[1;");
    }

    #[test]
    fn utf16_logs_are_detected_and_stripped() {
        let text = "\x1b[31mERROR\x1b[0m boom\r\nplain\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u16.log");
        std::fs::write(&path, bytes).unwrap();
        let engine = TailEngine::open(&path).unwrap();
        assert_eq!(engine.ansi_effective(), AnsiMode::Render);
        assert_eq!(engine.get_line(0).as_deref(), Some("ERROR boom"));
        assert_eq!(engine.level_of(0), LogLevel::Error);
    }

    #[test]
    fn line_cut_by_the_length_cap_keeps_its_marker() {
        let mut line = "\x1b[31m".to_string();
        line.push_str(&"a".repeat(fasttail::tail_engine::MAX_LINE_BYTES));
        line.push_str("\x1b]8;;cut after the cap\n");
        let (_dir, path) = log_with(&line);
        let engine = TailEngine::open(&path).unwrap();
        let text = engine.get_line(0).unwrap();
        assert!(text.ends_with(fasttail::tail_engine::TRUNCATED_LINE_MARKER));
        assert!(!text.contains('\x1b'));
    }

    #[test]
    fn mode_is_persisted_in_sessions_and_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        std::fs::write(&log, "a\n").unwrap();
        let mut entry = StreamEntry::new(log.clone());
        entry.ansi = Some("raw".to_string());
        let session = Session {
            streams: vec![entry.clone()],
            dock_layout: None,
        };
        let file = dir.path().join("s.fasttail-session.ini");
        session.save_to(&file).unwrap();
        let loaded = Session::load_from(&file).unwrap();
        assert_eq!(loaded.session.streams[0].ansi.as_deref(), Some("raw"));

        // The workspace keeps it in the stream entry of the default session.
        let mut cfg = FastTailConfig::default();
        cfg.open_files = vec![log.clone()];
        cfg.set_stream_state(entry);
        let restored = FastTailConfig::from_ini(&cfg.to_ini());
        assert_eq!(
            restored.stream_state_for(&log).unwrap().ansi.as_deref(),
            Some("raw")
        );

        // Old files without the key, and an explicit auto, read as auto.
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("ansi=raw"));
        std::fs::write(&file, text.replace("ansi=raw", "")).unwrap();
        assert_eq!(
            Session::load_from(&file).unwrap().session.streams[0].ansi,
            None
        );
        std::fs::write(&file, text.replace("ansi=raw", "ansi=auto")).unwrap();
        assert_eq!(
            Session::load_from(&file).unwrap().session.streams[0].ansi,
            None
        );
        // Auto is not written at all.
        let auto = Session {
            streams: vec![StreamEntry::new(log)],
            dock_layout: None,
        };
        assert!(!auto.serialized(None).contains("ansi"));
    }

    #[test]
    fn ansi_i18n_keys() {
        for lang in fasttail::i18n::Language::ALL {
            for key in [
                "ansi_auto",
                "ansi_render",
                "ansi_strip",
                "ansi_raw",
                "tip_ansi",
                "ansi_switched",
            ] {
                assert_ne!(
                    fasttail::i18n::t(*lang, key),
                    "Unknown",
                    "{key} for {lang:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Search results pane: the match cap with the true total, error block counts, the
// pane preferences and the pane keys.
// ---------------------------------------------------------------------------

mod search_results_pane {
    use super::wait_for_jobs;
    use fasttail::config::FastTailConfig;
    use fasttail::i18n::{t, Language};
    use fasttail::log_level::LogLevel;
    use fasttail::tail_engine::{
        is_error_level, TailEngine, ERROR_BLOCK_LINES, MAX_SEARCH_MATCHES,
    };
    use std::io::Write;

    /// `hits` lines containing `x`, one `y` line after every 1000th of them.
    fn write_many_hits(path: &std::path::Path, hits: usize) {
        let mut text = String::with_capacity(hits * 2 + hits / 500);
        for i in 0..hits {
            text.push_str("x\n");
            if i % 1000 == 999 {
                text.push_str("y\n");
            }
        }
        std::fs::write(path, text).unwrap();
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    fn assert_capped(engine: &TailEngine, total: usize) {
        assert_eq!(engine.search_matches.len(), MAX_SEARCH_MATCHES);
        assert_eq!(engine.search_total(), total);
        assert!(engine.search_capped());
        assert!(engine.search_matches.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn sync_search_counts_past_the_cap_and_on_append() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("many.log");
        let hits = MAX_SEARCH_MATCHES + 2_345;
        write_many_hits(&log, hits);

        let mut engine = TailEngine::open(&log).unwrap();
        let generation = engine.search_generation;
        engine.update_search("X");
        assert_capped(&engine, hits);
        assert_ne!(engine.search_generation, generation, "the UI caches see it");

        // F3 wraps within the stored hits.
        assert_eq!(
            engine.select_match(MAX_SEARCH_MATCHES - 1),
            Some(engine.search_matches[MAX_SEARCH_MATCHES - 1])
        );
        assert_eq!(engine.search_next(false), Some(engine.search_matches[0]));
        assert_eq!(engine.current_match_idx, Some(0));
        assert_eq!(engine.select_match(MAX_SEARCH_MATCHES), None);

        // Appended hits are counted; a partial last line re-examined is not counted twice.
        append(&log, "x\ny\nxx");
        engine.poll_updates();
        assert_capped(&engine, hits + 2);
        append(&log, "yy\nx\n");
        engine.poll_updates();
        assert_capped(&engine, hits + 3);
        assert_eq!(engine.current_match_idx, Some(0), "the current match stays");

        // Cleared and searched again from scratch: the same total.
        engine.update_search("");
        assert_eq!(engine.search_total(), 0);
        assert!(!engine.search_capped());
        engine.update_search("x");
        assert_capped(&engine, hits + 3);
    }

    #[test]
    fn background_search_counts_past_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("many_bg.log");
        let hits = MAX_SEARCH_MATCHES + 777;
        write_many_hits(&log, hits);

        let mut engine = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        wait_for_jobs(&mut engine);
        engine.update_search("x");
        assert!(engine.scan_progress().is_some(), "runs on the worker");
        wait_for_jobs(&mut engine);
        assert_capped(&engine, hits);
        let sync = TailEngine::open(&log).unwrap().find_matches("x");
        assert_eq!(
            engine.search_matches, sync,
            "the same first hits as the sync path"
        );

        append(&log, "x\nx\n");
        engine.poll_updates();
        wait_for_jobs(&mut engine);
        assert_capped(&engine, hits + 2);
    }

    #[test]
    fn totals_below_the_cap_match_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("few.log");
        std::fs::write(&log, "a x\nb\nc x\n").unwrap();
        for mut engine in [
            TailEngine::open(&log).unwrap(),
            TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap(),
        ] {
            wait_for_jobs(&mut engine);
            engine.update_search("x");
            wait_for_jobs(&mut engine);
            assert_eq!(engine.search_matches, vec![0, 2]);
            assert_eq!(engine.search_total(), 2);
            assert!(!engine.search_capped());
            engine.set_include_filter("a");
            wait_for_jobs(&mut engine);
            assert_eq!(engine.search_total(), 1, "hidden hits are not counted");
        }
    }

    /// The block counts agree with the level cache they summarize.
    fn assert_blocks_match_levels(engine: &TailEngine) {
        let levels = engine.cached_levels();
        let blocks = engine.error_block_counts();
        assert_eq!(blocks.len(), levels.len().div_ceil(ERROR_BLOCK_LINES));
        for (b, &count) in blocks.iter().enumerate() {
            let end = ((b + 1) * ERROR_BLOCK_LINES).min(levels.len());
            let expected = levels[b * ERROR_BLOCK_LINES..end]
                .iter()
                .filter(|&&v| is_error_level(v))
                .count();
            assert_eq!(count as usize, expected, "block {b}");
        }
        assert_eq!(
            engine.error_lines_in(0..levels.len()),
            engine.level_count(LogLevel::Error) + engine.level_count(LogLevel::Fatal)
        );
    }

    fn write_levels_log(path: &std::path::Path, lines: usize) {
        let mut text = String::new();
        for i in 0..lines {
            let level = match i % 7 {
                0 => "ERROR",
                3 => "FATAL",
                _ => "INFO",
            };
            text.push_str(&format!("2026-09-25 10:00:00 {level} line {i}\n"));
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn error_block_counts_follow_appends_and_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("levels.log");
        write_levels_log(&log, 10_000);
        append(&log, "2026-09-25 10:00:01 ERROR partial");

        let mut engine = TailEngine::open(&log).unwrap();
        assert!(engine.levels_complete());
        assert_blocks_match_levels(&engine);
        let before = engine.error_lines_in(0..engine.total_lines());

        // The partial last line is re-read (its level truncated and pushed again).
        append(
            &log,
            " still\n2026-09-25 10:00:02 FATAL next\n2026-09-25 10:00:03 INFO ok\n",
        );
        engine.poll_updates();
        assert!(engine.levels_complete());
        assert_blocks_match_levels(&engine);
        assert_eq!(engine.error_lines_in(0..engine.total_lines()), before + 1);

        // Ranges that start and end inside blocks.
        let direct = |r: std::ops::Range<usize>| {
            r.filter(|&l| is_error_level(engine.level_of(l) as u8))
                .count() as u64
        };
        for range in [
            0..1,
            5..4100,
            4095..8193,
            100..engine.total_lines(),
            9_000..20_000,
        ] {
            assert_eq!(
                engine.error_lines_in(range.clone()),
                direct(range.start..range.end.min(engine.total_lines())),
                "{range:?}"
            );
        }

        // A shorter rewrite drops the counts of the lines that are gone.
        write_levels_log(&log, 5_000);
        engine.poll_updates();
        assert_eq!(engine.total_lines(), 5_000);
        assert_blocks_match_levels(&engine);
    }

    #[test]
    fn error_block_counts_survive_a_resumed_levels_job() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("levels_bg.log");
        write_levels_log(&log, 30_000);
        let sync = TailEngine::open(&log).unwrap();

        let mut engine = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        // A search takes over from the level scan, which resumes from its prefix.
        engine.update_search("line 1");
        wait_for_jobs(&mut engine);
        assert!(engine.levels_complete());
        assert_blocks_match_levels(&engine);
        assert_eq!(engine.error_block_counts(), sync.error_block_counts());
    }

    #[test]
    fn pane_preferences_round_trip_through_the_ini() {
        let mut cfg = FastTailConfig::default();
        assert!(!cfg.search_pane, "the pane is off by default");
        assert!(cfg.overview_strip, "the strip is on by default");
        cfg.search_pane = true;
        cfg.search_pane_height = 321.0;
        cfg.overview_strip = false;
        let restored = FastTailConfig::from_ini(&cfg.to_ini());
        assert!(restored.search_pane);
        assert_eq!(restored.search_pane_height, 321.0);
        assert!(!restored.overview_strip);

        // A height out of range keeps the default.
        let mut ini = cfg.to_ini();
        ini.with_section(Some("general"))
            .set("search_pane_height", "-5");
        let restored = FastTailConfig::from_ini(&ini);
        assert_eq!(
            restored.search_pane_height,
            fasttail::ui::dock::DEFAULT_SEARCH_PANE_HEIGHT
        );
    }

    #[test]
    fn pane_i18n_keys_are_translated_everywhere() {
        let keys = [
            "tip_search_pane",
            "search_pane_header",
            "search_capped",
            "search_pane_no_matches",
            "search_pane_hex",
            "overview_strip",
            "overview_strip_tip",
            "overview_sampled",
            "help_desc_search_pane",
        ];
        for lang in Language::ALL {
            for key in keys {
                let text = t(*lang, key);
                assert!(!text.is_empty() && text != "Unknown", "{key} for {lang:?}");
                if *lang != Language::En {
                    assert_ne!(text, t(Language::En, key), "{key} untranslated in {lang:?}");
                }
            }
            assert!(t(*lang, "search_capped").contains("{n}"), "{lang:?}");
        }
        assert_eq!(fasttail::ui::dock::group_thousands(3_412_009), "3,412,009");
        assert_eq!(fasttail::ui::dock::group_thousands(999), "999");
        assert_eq!(fasttail::ui::dock::group_thousands(1_000), "1,000");
    }

    /// One stream in a dock, drawn frame by frame with the given input events.
    struct Harness {
        engines: Vec<TailEngine>,
        open_files: Vec<std::path::PathBuf>,
        dock: egui_dock::DockState<fasttail::ui::dock::FastTailTab>,
        prefs: fasttail::ui::dock::SearchViewPrefs,
        ctx: egui::Context,
        path: std::path::PathBuf,
    }

    impl Harness {
        fn new(path: &std::path::Path, query: &str) -> Self {
            use fasttail::ui::dock::FastTailTab;
            let mut engine = TailEngine::open(path).unwrap();
            engine.search_query = query.to_string();
            Self {
                engines: vec![engine],
                open_files: vec![path.to_path_buf()],
                dock: egui_dock::DockState::new(vec![FastTailTab::LogStream(path.to_path_buf())]),
                prefs: fasttail::ui::dock::SearchViewPrefs {
                    search_pane: true,
                    ..Default::default()
                },
                ctx: egui::Context::default(),
                path: path.to_path_buf(),
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            use fasttail::ui::dock::{DockContext, FastTailTabViewer};
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = self.ctx.run_ui(input, |ui| {
                let dock_ctx = DockContext {
                    engines: &mut self.engines,
                    open_files: &mut self.open_files,
                    theme: &mut fasttail::theme::CyberTheme::Tron,
                    language: &mut Language::En,
                    global_rules: &mut Vec::new(),
                    screensaver_enabled: &mut false,
                    screensaver_timeout_mins: &mut 5,
                    telemetry_enabled: &mut false,
                    sound_enabled: &mut false,
                    borderless: &mut false,
                    show_line_numbers: &mut true,
                    font_size: &mut 13.0,
                    level_colors: &mut true,
                    size_unit: &mut fasttail::tail_engine::SizeUnit::Bytes,
                    search_history: &mut Vec::new(),
                    tab_closed: &mut false,
                    test_screensaver: &mut false,
                    language_auto: &mut false,
                    lock_enabled: &mut false,
                    lock_pin: &mut String::new(),
                    lock_now: &mut false,
                    quick_labels: &mut Vec::new(),
                    labels_changed: &mut false,
                    external_tools: &mut Vec::new(),
                    tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
                    focused_stream: Some(self.path.clone()),
                    search_view: &mut self.prefs,
                    time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
                    find_all: &mut fasttail::find_all::FindAllSession::default(),
                    filter_presets: &mut Vec::new(),
                    preset_events: &mut Default::default(),
                };
                let mut viewer = FastTailTabViewer { ctx: dock_ctx };
                egui_dock::DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
            });
            out.textures_delta.clear();
        }

        fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }]);
            self.frame(Vec::new());
        }

        fn click(&mut self, pos: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(Vec::new());
        }

        fn engine(&self) -> &TailEngine {
            &self.engines[0]
        }

        fn pane_id(&self) -> egui::Id {
            fasttail::ui::dock::search_pane_id(self.engine())
        }

        fn pane_state(&self) -> fasttail::ui::hit_list::HitListState {
            let id = self.pane_id();
            self.ctx
                .data(|d| d.get_temp(id))
                .expect("the pane was drawn")
        }

        fn pane_focused(&self) -> bool {
            let id = self.pane_id();
            self.ctx.memory(|m| m.has_focus(id))
        }

        /// Screen rect of the pane row of hit `hit`, as laid out last frame.
        fn hit_row(&self, hit: usize) -> egui::Rect {
            let id = self.pane_id().with(("hit_row", hit));
            self.ctx
                .read_response(id)
                .map(|r| r.rect)
                .expect("the hit row is on screen")
        }
    }

    fn pane_log(dir: &std::path::Path) -> std::path::PathBuf {
        let log = dir.join("pane.log");
        let mut text = String::new();
        for i in 0..2_000 {
            if i % 50 == 7 {
                text.push_str(&format!("2026-09-25 WARN request {i} timeout\n"));
            } else {
                text.push_str(&format!("2026-09-25 INFO request {i} ok\n"));
            }
        }
        std::fs::write(&log, text).unwrap();
        log
    }

    #[test]
    fn clicking_a_pane_row_makes_it_current_and_focuses_the_pane() {
        let dir = tempfile::tempdir().unwrap();
        let log = pane_log(dir.path());
        let mut h = Harness::new(&log, "timeout");
        h.frame(Vec::new());
        h.frame(Vec::new());
        assert_eq!(h.engine().search_matches.len(), 40);
        assert_eq!(h.engine().current_match_idx, Some(0));
        assert!(h.engine().follow_tail);

        let row = h.hit_row(3);
        h.click(row.center());
        assert_eq!(h.engine().current_match_idx, Some(3), "the clicked hit");
        assert!(!h.engine().follow_tail, "follow is paused");
        assert!(h.pane_focused(), "the pane has the keyboard");
        assert_eq!(h.pane_state().selected, 3);

        // Arrow keys move the selection only; Enter commits it.
        for _ in 0..60 {
            h.frame(Vec::new()); // the animated scroll to the hit settles
        }
        let scroll = h.engine().current_scroll_y;
        for _ in 0..5 {
            h.key(egui::Key::ArrowDown, egui::Modifiers::NONE);
        }
        assert_eq!(h.pane_state().selected, 8);
        assert_eq!(
            h.engine().current_match_idx,
            Some(3),
            "the current match stays"
        );
        assert_eq!(h.engine().current_scroll_y, scroll, "the main view stays");
        h.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(h.engine().current_match_idx, Some(8));
        assert!(h.pane_focused());

        // F3 keeps its stream meaning with the pane focused; the selection follows.
        h.key(egui::Key::F3, egui::Modifiers::NONE);
        assert_eq!(h.engine().current_match_idx, Some(9));
        assert_eq!(h.pane_state().selected, 9);

        // Esc hands the keyboard back: PgDown scrolls the main view, not the selection.
        h.key(egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!h.pane_focused());
        let scroll = h.engine().current_scroll_y;
        h.key(egui::Key::PageDown, egui::Modifiers::NONE);
        h.frame(Vec::new());
        assert_eq!(h.pane_state().selected, 9);
        assert!(
            h.engine().current_scroll_y > scroll,
            "the main view paged down"
        );
    }

    #[test]
    fn overview_strip_marks_hits_beside_the_rows() {
        let dir = tempfile::tempdir().unwrap();
        let log = pane_log(dir.path());
        let mut h = Harness::new(&log, "timeout");
        h.frame(Vec::new());
        h.frame(Vec::new());
        let id = egui::Id::new("overview_cache").with(&h.engine().path);
        let cache: fasttail::ui::overview_strip::StripCache =
            h.ctx.data(|d| d.get_temp(id)).expect("the strip cache");
        let hits = cache.marks.pixels.iter().filter(|&&p| p != 0).count();
        assert!(hits > 10, "hits are marked: {hits}");
        let strip = h
            .ctx
            .read_response(egui::Id::new("overview_strip").with(&h.engine().path))
            .expect("the strip is drawn");
        assert_eq!(
            strip.rect.width(),
            fasttail::ui::overview_strip::STRIP_WIDTH
        );
    }

    #[test]
    fn pane_is_hidden_without_a_query_and_notes_the_hex_view() {
        let dir = tempfile::tempdir().unwrap();
        let log = pane_log(dir.path());
        let mut h = Harness::new(&log, "");
        h.frame(Vec::new());
        h.frame(Vec::new());
        let id = h.pane_id();
        assert!(
            h.ctx
                .data(|d| d.get_temp::<fasttail::ui::hit_list::HitListState>(id))
                .is_none(),
            "no query, no pane"
        );

        h.engines[0].search_query = "timeout".into();
        h.engines[0].set_view_mode(fasttail::tail_engine::ViewMode::Hex);
        h.frame(Vec::new());
        h.frame(Vec::new());
        assert!(
            h.ctx
                .data(|d| d.get_temp::<fasttail::ui::hit_list::HitListState>(id))
                .is_none(),
            "HEX view shows a notice, not the list"
        );
        assert!(h.prefs.search_pane, "the preference is untouched");
    }

    /// 120 entries, one per second from 10:00:00, an ERROR every tenth.
    fn timeline_log(dir: &std::path::Path) -> std::path::PathBuf {
        let log = dir.join("timeline.log");
        let mut text = String::new();
        for i in 0..120 {
            let level = if i % 10 == 0 { "ERROR" } else { "INFO" };
            text.push_str(&format!(
                "2026-09-25 10:{:02}:{:02} {level} request {i}\n",
                i / 60,
                i % 60
            ));
        }
        std::fs::write(&log, text).unwrap();
        log
    }

    fn timeline_harness(log: &std::path::Path) -> Harness {
        let mut h = Harness::new(log, "");
        h.prefs.timeline_histogram = true;
        h.frame(Vec::new());
        h.frame(Vec::new());
        h
    }

    fn strip_rect(h: &Harness) -> egui::Rect {
        h.ctx
            .read_response(fasttail::ui::timeline_strip::strip_id(h.engine()))
            .expect("the strip is drawn")
            .rect
    }

    #[test]
    fn opening_the_timeline_times_the_stream_and_draws_every_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = timeline_log(dir.path());
        let mut h = timeline_harness(&log);
        assert!(
            h.engine().timestamps_complete(),
            "opening the strip times the log"
        );
        let histogram = h.engine().time_histogram();
        assert_eq!(histogram.timed(), 120);
        assert_eq!(histogram.len(), 120, "one-second buckets");
        let rect = strip_rect(&h);
        assert_eq!(rect.height(), fasttail::ui::timeline_strip::STRIP_HEIGHT);

        // Closed again: no strip.
        h.prefs.timeline_histogram = false;
        h.frame(Vec::new());
        h.frame(Vec::new());
        let id = fasttail::ui::timeline_strip::strip_id(h.engine());
        assert!(h.ctx.read_response(id).is_none());
    }

    #[test]
    fn clicking_a_bar_sets_the_time_range_to_its_second() {
        let dir = tempfile::tempdir().unwrap();
        let log = timeline_log(dir.path());
        let mut h = timeline_harness(&log);
        let rect = strip_rect(&h);
        // 120 buckets across the strip: the middle of bucket 30 is 10:00:30.
        let x = rect.left() + rect.width() * (30.5 / 120.0);
        h.click(egui::pos2(x, rect.center().y));
        let engine = h.engine();
        assert_eq!(engine.time_from_text, "2026-09-25 10:00:30");
        assert_eq!(engine.time_to_text, "2026-09-25 10:00:30");
        assert!(engine.is_time_filtered());
        assert_eq!(engine.filtered_lines, vec![30]);
    }

    #[test]
    fn dragging_across_bars_sets_the_dragged_span() {
        let dir = tempfile::tempdir().unwrap();
        let log = timeline_log(dir.path());
        let mut h = timeline_harness(&log);
        let rect = strip_rect(&h);
        let at = |bucket: f32| {
            egui::pos2(
                rect.left() + rect.width() * (bucket / 120.0),
                rect.center().y,
            )
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // Right to left: the order of the ends does not matter.
        h.frame(vec![egui::Event::PointerMoved(at(90.5))]);
        h.frame(vec![button(at(90.5), true)]);
        for b in [85.5, 75.5, 65.5, 60.5] {
            h.frame(vec![egui::Event::PointerMoved(at(b))]);
        }
        h.frame(vec![button(at(60.5), false)]);
        h.frame(Vec::new());
        let engine = h.engine();
        assert_eq!(engine.time_from_text, "2026-09-25 10:01:00");
        assert_eq!(engine.time_to_text, "2026-09-25 10:01:30");
        assert_eq!(engine.filtered_lines, (60..=90).collect::<Vec<_>>());
    }

    #[test]
    fn a_stream_without_timestamps_gets_the_hint_instead_of_the_strip() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("plain.log");
        let text: String = (0..500).map(|i| format!("plain line {i}\n")).collect();
        std::fs::write(&log, text).unwrap();
        let h = timeline_harness(&log);
        assert!(h.engine().timestamps_complete());
        assert!(!h.engine().timestamps_usable());
        let id = fasttail::ui::timeline_strip::strip_id(h.engine());
        assert!(
            h.ctx.read_response(id).is_none(),
            "no strip, the hint instead"
        );
    }

    #[test]
    fn timeline_preferences_round_trip_through_the_ini() {
        let mut cfg = FastTailConfig::default();
        assert!(!cfg.timeline_histogram, "the timeline is off by default");
        assert!(cfg.timeline_search_lane, "its search lane is on by default");
        cfg.timeline_histogram = true;
        cfg.timeline_search_lane = false;
        let restored = FastTailConfig::from_ini(&cfg.to_ini());
        assert!(restored.timeline_histogram);
        assert!(!restored.timeline_search_lane);
    }

    #[test]
    fn timeline_i18n_keys_are_translated_everywhere() {
        let keys = [
            "timeline_tip",
            "timeline_search_lane_tip",
            "timeline_peak",
            "timeline_empty",
            "timeline_no_level",
            "timeline_untimed",
            "timeline_lane_note",
        ];
        for lang in Language::ALL {
            for key in keys {
                let text = t(*lang, key);
                assert!(!text.is_empty() && text != "Unknown", "{key} for {lang:?}");
                if *lang != Language::En {
                    assert_ne!(text, t(Language::En, key), "{key} untranslated in {lang:?}");
                }
            }
            assert!(t(*lang, "timeline_peak").contains("{n}"), "{lang:?}");
            assert!(t(*lang, "timeline_untimed").contains("{n}"), "{lang:?}");
        }
    }
}

// ----- Timeline histogram -----

mod timeline_histogram {
    use super::background_timestamps::write_timed_log;
    use super::wait_for_jobs;
    use fasttail::log_level::LogLevel;
    use fasttail::scan_job::ScanKind;
    use fasttail::tail_engine::TailEngine;
    use fasttail::time_histogram::TimeHistogram;
    use std::io::Write;

    /// The histogram rebuilt from the engine's caches at the width the incremental one
    /// reached: the two must be equal after every event.
    fn assert_matches_caches(engine: &TailEngine) {
        let incremental = engine.time_histogram();
        let (stamps, _, _, _) = engine.timestamp_cache();
        let levels = engine.cached_levels();
        let lines = stamps.len().min(levels.len());
        assert_eq!(
            engine.histogram_lines(),
            lines,
            "every line with both caches"
        );
        let mut rebuilt = TimeHistogram::with_bucket_ms(incremental.bucket_ms());
        for idx in 0..lines {
            rebuilt.add(stamps[idx], levels[idx]);
        }
        assert_eq!(incremental, &rebuilt);
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    fn timed_log(entries: usize) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("timeline.log");
        write_timed_log(&log, entries);
        (dir, log)
    }

    #[test]
    fn built_only_once_the_stream_is_timed_and_counts_every_line() {
        let (_dir, log) = timed_log(3_000);
        let mut engine = TailEngine::open(&log).unwrap();
        assert!(
            engine.time_histogram().is_empty(),
            "nothing timed, nothing built"
        );
        engine.set_include_filter("svc-3");
        engine.request_timeline();
        assert!(engine.timestamps_complete());
        assert_matches_caches(&engine);
        let histogram = engine.time_histogram();
        // Filters do not hide the histogram's data; the banner is the only untimed line.
        assert_eq!(histogram.untimed(), 1);
        assert_eq!(histogram.timed() as usize, engine.total_lines() - 1);
        let errors: u64 = (0..histogram.len())
            .map(|i| u64::from(histogram.bucket(i).unwrap()[LogLevel::Error as usize]))
            .sum();
        assert_eq!(errors, 300);
        // 3,000 s fit in 2,048 buckets of 2 s.
        assert_eq!(histogram.bucket_ms(), 2_000);
    }

    #[test]
    fn appends_and_a_completed_partial_line_are_counted_once() {
        let (_dir, log) = timed_log(1_000);
        let mut engine = TailEngine::open(&log).unwrap();
        engine.size_check_interval = std::time::Duration::ZERO;
        engine.request_timeline();
        let before = engine.time_histogram().timed();
        append(
            &log,
            "2026-09-19T11:00:00.000Z ERROR appended\n2026-09-19T11:00:01.000Z IN",
        );
        engine.poll_updates();
        assert_matches_caches(&engine);
        // The partial last line is re-evaluated with the rest of it: removed, added again.
        append(&log, "FO done\n");
        engine.poll_updates();
        assert_matches_caches(&engine);
        let histogram = engine.time_histogram();
        assert_eq!(histogram.timed(), before + 2);
        let last = histogram.bucket(histogram.len() - 1).unwrap();
        assert_eq!(
            last[LogLevel::Info as usize],
            1,
            "the completed line is INFO"
        );
    }

    #[test]
    fn a_rewrite_empties_the_histogram_then_shows_the_new_lines() {
        let (_dir, log) = timed_log(5_000);
        let mut engine = TailEngine::open(&log).unwrap();
        engine.size_check_interval = std::time::Duration::ZERO;
        engine.request_timeline();
        assert!(engine.time_histogram().bucket_ms() > 1_000);
        std::fs::write(&log, "").unwrap();
        engine.poll_updates();
        assert!(engine.time_histogram().is_empty());
        assert_eq!(engine.time_histogram().untimed(), 0);
        append(
            &log,
            "2026-09-20 08:00:00 WARN fresh\n2026-09-20 08:00:05 INFO fresh\n",
        );
        engine.poll_updates();
        // The strip asks again every frame: a rewritten stream is timed anew.
        engine.request_timeline();
        assert_matches_caches(&engine);
        let histogram = engine.time_histogram();
        assert_eq!(
            histogram.bucket_ms(),
            1_000,
            "a full reset starts over at 1 s"
        );
        assert_eq!(histogram.timed(), 2);
        assert_eq!(histogram.len(), 6);
    }

    #[test]
    fn a_background_timing_matches_the_synchronous_histogram() {
        let (_dir, log) = timed_log(40_000);
        let mut sync = TailEngine::open(&log).unwrap();
        sync.request_timeline();

        // The level scan starts on open; the timeline's timing preempts it, finishes first,
        // and the levels resume: the histogram fills as the second cache catches up.
        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        assert_eq!(bg.scan_progress().map(|p| p.0), Some(ScanKind::Levels));
        bg.request_timeline();
        assert_eq!(bg.scan_progress().map(|p| p.0), Some(ScanKind::Timestamps));
        wait_for_jobs(&mut bg);
        assert!(bg.timestamps_complete() && bg.levels_complete());
        assert_matches_caches(&bg);
        assert_eq!(bg.time_histogram(), sync.time_histogram());
    }

    #[test]
    fn a_timing_scan_preempted_and_resumed_matches_too() {
        let (_dir, log) = timed_log(150_000);
        let mut sync = TailEngine::open(&log).unwrap();
        sync.request_timeline();

        let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        wait_for_jobs(&mut bg); // the level scan
        bg.request_timeline();
        let started = std::time::Instant::now();
        while bg.histogram_lines() == 0 && started.elapsed().as_secs() < 30 {
            bg.poll_updates();
            if bg.scan_progress().is_none() {
                break;
            }
        }
        // A filter takes over; the timing resumes from its prefix afterwards.
        bg.set_include_filter("ERROR");
        assert_matches_caches(&bg);
        wait_for_jobs(&mut bg);
        assert!(bg.timestamps_complete());
        assert_matches_caches(&bg);
        assert_eq!(bg.time_histogram(), sync.time_histogram());
    }
}

mod time_delta {
    use super::wait_for_jobs;
    use fasttail::scan_job::ScanKind;
    use fasttail::tail_engine::{TailEngine, TimeDelta};
    use std::io::Write;

    /// Entries with a stack trace under the ERROR, and payment lines 4 s apart with other
    /// entries between them.
    const LOG: &[&str] = &[
        "2026-09-18T14:02:05.100Z INFO payment started",
        "2026-09-18T14:02:05.225Z DEBUG cache lookup",
        "2026-09-18T14:02:07.580Z ERROR gateway timeout",
        "    at Gateway.call(Gateway.java:10)",
        "    at Gateway.retry(Gateway.java:20)",
        "2026-09-18T14:02:08.000Z INFO retrying",
        "2026-09-18T14:02:09.100Z INFO payment done",
    ];

    fn open(lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf, TailEngine) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("delta.log");
        super::write_lines(&log, lines);
        let mut engine = TailEngine::open(&log).unwrap();
        engine.ensure_timestamps();
        (dir, log, engine)
    }

    fn deltas(engine: &TailEngine) -> Vec<TimeDelta> {
        (0..engine.visible_line_count())
            .map(|row| engine.row_time_delta(row))
            .collect()
    }

    #[test]
    fn time_delta_i18n_keys_are_translated_everywhere() {
        use fasttail::i18n::{t, Language};
        let keys = [
            "tip_time_delta",
            "time_delta_unusable",
            "show_time_delta",
            "time_delta_gap",
            "time_anchor_set",
            "time_anchor_clear",
            "selection_elapsed",
            "selection_elapsed_tip",
        ];
        for lang in Language::ALL {
            for key in keys {
                let text = t(*lang, key);
                assert!(!text.is_empty() && text != "Unknown", "{key} for {lang:?}");
                if *lang != Language::En {
                    assert_ne!(text, t(Language::En, key), "{key} untranslated in {lang:?}");
                }
            }
            let status = t(*lang, "selection_elapsed");
            assert!(
                status.contains("{delta}") && status.contains("{n}"),
                "{lang:?}"
            );
        }
    }

    #[test]
    fn previous_row_deltas_with_blank_continuations() {
        let (_dir, _log, engine) = open(LOG);
        assert_eq!(
            deltas(&engine),
            vec![
                TimeDelta::Blank, // first row
                TimeDelta::Millis(125),
                TimeDelta::Millis(2_355),
                TimeDelta::Blank, // stack frame
                TimeDelta::Blank, // stack frame
                // From the stack frame above, which carries the ERROR's time.
                TimeDelta::Millis(420),
                TimeDelta::Millis(1_100),
            ]
        );
        assert_eq!(fasttail::timestamp::format_delta(2_355), "+2.355");
    }

    #[test]
    fn the_delta_follows_the_include_filter() {
        let (_dir, _log, mut engine) = open(LOG);
        engine.set_include_filter("payment");
        assert_eq!(engine.visible_line_count(), 2);
        assert_eq!(
            deltas(&engine),
            vec![TimeDelta::Blank, TimeDelta::Millis(4_000)]
        );
    }

    #[test]
    fn the_anchor_gives_signed_values_and_survives_a_filter() {
        let (_dir, _log, mut engine) = open(LOG);
        engine.toggle_time_anchor(1);
        assert_eq!(engine.time_anchor(), Some(1));
        assert_eq!(
            deltas(&engine),
            vec![
                TimeDelta::Millis(-125),
                TimeDelta::Anchor,
                TimeDelta::Millis(2_355),
                TimeDelta::Blank,
                TimeDelta::Blank,
                TimeDelta::Millis(2_775),
                TimeDelta::Millis(3_875),
            ]
        );

        // The anchor row is hidden by the filter: the values still count from it.
        engine.set_include_filter("payment");
        assert_eq!(engine.time_anchor(), Some(1));
        assert_eq!(
            deltas(&engine),
            vec![TimeDelta::Millis(-125), TimeDelta::Millis(3_875)]
        );

        // Setting it again on the same line clears it.
        engine.toggle_time_anchor(1);
        assert_eq!(engine.time_anchor(), None);
        engine.toggle_time_anchor(6);
        engine.clear_time_anchor();
        assert_eq!(engine.time_anchor(), None);
    }

    #[test]
    fn the_anchor_is_cleared_on_truncation() {
        let (_dir, log, mut engine) = open(LOG);
        engine.toggle_time_anchor(2);
        std::fs::File::create(&log)
            .unwrap()
            .write_all(b"2026-09-18T14:02:05.100Z INFO fresh\n")
            .unwrap();
        engine.poll_updates();
        assert_eq!(engine.total_lines(), 1);
        assert_eq!(engine.time_anchor(), None);
    }

    #[test]
    fn selection_elapsed_time_of_a_range_and_of_ctrl_a() {
        let (_dir, _log, mut engine) = open(LOG);
        assert_eq!(engine.selection_elapsed(), None, "nothing selected");
        engine.select_row(0);
        assert_eq!(engine.selection_elapsed(), None, "a single row");
        engine.extend_selection_to(5);
        assert_eq!(engine.selection_elapsed(), Some((2_900, 6)));
        // A continuation line at the end counts with its entry's time.
        engine.select_row(0);
        engine.extend_selection_to(3);
        assert_eq!(engine.selection_elapsed(), Some((2_480, 4)));

        engine.set_include_filter("payment");
        engine.select_all_visible();
        assert_eq!(engine.selection_elapsed(), Some((4_000, 2)));
    }

    #[test]
    fn no_elapsed_time_without_usable_timestamps() {
        let (_dir, _log, mut engine) = open(&["alpha", "beta", "gamma"]);
        engine.select_all_visible();
        assert!(!engine.timestamps_usable());
        assert_eq!(engine.selection_elapsed(), None);
        assert!(deltas(&engine).iter().all(|d| *d == TimeDelta::Blank));
    }

    #[test]
    fn the_zone_suffix_is_not_applied() {
        // 14:02:05 at +02:00 and 14:02:06 in UTC: one second apart on the printed clock.
        let (_dir, _log, engine) = open(&[
            "2026-09-18T14:02:05.100+02:00 INFO local",
            "2026-09-18T14:02:06.100Z INFO utc",
        ]);
        assert_eq!(engine.row_time_delta(1), TimeDelta::Millis(1_000));
    }

    #[test]
    fn rows_not_timed_yet_are_pending_while_a_background_scan_runs() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("big.log");
        {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&log).unwrap());
            for i in 0..50_000 {
                writeln!(
                    f,
                    "2026-09-19T10:{:02}:{:02}.{:03}Z INFO req={i}",
                    (i / 600) % 60,
                    (i / 10) % 60,
                    (i % 10) * 100
                )
                .unwrap();
            }
        }
        // Threshold 0: the timing asked by the column always goes to a background scan.
        let mut engine = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
        wait_for_jobs(&mut engine);
        let last = engine.visible_line_count() - 1;
        assert_eq!(engine.row_time_delta(last), TimeDelta::Pending);
        engine.want_timestamps();
        assert_eq!(
            engine.scan_progress().map(|(kind, _, _)| kind),
            Some(ScanKind::Timestamps),
            "the column never times the file on the calling thread"
        );
        wait_for_jobs(&mut engine);
        assert!(engine.timestamps_complete());
        assert_eq!(engine.row_time_delta(last), TimeDelta::Millis(100));
    }

    /// Draws the stream in a dock with the time delta column on, as the app does.
    fn render(engines: &mut Vec<TailEngine>, prefs: &mut fasttail::ui::dock::TimeDeltaPrefs) {
        use fasttail::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
        let path = engines[0].path.clone();
        let mut open_files = vec![path.clone()];
        let mut dock = egui_dock::DockState::new(vec![FastTailTab::LogStream(path.clone())]);
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let dock_ctx = DockContext {
                engines,
                open_files: &mut open_files,
                theme: &mut fasttail::theme::CyberTheme::Tron,
                language: &mut fasttail::i18n::Language::En,
                global_rules: &mut Vec::new(),
                screensaver_enabled: &mut false,
                screensaver_timeout_mins: &mut 5,
                telemetry_enabled: &mut false,
                sound_enabled: &mut false,
                borderless: &mut false,
                show_line_numbers: &mut true,
                font_size: &mut 13.0,
                level_colors: &mut true,
                size_unit: &mut fasttail::tail_engine::SizeUnit::Bytes,
                search_history: &mut Vec::new(),
                tab_closed: &mut false,
                test_screensaver: &mut false,
                language_auto: &mut false,
                lock_enabled: &mut false,
                lock_pin: &mut String::new(),
                lock_now: &mut false,
                quick_labels: &mut Vec::new(),
                labels_changed: &mut false,
                external_tools: &mut Vec::new(),
                tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
                focused_stream: Some(path.clone()),
                search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
                time_delta: prefs,
                find_all: &mut fasttail::find_all::FindAllSession::default(),
                filter_presets: &mut Vec::new(),
                preset_events: &mut Default::default(),
            };
            let mut viewer = FastTailTabViewer { ctx: dock_ctx };
            egui_dock::DockArea::new(&mut dock).show_inside(ui, &mut viewer);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn the_column_times_the_stream_when_shown() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("ui.log");
        super::write_lines(&log, LOG);
        let mut engines = vec![TailEngine::open(&log).unwrap()];
        let mut prefs = fasttail::ui::dock::TimeDeltaPrefs::default();
        assert!(!prefs.show, "off by default");
        assert_eq!(prefs.gap_ms, 1000);

        // Off: nothing asks for the timing.
        render(&mut engines, &mut prefs);
        assert_eq!(engines[0].row_time_delta(1), TimeDelta::Pending);

        // On, extended and wrapped rows, with and without an anchor.
        prefs.show = true;
        render(&mut engines, &mut prefs);
        assert!(
            engines[0].timestamps_complete(),
            "a small file is timed at once"
        );
        assert_eq!(engines[0].row_time_delta(1), TimeDelta::Millis(125));
        engines[0].toggle_time_anchor(2);
        engines[0].wrap_lines = true;
        render(&mut engines, &mut prefs);
        render(&mut engines, &mut prefs);
        assert_eq!(engines[0].time_anchor(), Some(2));
    }

    #[test]
    fn a_selection_times_the_stream_for_its_elapsed_time() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("sel.log");
        super::write_lines(&log, LOG);
        let mut engines = vec![TailEngine::open(&log).unwrap()];
        engines[0].select_row(0);
        engines[0].extend_selection_to(2);
        assert_eq!(engines[0].selection_elapsed(), None, "not timed yet");
        render(
            &mut engines,
            &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
        );
        assert_eq!(engines[0].selection_elapsed(), Some((2_480, 3)));
    }
}

mod search_all_streams {
    use fasttail::find_all::{FindAllSession, FindState};
    use fasttail::i18n::{t, Language};
    use fasttail::tail_engine::TailEngine;
    use fasttail::ui::dock::FastTailTab;
    use fasttail::ui::find_results::{
        apply_find_jump, consume_find_all_shortcut, open_find_results_tab, results_list_id,
        without_find_results,
    };
    use fasttail::ui::hit_list::GroupedHitList;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const KEYS: [&str; 18] = [
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
    ];

    #[test]
    fn i18n_keys_are_translated_everywhere() {
        for lang in Language::ALL {
            for key in KEYS {
                let text = t(*lang, key);
                assert!(!text.is_empty() && text != "Unknown", "{key} for {lang:?}");
                if *lang != Language::En {
                    assert_ne!(text, t(Language::En, key), "{key} untranslated in {lang:?}");
                }
            }
            let summary = t(*lang, "find_all_summary");
            for p in ["{hits}", "{streams}", "{total}"] {
                assert!(summary.contains(p), "{lang:?} summary lacks {p}");
            }
            assert!(t(*lang, "find_all_group_count").contains("{n}"), "{lang:?}");
            assert!(t(*lang, "find_all_snapshot").contains("{age}"), "{lang:?}");
            let progress = t(*lang, "find_all_progress");
            assert!(progress.contains("{running}") && progress.contains("{queued}"));
        }
    }

    fn dock_context<'a>(
        engines: &'a mut Vec<TailEngine>,
        open_files: &'a mut Vec<PathBuf>,
        session: &'a mut FindAllSession,
        focused_stream: Option<PathBuf>,
    ) -> fasttail::ui::dock::DockContext<'a> {
        // The settings the dock reads but these tests never look at.
        fn leak<T>(v: T) -> &'static mut T {
            Box::leak(Box::new(v))
        }
        fasttail::ui::dock::DockContext {
            engines,
            open_files,
            theme: leak(fasttail::theme::CyberTheme::Tron),
            language: leak(Language::En),
            global_rules: leak(Vec::new()),
            screensaver_enabled: leak(false),
            screensaver_timeout_mins: leak(5),
            telemetry_enabled: leak(false),
            sound_enabled: leak(false),
            borderless: leak(false),
            show_line_numbers: leak(true),
            font_size: leak(13.0),
            level_colors: leak(true),
            size_unit: leak(fasttail::tail_engine::SizeUnit::Bytes),
            search_history: leak(Vec::new()),
            tab_closed: leak(false),
            test_screensaver: leak(false),
            language_auto: leak(false),
            lock_enabled: leak(false),
            lock_pin: leak(String::new()),
            lock_now: leak(false),
            quick_labels: leak(Vec::new()),
            labels_changed: leak(false),
            external_tools: leak(Vec::new()),
            tool_runner: leak(fasttail::external_tools::ToolRunner::default()),
            focused_stream,
            search_view: leak(fasttail::ui::dock::SearchViewPrefs::default()),
            time_delta: leak(fasttail::ui::dock::TimeDeltaPrefs::default()),
            find_all: session,
            filter_presets: leak(Vec::new()),
            preset_events: leak(Default::default()),
        }
    }

    /// Streams in one dock leaf, the Find results tab split below, drawn frame by frame
    /// the way the app does: Ctrl+Shift+F consumed first, the jump applied after the dock.
    struct Harness {
        engines: Vec<TailEngine>,
        open_files: Vec<PathBuf>,
        dock: egui_dock::DockState<FastTailTab>,
        session: FindAllSession,
        ctx: egui::Context,
        _dir: tempfile::TempDir,
    }

    impl Harness {
        fn new(files: &[(&str, String)]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut engines = Vec::new();
            for (name, text) in files {
                let path = dir.path().join(name);
                std::fs::write(&path, text).unwrap();
                let mut engine = TailEngine::open(&path).unwrap();
                engine.size_check_interval = Duration::ZERO;
                engines.push(engine);
            }
            let tabs = engines
                .iter()
                .map(|e| FastTailTab::LogStream(e.path.clone()))
                .collect();
            Self {
                open_files: engines.iter().map(|e| e.path.clone()).collect(),
                engines,
                dock: egui_dock::DockState::new(tabs),
                session: FindAllSession::with_limits(2, 1_000),
                ctx: egui::Context::default(),
                _dir: dir,
            }
        }

        fn focused_stream(&mut self) -> Option<PathBuf> {
            match self.dock.find_active_focused() {
                Some((_, FastTailTab::LogStream(p))) => Some(p.clone()),
                Some(_) => None,
                None => match self.dock.main_surface_mut().find_active() {
                    Some((_, FastTailTab::LogStream(p))) => Some(p.clone()),
                    _ => None,
                },
            }
        }

        /// One frame; `app_shortcut` consumes Ctrl+Shift+F before the dock as the app
        /// does. Returns whether it was consumed.
        fn frame(&mut self, events: Vec<egui::Event>, app_shortcut: bool) -> bool {
            let focused = self.focused_stream();
            for e in &mut self.engines {
                e.poll_updates();
            }
            self.session.poll(&self.engines);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut consumed = false;
            let mut out = self.ctx.run_ui(input, |ui| {
                if app_shortcut {
                    consumed = consume_find_all_shortcut(ui.ctx());
                }
                let dock_ctx = dock_context(
                    &mut self.engines,
                    &mut self.open_files,
                    &mut self.session,
                    focused.clone(),
                );
                let mut viewer = fasttail::ui::dock::FastTailTabViewer { ctx: dock_ctx };
                egui_dock::DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
            });
            out.textures_delta.clear();
            apply_find_jump(
                &mut self.session,
                &mut self.engines,
                &mut self.dock,
                Language::En,
            );
            consumed
        }

        fn click(&mut self, id: egui::Id) {
            let pos = self
                .ctx
                .read_response(id)
                .map(|r| r.rect.center())
                .expect("the row is on screen");
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(pos)], false);
            self.frame(vec![button(true)], false);
            self.frame(vec![button(false)], false);
            self.frame(Vec::new(), false);
        }

        fn run_to_end(&mut self) {
            let started = Instant::now();
            while self.session.is_active() {
                assert!(started.elapsed() < Duration::from_secs(30), "search hangs");
                std::thread::sleep(Duration::from_millis(2));
                self.session.poll(&self.engines);
            }
        }

        fn active_is_results(&mut self) -> bool {
            matches!(
                self.dock.find_active_focused(),
                Some((_, FastTailTab::FindResults))
            )
        }

        fn active_is_stream(&mut self, idx: usize) -> bool {
            let path = self.engines[idx].path.clone();
            matches!(
                self.dock.find_active_focused(),
                Some((_, FastTailTab::LogStream(p))) if *p == path
            )
        }
    }

    fn ctrl_shift_f() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        }
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn logs() -> Vec<(&'static str, String)> {
        let gateway: String = (0..40)
            .map(|i| {
                if i % 10 == 3 {
                    format!("INFO gateway req-7f3a step {i}\n")
                } else {
                    format!("INFO gateway other {i}\n")
                }
            })
            .collect();
        let payment: String = (0..300)
            .map(|i| match i {
                120 | 250 => format!("INFO payment req-7f3a charge {i}\n"),
                _ => format!("INFO payment old line {i}\n"),
            })
            .collect();
        vec![("gateway.log", gateway), ("payment.log", payment)]
    }

    #[test]
    fn ctrl_shift_f_does_not_trigger_the_stream_ctrl_f() {
        let mut h = Harness::new(&logs()[..1]);
        h.engines[0].search_query = "req-7f3a".into();
        h.frame(Vec::new(), true);
        let search_id = egui::Id::new("log_search_input").with(&h.engines[0].path);

        assert!(
            h.frame(vec![ctrl_shift_f()], true),
            "the app takes the shortcut"
        );
        h.frame(Vec::new(), true);
        assert!(
            !h.ctx.memory(|m| m.has_focus(search_id)),
            "the stream's search box must not take the keyboard"
        );

        // Why the app consumes it first: egui matches Ctrl+F logically, extra Shift
        // ignored, so without it the stream's Ctrl+F fires on Ctrl+Shift+F.
        h.frame(vec![ctrl_shift_f()], false);
        h.frame(Vec::new(), false);
        assert!(h.ctx.memory(|m| m.has_focus(search_id)));
    }

    #[test]
    fn the_tab_opens_below_and_is_not_saved_in_the_layout() {
        let mut h = Harness::new(&logs());
        open_find_results_tab(&mut h.dock);
        assert!(h.active_is_results());
        let before = h.dock.iter_all_tabs().count();
        assert_eq!(before, 3);
        // Focus back on the streams; a second request focuses the same tab.
        let streams = h
            .dock
            .find_tab(&FastTailTab::LogStream(h.engines[0].path.clone()))
            .unwrap();
        h.dock.set_focused_node_and_surface(streams.node_path());
        assert!(!h.active_is_results());
        open_find_results_tab(&mut h.dock);
        assert_eq!(h.dock.iter_all_tabs().count(), before);
        assert!(h.active_is_results());

        let saved = without_find_results(&h.dock);
        let tabs: Vec<FastTailTab> = saved.iter_all_tabs().map(|(_, t)| t.clone()).collect();
        assert_eq!(tabs.len(), 2);
        assert!(tabs.iter().all(|t| matches!(t, FastTailTab::LogStream(_))));
        assert!(
            h.dock.find_tab(&FastTailTab::FindResults).is_some(),
            "the live dock keeps it"
        );
    }

    #[test]
    fn clicking_a_result_activates_the_stream_without_touching_its_search() {
        let mut h = Harness::new(&logs());
        // payment.log is the background tab and has a search of its own.
        h.engines[1].search_query = "old line".into();
        h.engines[1].update_search("old line");
        let own_matches = h.engines[1].search_matches.clone();
        open_find_results_tab(&mut h.dock);
        h.session.input = "REQ-7F3A".into();
        h.session.start(&h.engines);
        h.run_to_end();
        assert_eq!(h.session.groups[0].hits, vec![3, 13, 23, 33]);
        assert_eq!(h.session.groups[1].hits, vec![120, 250]);
        assert_eq!(h.session.total_hits(), 6);
        h.frame(Vec::new(), false);
        h.frame(Vec::new(), false);

        let list = results_list_id();
        h.click(GroupedHitList::hit_id(list, 1, 1));
        assert!(h.active_is_stream(1));
        let payment = &h.engines[1];
        assert!(
            payment.pending_jump.is_none(),
            "the stream centred the line"
        );
        assert!(!payment.follow_tail);
        assert!(payment.is_selected(250));
        assert_eq!(payment.search_query, "old line");
        assert_eq!(payment.last_searched_query, "old line");
        assert_eq!(payment.search_matches, own_matches);
        assert!(
            !h.ctx.memory(|m| m.has_focus(list)),
            "the stream takes the keyboard back"
        );

        // A header click collapses its group: the next header follows it directly.
        open_find_results_tab(&mut h.dock);
        h.frame(Vec::new(), false);
        h.click(GroupedHitList::header_id(list, 0));
        assert!(h.session.groups[0].collapsed);
        h.frame(Vec::new(), false);
        let rect = |h: &Harness, id| h.ctx.read_response(id).unwrap().rect;
        let header0 = rect(&h, GroupedHitList::header_id(list, 0));
        let header1 = rect(&h, GroupedHitList::header_id(list, 1));
        // Only the item spacing between them, no hit row.
        let gap = header1.top() - header0.bottom();
        assert!(gap >= 0.0 && gap < header0.height() / 2.0, "gap {gap}");
    }

    #[test]
    fn keyboard_walks_across_groups_and_enter_jumps() {
        let mut h = Harness::new(&logs());
        open_find_results_tab(&mut h.dock);
        h.session.input = "req-7f3a".into();
        h.session.start(&h.engines);
        h.run_to_end();
        h.frame(Vec::new(), false);
        let list = results_list_id();
        // A click on the first header focuses the list (and toggles it: twice).
        h.click(GroupedHitList::header_id(list, 0));
        h.click(GroupedHitList::header_id(list, 0));
        assert!(!h.session.groups[0].collapsed);
        assert!(h.ctx.memory(|m| m.has_focus(list)));
        // Rows: header 0, its 4 hits, header 1, its 2 hits. Six Downs land on the first
        // hit of the second group.
        for _ in 0..6 {
            h.frame(vec![key(egui::Key::ArrowDown)], false);
        }
        h.frame(vec![key(egui::Key::Enter)], false);
        h.frame(Vec::new(), false);
        assert!(h.engines[1].is_selected(120));
        assert!(h.active_is_stream(1));
    }

    #[test]
    fn a_result_of_a_truncated_stream_does_not_jump() {
        let mut h = Harness::new(&logs());
        open_find_results_tab(&mut h.dock);
        h.session.input = "req-7f3a".into();
        h.session.start(&h.engines);
        h.run_to_end();
        h.frame(Vec::new(), false);
        std::fs::write(&h.engines[0].path, "rewritten\n").unwrap();
        h.frame(Vec::new(), false);
        h.frame(Vec::new(), false);
        assert_eq!(h.session.groups[0].state, FindState::Stale);
        h.click(GroupedHitList::hit_id(results_list_id(), 0, 1));
        assert!(h.active_is_results(), "the view does not move");
        assert!(h.engines[0].pending_jump.is_none());
        assert!(!h.engines[0].is_selected(13));
    }

    #[test]
    fn closing_the_tab_cancels_the_search() {
        use egui_dock::TabViewer;
        let big: String = (0..200_000)
            .map(|i| format!("padding padding padding {i} hit\n"))
            .collect();
        let mut h = Harness::new(&[("big.log", big)]);
        open_find_results_tab(&mut h.dock);
        h.session.input = "hit".into();
        h.session.start(&h.engines);
        assert!(h.session.is_active());
        let mut viewer = fasttail::ui::dock::FastTailTabViewer {
            ctx: dock_context(&mut h.engines, &mut h.open_files, &mut h.session, None),
        };
        viewer.on_close(&mut FastTailTab::FindResults);
        drop(viewer);
        assert!(!h.session.is_active());
        assert!(h.session.groups.is_empty());
        assert_eq!(h.engines.len(), 1, "the stream stays open");
    }
}

mod filter_terms_and_presets {
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
        let (_dir, path) = log_with(
            "WARN payment slow\nERROR payment healthcheck\nINFO payment ok\nERROR other\n",
        );
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
}

mod stdin_stream {
    use fasttail::config::FastTailConfig;
    use fasttail::stdin_source::{is_stdin_path, InputState, STDIN_PATH};
    use fasttail::ui::app::StdinOptions;
    use fasttail::ui::FastTailApp;
    use std::io::Read;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A producer fed through a channel: each message is one write, dropping the sender
    /// is the end of input.
    struct Producer(mpsc::Receiver<Vec<u8>>, Vec<u8>);

    impl Read for Producer {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.1.is_empty() {
                match self.0.recv() {
                    Ok(data) => self.1 = data,
                    Err(_) => return Ok(0),
                }
            }
            let n = self.1.len().min(buf.len());
            buf[..n].copy_from_slice(&self.1[..n]);
            self.1.drain(..n);
            Ok(n)
        }
    }

    fn producer() -> (mpsc::Sender<Vec<u8>>, Producer) {
        let (tx, rx) = mpsc::channel();
        (tx, Producer(rx, Vec::new()))
    }

    fn app_with_spool(dir: &Path) -> FastTailApp {
        let config = FastTailConfig {
            spool_dir: Some(dir.to_path_buf()),
            ..Default::default()
        };
        FastTailApp::from_config(config)
    }

    fn frame(app: &mut FastTailApp, ctx: &egui::Context) {
        let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
        out.textures_delta.clear();
    }

    fn frames_until(
        app: &mut FastTailApp,
        ctx: &egui::Context,
        what: &str,
        mut cond: impl FnMut(&FastTailApp) -> bool,
    ) {
        let start = Instant::now();
        loop {
            frame(app, ctx);
            if cond(app) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn stdin_engine(app: &FastTailApp) -> Option<&fasttail::tail_engine::TailEngine> {
        app.engines.iter().find(|e| e.is_stdin())
    }

    #[test]
    fn find_results_search_standard_input_under_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_spool(dir.path());
        let (tx, input) = producer();
        app.start_stdin(input, true, StdinOptions::default());
        let ctx = egui::Context::default();
        tx.send(
            b"INFO a
ERROR b
INFO c
ERROR d
"
            .to_vec(),
        )
        .unwrap();
        frames_until(&mut app, &ctx, "the lines", |app| {
            stdin_engine(app).is_some_and(|e| e.total_lines() == 4)
        });

        app.find_all.input = "error".into();
        app.find_all.start(&app.engines);
        frames_until(&mut app, &ctx, "the search", |app| {
            !app.find_all.is_active()
        });
        let group = &app.find_all.groups[0];
        assert_eq!(group.name, fasttail::stdin_source::STDIN_TITLE);
        assert_eq!(group.hits, vec![1, 3]);
        drop(tx);
    }

    #[test]
    fn dash_opens_at_once_with_the_cli_filter_and_is_never_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut log, b"file line\n").unwrap();
        let mut app = app_with_spool(dir.path());
        app.open_log_file(log.path().to_path_buf());
        let (tx, input) = producer();
        let options = StdinOptions {
            filter: Some("ERROR".to_string()),
            ..Default::default()
        };
        app.start_stdin(input, true, options);
        // With `-` the stream exists before any byte arrives.
        let engine = stdin_engine(&app).expect("stdin stream opened");
        assert!(is_stdin_path(&engine.path));
        assert_eq!(engine.include_filter(), "ERROR");
        assert!(engine.follow_tail);

        let ctx = egui::Context::default();
        tx.send(b"INFO a\nERROR b\nINFO c\nERROR d\n".to_vec())
            .unwrap();
        frames_until(&mut app, &ctx, "the lines", |app| {
            stdin_engine(app).is_some_and(|e| e.total_lines() == 4)
        });
        let engine = app.engines.iter_mut().find(|e| e.is_stdin()).unwrap();
        engine.set_bookmarks(vec![1]);
        engine.bookmarks_dirty = true;
        frame(&mut app, &ctx);
        app.save_dock_layout();

        // Nothing of it reaches the workspace or the recent files.
        let cfg = &app.config;
        assert!(!cfg.open_files.iter().any(|p| is_stdin_path(p)));
        assert!(cfg.open_files.iter().any(|p| p == log.path()));
        assert!(!cfg.recent_files.iter().any(|p| is_stdin_path(p)));
        assert!(!cfg.bookmarks.iter().any(|(p, _)| is_stdin_path(p)));
        let layout = cfg.dock_layout.clone().unwrap_or_default();
        assert!(!layout.contains(STDIN_PATH), "{layout}");
        let mut ini = Vec::new();
        cfg.to_ini().write_to(&mut ini).unwrap();
        assert!(!String::from_utf8_lossy(&ini).contains(STDIN_PATH));

        // Nor a session file, and the save says so.
        let session = app.capture_session();
        assert_eq!(session.streams.len(), 1);
        let file = dir.path().join("work.fasttail-session.ini");
        app.save_session_as(file.clone()).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains(STDIN_PATH));
        assert!(app.save_notice.is_some());

        // Loading a session keeps the stream: standard input cannot be read again.
        app.load_session_file(file, true);
        assert!(stdin_engine(&app).is_some());
        assert_eq!(app.engines.len(), 2);

        drop(tx);
        frames_until(&mut app, &ctx, "the end of input", |app| {
            stdin_engine(app)
                .and_then(|e| e.stdin.as_ref())
                .is_some_and(|s| s.state() == InputState::Ended)
        });
        assert_eq!(stdin_engine(&app).unwrap().total_lines(), 4);
    }

    #[test]
    fn piped_without_dash_opens_on_the_first_byte() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_spool(dir.path());
        let ctx = egui::Context::default();
        let (tx, input) = producer();
        app.start_stdin(input, false, StdinOptions::default());
        frame(&mut app, &ctx);
        assert!(stdin_engine(&app).is_none(), "no tab before the first byte");
        assert!(app.pending_stdin.is_some());

        tx.send(b"hello\n".to_vec()).unwrap();
        frames_until(&mut app, &ctx, "the stream", |app| {
            stdin_engine(app).is_some_and(|e| e.total_lines() == 1)
        });
        assert!(app.pending_stdin.is_none());
        let spool = stdin_engine(&app)
            .and_then(|e| e.stdin.as_ref())
            .map(|s| s.spool_path().to_path_buf())
            .unwrap();
        assert!(spool.is_file());
        // Closing the stream deletes its spool once the copier lets go of it.
        app.engines.retain(|e| !e.is_stdin());
        drop(tx);
        let start = Instant::now();
        while spool.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "spool left behind"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn piped_without_dash_that_ends_empty_opens_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_spool(dir.path());
        let ctx = egui::Context::default();
        let (tx, input) = producer();
        app.start_stdin(input, false, StdinOptions::default());
        drop(tx);
        frames_until(&mut app, &ctx, "the empty end", |app| {
            app.pending_stdin.is_none()
        });
        assert!(stdin_engine(&app).is_none());
        assert_eq!(app.dock_state.iter_all_tabs().count(), 0);
    }
}
