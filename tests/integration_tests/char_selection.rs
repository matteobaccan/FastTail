// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::tail_engine::{CaretStep, CharSelection, TailEngine};
use fasttail::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
use std::path::PathBuf;
use std::sync::Arc;

const LOG: &[&str] = &[
    "2026-09-18T14:02:05.100Z INFO request id=abc-123 done",
    "2026-09-18T14:02:05.225Z WARN retry user=bob",
];

#[test]
fn caret_keys_move_the_head_by_characters_words_and_to_the_ends() {
    let text = "ERROR id=abc-123 in 5ms".to_string();
    let mut sel = CharSelection::new(0, text, 6, 6);
    sel.move_head(CaretStep::WordRight);
    assert_eq!(sel.selected(), "id");
    sel.move_head(CaretStep::WordRight);
    assert_eq!(sel.selected(), "id=abc");
    sel.move_head(CaretStep::Right);
    assert_eq!(sel.selected(), "id=abc-");
    sel.move_head(CaretStep::End);
    assert_eq!(sel.selected(), "id=abc-123 in 5ms");
    sel.move_head(CaretStep::Home);
    assert_eq!(sel.selected(), "ERROR ", "the head crosses the anchor");
    sel.move_head(CaretStep::WordRight);
    sel.move_head(CaretStep::WordLeft);
    assert!(sel.is_empty() || sel.selected() == "ERROR ");

    // Offsets are clamped to character boundaries.
    let accents = CharSelection::new(0, "café latte".into(), 4, 99);
    assert_eq!(accents.selected(), "é latte");
    let mut back = CharSelection::new(0, "héllo".into(), 3, 3);
    back.move_head(CaretStep::Left);
    assert_eq!(back.selected(), "é");
}

#[test]
fn truncation_and_a_hiding_filter_clear_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.log");
    super::write_lines(&path, LOG);
    let mut engine = TailEngine::open(&path).unwrap();
    let text = engine.get_line(1).unwrap().into_owned();
    engine.char_selection = Some(CharSelection::new(1, text, 0, 4));
    engine.set_include_filter("request");
    assert!(engine.char_selection.is_none(), "line 1 is hidden");

    engine.set_include_filter("");
    let text = engine.get_line(0).unwrap().into_owned();
    engine.char_selection = Some(CharSelection::new(0, text, 0, 4));
    std::fs::write(&path, "new\n").unwrap();
    engine.poll_updates();
    assert!(engine.char_selection.is_none());
}

/// A text shape painted this frame: its text, where it was drawn and its layout.
struct Painted {
    text: String,
    pos: egui::Pos2,
    galley: Arc<egui::Galley>,
}

struct Harness {
    engines: Vec<TailEngine>,
    open_files: Vec<PathBuf>,
    dock: egui_dock::DockState<FastTailTab>,
    ctx: egui::Context,
    painted: Vec<Painted>,
    /// Text put on the clipboard by the last frame.
    copied: Option<String>,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sel.log");
        super::write_lines(&path, LOG);
        let engines = vec![TailEngine::open(&path).unwrap()];
        let dock = egui_dock::DockState::new(vec![FastTailTab::LogStream(path.clone())]);
        Self {
            engines,
            open_files: vec![path],
            dock,
            ctx: egui::Context::default(),
            painted: Vec::new(),
            copied: None,
            _dir: dir,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 600.0),
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
            };
            let mut viewer = FastTailTabViewer { ctx: dock_ctx };
            egui_dock::DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        });
        out.textures_delta.clear();
        self.copied = out.platform_output.commands.iter().find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
        self.painted.clear();
        for clipped in &out.shapes {
            collect(&clipped.shape, &mut self.painted);
        }
    }

    /// Where character `idx` of the painted row text `row` starts, on its baseline middle.
    fn char_pos(&self, row: &str, idx: usize) -> egui::Pos2 {
        let p = self
            .painted
            .iter()
            .find(|p| p.text == row)
            .unwrap_or_else(|| panic!("row {row:?} not painted"));
        let rect = p
            .galley
            .pos_from_cursor(egui::text::CCursor::new(idx))
            .translate(p.pos.to_vec2());
        egui::pos2(rect.min.x + 1.0, rect.center().y)
    }

    fn button(&mut self, pos: egui::Pos2, pressed: bool) {
        self.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }]);
    }

    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(from)]);
        self.button(from, true);
        let mid = from + (to - from) * 0.5;
        self.frame(vec![egui::Event::PointerMoved(mid)]);
        self.frame(vec![egui::Event::PointerMoved(to)]);
        self.button(to, false);
        self.frame(Vec::new());
    }

    fn clicks(&mut self, at: egui::Pos2, count: usize) {
        self.frame(vec![egui::Event::PointerMoved(at)]);
        for _ in 0..count {
            self.button(at, true);
            self.button(at, false);
        }
        self.frame(Vec::new());
    }

    fn selected(&self) -> Option<String> {
        self.engines[0]
            .char_selection
            .as_ref()
            .map(|sel| sel.selected().to_string())
    }
}

fn collect(shape: &egui::Shape, out: &mut Vec<Painted>) {
    match shape {
        egui::Shape::Text(text) => out.push(Painted {
            text: text.galley.text().to_string(),
            pos: text.pos,
            galley: text.galley.clone(),
        }),
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

#[test]
fn a_drag_on_a_row_selects_characters_and_a_click_keeps_row_selection() {
    let mut h = Harness::new();
    h.frame(Vec::new());
    h.frame(Vec::new());
    let row = LOG[0];
    let start = row.find("abc").unwrap();
    let (from, to) = (h.char_pos(row, start), h.char_pos(row, start + 7));
    h.drag(from, to);
    assert_eq!(h.selected().as_deref(), Some("abc-123"));
    assert!(h.engines[0].is_selected(0), "the row is selected too");

    // A plain click elsewhere on the text leaves a caret there, and selects that row.
    let other = LOG[1];
    let at = h.char_pos(other, 3);
    h.clicks(at, 1);
    let sel = h.engines[0].char_selection.clone().expect("a caret");
    assert_eq!((sel.line, sel.is_empty()), (1, true));
    assert!(h.engines[0].is_selected(1));
}

#[test]
fn double_click_selects_the_token_and_triple_click_the_row() {
    let mut h = Harness::new();
    h.frame(Vec::new());
    h.frame(Vec::new());
    let row = LOG[1];
    let at = h.char_pos(row, row.find("bob").unwrap() + 1);
    h.clicks(at, 2);
    // The token rule of the selection highlight: `=` separates tokens.
    assert_eq!(h.selected().as_deref(), Some("bob"));
    h.clicks(at, 3);
    assert_eq!(h.selected().as_deref(), Some(row));
}

#[test]
fn ctrl_c_copies_the_selected_characters_before_the_rows() {
    let mut h = Harness::new();
    h.frame(Vec::new());
    h.frame(Vec::new());
    let row = LOG[0];
    let start = row.find("abc").unwrap();
    h.drag(h.char_pos(row, start), h.char_pos(row, start + 3));
    assert_eq!(h.selected().as_deref(), Some("abc"));
    let ctrl_c = egui::Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    h.frame(vec![ctrl_c.clone()]);
    assert_eq!(h.copied.as_deref(), Some("abc"));

    // SHIFT + End extends the selection to the end of the row.
    let shift_end = egui::Event::Key {
        key: egui::Key::End,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    h.frame(vec![shift_end]);
    assert_eq!(h.selected().as_deref(), Some(&row[start..]));

    // Esc clears it: CTRL + C copies the selected row again.
    let esc = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame(vec![esc]);
    assert!(h.engines[0].char_selection.is_none());
    h.frame(vec![ctrl_c]);
    assert_eq!(h.copied.as_deref(), Some(row));
}

#[test]
fn select_all_other_views_and_the_time_display_drop_the_selection() {
    use fasttail::tail_engine::ViewMode;
    use fasttail::timestamp::TimeDisplay;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.log");
    super::write_lines(&path, LOG);
    let mut engine = TailEngine::open(&path).unwrap();
    let select = |engine: &mut TailEngine| {
        let text = engine.get_line(0).unwrap().into_owned();
        engine.char_selection = Some(CharSelection::new(0, text, 0, 4));
    };
    select(&mut engine);
    engine.select_all_visible();
    assert!(engine.char_selection.is_none());
    select(&mut engine);
    engine.set_view_mode(ViewMode::Hex);
    assert!(engine.char_selection.is_none());
    engine.set_view_mode(ViewMode::Text);
    select(&mut engine);
    engine.set_time_display(TimeDisplay::Utc);
    assert!(engine.char_selection.is_none());
}

#[test]
fn a_shift_click_selects_rows_and_ctrl_c_copies_them_again() {
    let mut h = Harness::new();
    h.frame(Vec::new());
    h.frame(Vec::new());
    let row = LOG[0];
    let start = row.find("abc").unwrap();
    h.drag(h.char_pos(row, start), h.char_pos(row, start + 3));
    assert_eq!(h.selected().as_deref(), Some("abc"));
    let at = h.char_pos(LOG[1], 2);
    let shift_button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::SHIFT,
    };
    h.frame(vec![
        egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
        egui::Event::PointerMoved(at),
    ]);
    h.frame(vec![shift_button(true)]);
    h.frame(vec![shift_button(false)]);
    h.frame(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    assert!(h.engines[0].char_selection.is_none());
    assert!(h.engines[0].is_selected(0) && h.engines[0].is_selected(1));
    h.frame(vec![egui::Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }]);
    assert_eq!(
        h.copied.as_deref(),
        Some(format!("{}\n{}", LOG[0], LOG[1]).as_str())
    );
}
