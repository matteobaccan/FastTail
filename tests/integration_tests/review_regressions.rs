// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;

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
        auto_bookmark: false,
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
            filter_presets: &mut Vec::new(),
            preset_events: &mut Default::default(),
            palette_action: None,
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
                filter_presets: &mut Vec::new(),
                preset_events: &mut Default::default(),
                palette_action: None,
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
    let picker = app
        .archive_picker
        .take()
        .expect("the entry picker is shown");
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

#[test]
fn test_7z_archives_offer_their_entries_like_zip_bundles() {
    use fasttail::ui::FastTailApp;
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..FastTailConfig::default()
    };
    let mut app = FastTailApp::from_config(config);

    // Two logs: the picker lists both, each chosen entry is its own titled stream.
    let logs = dir.path().join("logs.7z");
    write_7z(
        &logs,
        &[("server.log", b"started\n"), ("worker.log", b"started\n")],
    );
    app.open_log_file(logs.clone());
    assert!(app.engines.is_empty(), "nothing opens before a choice");
    let picker = app
        .archive_picker
        .take()
        .expect("the entry picker is shown");
    let names: Vec<&str> = picker.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["server.log", "worker.log"]);
    assert!(!picker.partial);
    for name in names {
        app.open_log_file(fasttail::compressed::entry_path(&logs, name));
    }
    assert_eq!(app.engines.len(), 2);
    let titles: Vec<String> = app
        .engines
        .iter()
        .map(|e| e.compressed.as_ref().unwrap().title())
        .collect();
    assert_eq!(titles, ["logs.7z › server.log", "logs.7z › worker.log"]);

    // A single log opens directly, whatever the extension.
    let single = dir.path().join("single.bin");
    write_7z(&single, &[("app.log", b"one\ntwo\n")]);
    app.open_log_file(single.clone());
    assert!(app.archive_picker.is_none());
    assert_eq!(app.engines.len(), 3);
    assert_eq!(
        app.engines[2].path,
        fasttail::compressed::entry_path(&single, "app.log")
    );

    // A damaged 7z says why instead of opening as text.
    let damaged = dir.path().join("damaged.7z");
    std::fs::write(&damaged, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4]).unwrap();
    app.open_log_file(damaged);
    assert_eq!(app.engines.len(), 3);
    assert!(app.open_notice.is_some());
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
