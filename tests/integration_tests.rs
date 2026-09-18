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
    assert_eq!(Language::from_code("de-DE"), Language::En); // fallback
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
        "screensaver",
        "screensaver_timeout",
        "screensaver_zero_off",
        "ext_tools",
        "ext_tools_placeholders",
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
        "about_git_tag",
        "about_build_date",
        "about_author",
        "about_repo",
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
        "renderer_note",
        "renderer_tip",
        "export_visible",
        "export_matches",
        "export_tip",
        "help_desc_select",
        "help_desc_copy",
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
        "md_too_large",
    ];

    for lang in &[
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
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
    config.language = Language::Fr;
    config.screensaver_enabled = false;
    config.screensaver_timeout_mins = 15;
    config.telemetry_enabled = false;
    config.sound_enabled = true;
    config.borderless = true;
    config.show_line_numbers = false;
    config.font_size = 16.5;
    config.size_unit = SizeUnit::Hex;
    config.baretail_import = false;
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
    assert_eq!(loaded.screensaver_enabled, false);
    assert_eq!(loaded.screensaver_timeout_mins, 15);
    assert_eq!(loaded.telemetry_enabled, false);
    assert_eq!(loaded.sound_enabled, true);
    assert_eq!(loaded.borderless, true);
    assert_eq!(loaded.show_line_numbers, false);
    assert_eq!(loaded.font_size, 16.5);
    assert_eq!(loaded.size_unit, SizeUnit::Hex);
    assert_eq!(loaded.baretail_import, false);
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
    assert_eq!(APP_VERSION, "0.3.0");

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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
            quick_labels: &mut Vec::new(),
            labels_changed: &mut false,
            external_tools: &mut Vec::new(),
            tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
            focused_stream: focused_stream.clone(),
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
                quick_labels: &mut Vec::new(),
                labels_changed: &mut false,
                external_tools: &mut Vec::new(),
                tool_runner: &mut fasttail::external_tools::ToolRunner::default(),
                focused_stream: focused_stream.clone(),
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
    assert_eq!(engine.include_filter, "error");
    assert_eq!(engine.exclude_filter, "healthcheck");
    assert!(!engine.follow_tail);
    assert_eq!(engine.visible_line_count(), 1);

    // Opening the same path again from the command line does not duplicate the stream.
    app.apply_cli(&cli);
    assert_eq!(app.engines.len(), 1);
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
            hidden: false
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
            hidden: true
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
    use fasttail::tail_engine::MARKDOWN_MAX_BYTES;
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
    assert!(engine.markdown_too_large());
    assert_eq!(engine.markdown_text(), "");
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
    assert_eq!(engine.include_filter, "ERROR", "filters survive the switch");
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
