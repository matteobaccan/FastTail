// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::session::SESSION_SUFFIX;
use fasttail::tail_engine::{TailEngine, TimeDelta};
use fasttail::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
use fasttail::ui::FastTailApp;
use std::path::{Path, PathBuf};

/// Three timed lines: the left stream of the harness.
const SHORT: &[&str] = &[
    "2026-09-18T14:02:05.100Z INFO short one",
    "2026-09-18T14:02:05.225Z INFO short two",
    "2026-09-18T14:02:05.300Z INFO short three",
];
/// Six timed lines, 125 ms apart: the right stream.
const LONG: &[&str] = &[
    "2026-09-18T14:02:05.000Z INFO long one",
    "2026-09-18T14:02:05.125Z INFO long two",
    "2026-09-18T14:02:05.250Z INFO long three",
    "2026-09-18T14:02:05.375Z INFO long four",
    "2026-09-18T14:02:05.500Z INFO long five",
    "2026-09-18T14:02:05.625Z INFO long six",
];

/// Two streams side by side in one dock, drawn frame by frame as the app does, with
/// the text painted by the last frame and where it was painted.
struct Harness {
    engines: Vec<TailEngine>,
    open_files: Vec<PathBuf>,
    dock: egui_dock::DockState<FastTailTab>,
    ctx: egui::Context,
    texts: Vec<(String, egui::Pos2)>,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let short = dir.path().join("short.log");
        let long = dir.path().join("long.log");
        super::write_lines(&short, SHORT);
        super::write_lines(&long, LONG);
        let engines = vec![
            TailEngine::open(&short).unwrap(),
            TailEngine::open(&long).unwrap(),
        ];
        let mut dock = egui_dock::DockState::new(vec![FastTailTab::LogStream(short.clone())]);
        dock.main_surface_mut().split_right(
            egui_dock::NodeIndex::root(),
            0.5,
            vec![FastTailTab::LogStream(long.clone())],
        );
        Self {
            engines,
            open_files: vec![short, long],
            dock,
            ctx: egui::Context::default(),
            texts: Vec::new(),
            _dir: dir,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(2400.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let focused_stream = Some(self.open_files[0].clone());
        let mut out = self.ctx.run_ui(input, |ui| {
            let dock_ctx = DockContext {
                engines: &mut self.engines,
                open_files: &mut self.open_files,
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
                auto_highlight: &mut false,
                auto_highlight_kinds: &mut fasttail::auto_highlight::TokenKinds::default(),
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
                focused_stream: focused_stream.clone(),
                search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
                time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
                find_all: &mut fasttail::find_all::FindAllSession::default(),
                scratchpad: &mut Default::default(),
                compare: &mut None,
                filter_presets: &mut Vec::new(),
                preset_events: &mut Default::default(),
                palette_action: None,
                markdown_caches: &mut Default::default(),
            };
            let mut viewer = FastTailTabViewer { ctx: dock_ctx };
            egui_dock::DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        });
        out.textures_delta.clear();
        self.texts.clear();
        for clipped in &out.shapes {
            collect_texts(&clipped.shape, &mut self.texts);
        }
    }

    /// Draws two frames: the first times the small files, the second shows it.
    fn draw(&mut self) {
        self.frame(Vec::new());
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

    fn painted(&self, text: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == text)
    }

    /// Centres of the labels reading exactly `text`, left to right.
    fn labels(&self, text: &str) -> Vec<egui::Pos2> {
        let mut found: Vec<egui::Pos2> = self
            .texts
            .iter()
            .filter(|(t, _)| t == text)
            .map(|(_, p)| *p)
            .collect();
        found.sort_by(|a, b| a.x.total_cmp(&b.x));
        found
    }
}

fn collect_texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
    match shape {
        egui::Shape::Text(text) => {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            out.push((text.galley.text().to_string(), rect.center()));
        }
        egui::Shape::Vec(shapes) => {
            for s in shapes {
                collect_texts(s, out);
            }
        }
        _ => {}
    }
}

fn number_cell(line: usize) -> String {
    format!("{line:>6} │")
}

#[test]
fn two_streams_with_different_toggles_render_differently() {
    let mut h = Harness::new();
    h.engines[0].show_line_numbers = true;
    h.engines[0].show_time_delta = false;
    h.engines[1].show_line_numbers = false;
    h.engines[1].show_time_delta = true;
    h.draw();

    // The short stream numbers its three rows; the long one numbers none of its six.
    assert!(h.painted(&number_cell(1)) && h.painted(&number_cell(3)));
    assert!(!h.painted(&number_cell(4)) && !h.painted(&number_cell(6)));
    assert_eq!(h.labels("# 123").len(), 1);
    assert_eq!(h.labels("# ---").len(), 1);
    assert!(h.labels("# 123")[0].x < h.labels("# ---")[0].x, "left bar");

    // Only the long stream is timed for its column, and shows its deltas.
    assert!(h.engines[1].timestamps_complete());
    assert_eq!(h.engines[1].row_time_delta(1), TimeDelta::Millis(125));
    assert!(h.painted("+0.125"));
    assert!(!h.engines[0].timestamps_complete(), "no column, no timing");

    // Wrapped rows follow the same per-stream switches.
    h.engines[0].wrap_lines = true;
    h.engines[1].wrap_lines = true;
    h.draw();
    assert!(h.painted(&number_cell(3)));
    assert!(!h.painted(&number_cell(6)));
    assert!(h.painted("+0.125"));
}

#[test]
fn a_stream_bar_toggle_changes_only_its_stream() {
    let mut h = Harness::new();
    h.draw();
    let bars = h.labels("# 123");
    assert_eq!(bars.len(), 2, "both streams start with line numbers");
    h.click(bars[0]);
    assert!(
        !h.engines[0].show_line_numbers,
        "the clicked stream hides them"
    );
    assert!(h.engines[1].show_line_numbers, "the other keeps them");
    assert!(h.engines[0].view_columns_dirty && !h.engines[1].view_columns_dirty);
    assert!(h.painted(&number_cell(6)));

    let deltas = h.labels("Δt");
    assert_eq!(deltas.len(), 2);
    h.click(deltas[1]);
    assert!(h.engines[1].show_time_delta, "the clicked stream shows Δt");
    assert!(!h.engines[0].show_time_delta, "the other does not");
    h.draw();
    assert!(h.painted("+0.125"));
    assert!(!h.engines[0].timestamps_complete());
}

fn config_in(dir: &Path) -> FastTailConfig {
    FastTailConfig {
        spool_dir: Some(dir.to_path_buf()),
        ..Default::default()
    }
}

fn frame(app: &mut FastTailApp) {
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

fn columns(app: &FastTailApp, path: &Path) -> (bool, bool) {
    let e = app.engines.iter().find(|e| e.path == path).unwrap();
    (e.show_line_numbers, e.show_time_delta)
}

#[test]
fn a_new_stream_takes_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    super::write_lines(&log, SHORT);
    super::write_lines(&dir.path().join("svc-1.log"), LONG);
    let pattern = dir.path().join("svc-*.log");

    let mut app = FastTailApp::from_config(FastTailConfig {
        show_line_numbers: false,
        show_time_delta: true,
        ..config_in(dir.path())
    });
    app.open_log_file(log.clone());
    app.open_log_file(pattern.clone());
    assert_eq!(columns(&app, &log), (false, true));
    assert_eq!(columns(&app, &pattern), (false, true));

    // The shipped defaults: numbers on, Δt off.
    let mut app = FastTailApp::from_config(config_in(dir.path()));
    app.open_log_file(log.clone());
    assert_eq!(columns(&app, &log), (true, false));
}

#[test]
fn the_workspace_keeps_each_streams_columns() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    super::write_lines(&a, SHORT);
    super::write_lines(&b, LONG);
    let mut app = FastTailApp::from_config(config_in(dir.path()));
    app.open_log_file(a.clone());
    app.open_log_file(b.clone());
    frame(&mut app);
    let first = app.engines.iter_mut().find(|e| e.path == a).unwrap();
    first.set_show_line_numbers(false);
    first.set_show_time_delta(true);
    frame(&mut app);
    assert_eq!(columns(&app, &a), (false, true));
    assert_eq!(
        columns(&app, &b),
        (true, false),
        "the other stream is untouched"
    );

    // Both switches are written for every stream, in its section.
    let entry = app.config.stream_state_for(&a).unwrap();
    assert_eq!(
        (entry.line_numbers, entry.time_delta),
        (Some(false), Some(true))
    );
    let entry = app.config.stream_state_for(&b).unwrap();
    assert_eq!(
        (entry.line_numbers, entry.time_delta),
        (Some(true), Some(false))
    );
    let mut buf = Vec::new();
    app.config.to_ini().write_to(&mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    // Anchored at the line start: `[general]` has `show_line_numbers=` too.
    let count = |key: &str| text.matches(&format!("\n{key}")).count();
    assert_eq!(count("line_numbers=false"), 1, "{text}");
    assert_eq!(count("time_delta=true"), 1, "{text}");
    assert_eq!(count("line_numbers=true"), 1, "{text}");
    assert_eq!(count("time_delta=false"), 1, "{text}");

    // Restored on the next start.
    let restored = FastTailConfig::from_ini(&app.config.to_ini());
    let app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..restored
    });
    assert_eq!(columns(&app, &a), (false, true));
    assert_eq!(columns(&app, &b), (true, false));
}

#[test]
fn changing_the_defaults_keeps_open_streams_and_their_saved_columns() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    super::write_lines(&a, SHORT);
    let mut app = FastTailApp::from_config(config_in(dir.path()));
    app.open_log_file(a.clone());
    frame(&mut app);
    // Settings turns the numbers off for new streams: the open one keeps them, and
    // so does its saved entry.
    app.config.show_line_numbers = false;
    frame(&mut app);
    assert_eq!(columns(&app, &a), (true, false));
    let entry = app.config.stream_state_for(&a).unwrap();
    assert_eq!(
        (entry.line_numbers, entry.time_delta),
        (Some(true), Some(false))
    );
}

#[test]
fn a_saved_session_does_not_follow_a_later_change_of_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    super::write_lines(&a, SHORT);
    super::write_lines(&b, LONG);
    // Saved while Δt is off by default: a has it on, b is left at the default.
    let mut app = FastTailApp::from_config(config_in(dir.path()));
    app.open_log_file(a.clone());
    app.open_log_file(b.clone());
    let first = app.engines.iter_mut().find(|e| e.path == a).unwrap();
    first.set_show_time_delta(true);
    let file = dir.path().join(format!("x{SESSION_SUFFIX}"));
    app.save_session_as(file.clone()).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("time_delta=").count(), 2, "{text}");
    assert_eq!(text.matches("line_numbers=").count(), 2, "{text}");

    // Loaded after the defaults changed: every stream comes back as it was saved.
    let mut later = FastTailApp::from_config(FastTailConfig {
        show_line_numbers: false,
        show_time_delta: true,
        ..config_in(dir.path())
    });
    later.load_session_file(file, true);
    assert_eq!(columns(&later, &a), (true, true));
    assert_eq!(columns(&later, &b), (true, false));
}

#[test]
fn changing_a_default_does_not_mark_the_named_session_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    super::write_lines(&a, SHORT);
    let mut app = FastTailApp::from_config(config_in(dir.path()));
    app.open_log_file(a.clone());
    frame(&mut app);
    let file = dir.path().join(format!("clean{SESSION_SUFFIX}"));
    app.save_session_as(file).unwrap();
    let check = |app: &mut FastTailApp| {
        app.last_dirty_check = std::time::Instant::now() - std::time::Duration::from_secs(2);
        frame(app);
        app.session_dirty
    };
    assert!(!check(&mut app));

    app.config.show_line_numbers = false;
    app.config.show_time_delta = true;
    assert!(!check(&mut app), "the defaults are not part of the session");
    assert_eq!(columns(&app, &a), (true, false));

    // A stream's own switch is.
    app.engines[0].set_show_time_delta(true);
    assert!(check(&mut app));
}

#[test]
fn a_session_file_keeps_each_streams_columns() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    super::write_lines(&a, SHORT);
    super::write_lines(&b, LONG);
    let mut app = FastTailApp::from_config(config_in(dir.path()));
    app.open_log_file(a.clone());
    app.open_log_file(b.clone());
    let second = app.engines.iter_mut().find(|e| e.path == b).unwrap();
    second.set_show_line_numbers(false);
    second.set_show_time_delta(true);
    let file = dir.path().join(format!("cols{SESSION_SUFFIX}"));
    app.save_session_as(file.clone()).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("line_numbers=false"), "{text}");
    assert!(text.contains("time_delta=true"), "{text}");

    let mut other = FastTailApp::from_config(config_in(dir.path()));
    other.load_session_file(file, true);
    assert_eq!(columns(&other, &a), (true, false));
    assert_eq!(columns(&other, &b), (false, true));
}

/// The sections an older build wrote for `path`: no `line_numbers` nor `time_delta`.
fn old_stream_sections(path: &Path) -> ini::Ini {
    let mut conf = ini::Ini::new();
    conf.with_section(Some("session")).set("streams", "1");
    conf.with_section(Some("stream_0"))
        .set("path", path.to_string_lossy().to_string())
        .set("include", "")
        .set("exclude", "")
        .set("search", "")
        .set("wrap", "false")
        .set("encoding", "")
        .set("bookmarks", "");
    conf
}

#[test]
fn an_old_session_file_without_the_keys_takes_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    super::write_lines(&a, SHORT);
    let file = dir.path().join(format!("old{SESSION_SUFFIX}"));
    old_stream_sections(&a).write_to_file(&file).unwrap();
    let loaded = fasttail::session::Session::load_from(&file).unwrap();
    let entry = &loaded.session.streams[0];
    assert_eq!((entry.line_numbers, entry.time_delta), (None, None));

    let mut app = FastTailApp::from_config(FastTailConfig {
        show_line_numbers: false,
        show_time_delta: true,
        ..config_in(dir.path())
    });
    app.load_session_file(file, true);
    assert_eq!(columns(&app, &a), (false, true));
}

#[test]
fn an_old_workspace_without_the_keys_takes_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    super::write_lines(&a, SHORT);
    let mut ini = old_stream_sections(&a);
    ini.with_section(Some("general"))
        .set("show_line_numbers", "false")
        .set("show_time_delta", "true");
    let cfg = FastTailConfig::from_ini(&ini);
    assert_eq!(cfg.stream_state_for(&a).unwrap().line_numbers, None);
    let app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        open_files: vec![a.clone()],
        ..cfg
    });
    assert_eq!(columns(&app, &a), (false, true));
}
