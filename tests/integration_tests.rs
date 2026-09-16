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
    for theme in &[CyberTheme::Tron, CyberTheme::Matrix, CyberTheme::Blade] {
        let bg = theme.bg_color();
        let border = theme.border_color();
        let accent = theme.accent_color();
        assert_ne!(bg, border);
        assert_ne!(accent, bg);
    }
}

#[test]
fn test_i18n_translations_and_fallback() {
    for lang in &[Language::En, Language::It, Language::Fr, Language::Es, Language::Zh] {
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
    screensaver.check_inactivity(0, true);
    assert!(!screensaver.is_active);

    // Simulate idle by setting last_input_time in past
    screensaver.last_input_time = std::time::Instant::now() - Duration::from_secs(601);
    screensaver.check_inactivity(10, true);
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
    assert!(deserialized.highlight_rules.iter().any(|r| r.pattern == "CRITICAL"));
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
    assert_eq!(engine.get_line(1).as_deref(), Some("Line 2: Ignored while paused"));
}

#[test]
fn test_new_i18n_keys() {
    for lang in &[Language::En, Language::It, Language::Fr, Language::Es, Language::Zh] {
        assert_ne!(t(*lang, "borderless"), "Unknown");
        assert_ne!(t(*lang, "show_lines"), "Unknown");
        assert_ne!(t(*lang, "monitor_on"), "Unknown");
        assert_ne!(t(*lang, "monitor_off"), "Unknown");
    }
}

#[test]
fn test_dock_state_serialization_and_restore() {
    use egui_dock::DockState;
    use fasttail::ui::dock::FastTailTab;

    let dock_state = DockState::new(vec![FastTailTab::LogStream(0), FastTailTab::Settings]);
    let ron_str = ron::to_string(&dock_state).expect("serialize dock state with ron");
    assert!(!ron_str.is_empty());

    let restored: DockState<FastTailTab> = ron::from_str(&ron_str).expect("deserialize dock state with ron");
    assert!(restored.find_tab(&FastTailTab::Settings).is_some());
    assert!(restored.find_tab(&FastTailTab::LogStream(0)).is_some());

    let mut config = FastTailConfig::default();
    config.dock_layout = Some(ron_str);
    let toml_str = toml::to_string(&config).expect("serialize config with dock layout");
    let restored_cfg: FastTailConfig = toml::from_str(&toml_str).expect("deserialize config with dock layout");
    assert!(restored_cfg.dock_layout.is_some());
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
    let mut f = std::fs::OpenOptions::new().write(true).open(tmp.path()).unwrap();
    writeln!(f, "First line after empty").unwrap();
    f.flush().unwrap();

    engine.poll_updates();
    assert_eq!(engine.total_lines(), 1);
    assert_eq!(engine.get_line(0).as_deref(), Some("First line after empty"));
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

    // Release mmap before truncating (Windows NTFS requirement for file shrinking)
    engine.release_mmap();
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

    let style1 = engine.match_highlight("CRITICAL ERROR in payment gateway").unwrap();
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

    let style2 = engine.match_highlight("CRITICAL ERROR in payment gateway").unwrap();
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
        let visuals = ctx.style().visuals.clone();
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
        "tip_view_mode",
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
    ];

    for lang in &[Language::En, Language::It, Language::Fr, Language::Es, Language::Zh] {
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
    assert!(config.highlight_rules.is_empty(), "Filters must start empty by default");

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
        0xDE, 0xAD, 0xBE, 0xEF,                         // ....
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
    tmp.write_all(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A]).unwrap();
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
    bin_file.write_all(&[0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00]).unwrap();
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
    f_ansi.write_all(&[b'C', b'a', b'f', 0xE9, b'\n', b'O', b'k', b'\n']).unwrap();
    f_ansi.flush().unwrap();
    let mut engine_ansi = TailEngine::open(f_ansi.path()).unwrap();
    engine_ansi.set_encoding(FileEncoding::Ansi);
    assert_eq!(engine_ansi.total_lines(), 2);
    assert_eq!(engine_ansi.get_line(0).as_deref(), Some("Café"));
    assert_eq!(engine_ansi.get_line(1).as_deref(), Some("Ok"));

    // 3. UTF-8 (with BOM)
    let mut f_utf8 = NamedTempFile::new().unwrap();
    f_utf8.write_all(&[0xEF, 0xBB, 0xBF]).unwrap(); // UTF-8 BOM
    f_utf8.write_all("Prima riga UTF-8\nSeconda riga 🚀\n".as_bytes()).unwrap();
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
    assert_eq!(engine_utf16le.get_line(0).as_deref(), Some("Unicode LE Line 1"));
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
    assert_eq!(engine_utf16be.get_line(0).as_deref(), Some("Unicode BE Line 1"));
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
    let rule_b = HighlightRule::with_style("Database", [0, 0, 255], [255, 255, 0], false, false, true);

    // Initial order: [Rule A, Rule B].
    // Both match "2026-09-16 [CRITICAL] Database connection failed".
    // Evaluation order must be top-down and STOP at the first match!
    engine.set_highlight_rules(vec![rule_a.clone(), rule_b.clone()]);
    let style_first = engine.match_highlight("2026-09-16 [CRITICAL] Database connection failed").unwrap();
    assert_eq!(style_first.fg, egui::Color32::from_rgb(255, 0, 0));
    assert!(style_first.bold, "Rule A must apply bold");
    assert!(!style_first.italic, "Rule A is not italic");

    // Reorder: swap rules to [Rule B, Rule A]
    let mut reordered_rules = vec![rule_a, rule_b];
    reordered_rules.swap(0, 1);
    engine.set_highlight_rules(reordered_rules);

    // Now Rule B comes first, so Rule B must win!
    let style_reordered = engine.match_highlight("2026-09-16 [CRITICAL] Database connection failed").unwrap();
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
    assert_eq!(config.recent_files[0], PathBuf::from("C:\\logs\\app_19.log"));

    // Serialize and deserialize
    let toml_str = toml::to_string(&config).expect("serialize config");
    let deserialized: FastTailConfig = toml::from_str(&toml_str).expect("deserialize config");
    assert_eq!(deserialized.font_size, 18.0);
    assert_eq!(deserialized.recent_files.len(), 15);
    assert_eq!(deserialized.recent_files[0], PathBuf::from("C:\\logs\\app_19.log"));
}




