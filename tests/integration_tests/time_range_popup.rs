//! The stream bar's time span is the time range control: a click opens a popup whose
//! edits are a draft until OK.

use fasttail::i18n::Language;
use fasttail::tail_engine::TailEngine;
use fasttail::timestamp::{date_to_days, parse_user_time};
use fasttail::ui::calendar::day_id;
use fasttail::ui::time_range::{
    calendar_id, cancel_id, control_id, draft, field_id, ok_id, shortcut_id, Side,
};

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
    fn new(path: &std::path::Path) -> Self {
        use fasttail::ui::dock::FastTailTab;
        let engine = TailEngine::open(path).unwrap();
        let mut h = Self {
            engines: vec![engine],
            open_files: vec![path.to_path_buf()],
            dock: egui_dock::DockState::new(vec![FastTailTab::LogStream(path.to_path_buf())]),
            prefs: fasttail::ui::dock::SearchViewPrefs::default(),
            ctx: egui::Context::default(),
            path: path.to_path_buf(),
        };
        h.frame(Vec::new());
        h.frame(Vec::new());
        h
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        use fasttail::ui::dock::{DockContext, FastTailTabViewer};
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 1000.0),
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
                focused_stream: Some(self.path.clone()),
                search_view: &mut self.prefs,
                time_delta: &mut fasttail::ui::dock::TimeDeltaPrefs::default(),
                find_all: &mut fasttail::find_all::FindAllSession::default(),
                filter_presets: &mut Vec::new(),
                preset_events: &mut Default::default(),
                palette_action: None,
            };
            let mut viewer = FastTailTabViewer { ctx: dock_ctx };
            egui_dock::DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        });
        out.textures_delta.clear();
    }

    fn key(&mut self, key: egui::Key) {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(Vec::new());
    }

    fn click_at(&mut self, pos: egui::Pos2) {
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

    fn rect(&self, id: egui::Id) -> egui::Rect {
        self.ctx
            .read_response(id)
            .map(|r| r.rect)
            .expect("the widget is on screen")
    }

    fn click(&mut self, id: egui::Id) {
        let pos = self.rect(id).center();
        self.click_at(pos);
    }

    /// Types `text` at the end of a side's field.
    fn type_into(&mut self, side: Side, text: &str) {
        let id = field_id(self.engine(), side);
        self.ctx.memory_mut(|m| m.request_focus(id));
        self.frame(Vec::new());
        self.frame(vec![egui::Event::Key {
            key: egui::Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(vec![egui::Event::Text(text.into())]);
        self.frame(Vec::new());
    }

    fn open(&mut self) {
        let id = control_id(self.engine());
        self.click(id);
        self.frame(Vec::new());
        assert!(self.is_open(), "the popup opened");
    }

    fn is_open(&self) -> bool {
        draft(&self.ctx, self.engine()).is_some()
    }

    fn engine(&self) -> &TailEngine {
        &self.engines[0]
    }
}

/// 181 entries, one a minute from 2026-09-18 14:00 to 17:00.
fn day_log(dir: &std::path::Path) -> std::path::PathBuf {
    let log = dir.join("day.log");
    let mut text = String::new();
    for i in 0..=180 {
        text.push_str(&format!(
            "2026-09-18 {:02}:{:02}:05 INFO request {i}\n",
            14 + i / 60,
            i % 60
        ));
    }
    std::fs::write(&log, text).unwrap();
    log
}

/// Ten entries a day from 2026-05-25 to 2026-05-29, then one at 23:38:12.
fn week_log(dir: &std::path::Path) -> std::path::PathBuf {
    let log = dir.join("week.log");
    let mut text = String::new();
    for day in 25..=29 {
        for i in 0..10 {
            text.push_str(&format!("2026-05-{day} {:02}:00:00 INFO entry\n", 8 + i));
        }
    }
    text.push_str("2026-05-29 23:38:12 INFO last\n");
    std::fs::write(&log, text).unwrap();
    log
}

#[test]
fn first_and_last_timestamps_of_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("t.log");
    std::fs::write(
        &log,
        "banner\n2026-09-18 14:02:05 INFO a\n  at trace\n2026-09-18 16:30:12 INFO b\n  at x\n",
    )
    .unwrap();
    let mut engine = TailEngine::open(&log).unwrap();
    engine.ensure_timestamps();
    assert_eq!(
        engine.first_timestamp(),
        parse_user_time("2026-09-18 14:02:05", 0)
    );
    // The trace line inherits the time of its entry.
    assert_eq!(
        engine.last_timestamp(),
        parse_user_time("2026-09-18 16:30:12", 0)
    );
    assert!(!engine.timestamps_unordered());
    // Out of order: the first and last lines, not the earliest and the latest.
    let back = dir.path().join("back.log");
    std::fs::write(
        &back,
        "2026-09-18 14:00:00 a
2026-09-18 09:00:00 b
2026-09-18 12:00:00 c
",
    )
    .unwrap();
    let mut engine = TailEngine::open(&back).unwrap();
    engine.ensure_timestamps();
    assert!(engine.timestamps_unordered());
    assert_eq!(
        engine.first_timestamp(),
        parse_user_time("2026-09-18 14:00:00", 0)
    );
    assert_eq!(
        engine.last_timestamp(),
        parse_user_time("2026-09-18 12:00:00", 0)
    );
    let plain = dir.path().join("plain.log");
    std::fs::write(&plain, "no\ntimes\nhere\n").unwrap();
    let mut engine = TailEngine::open(&plain).unwrap();
    engine.ensure_timestamps();
    assert_eq!(engine.first_timestamp(), None);
    assert_eq!(engine.last_timestamp(), None);
}

#[test]
fn a_click_opens_the_popup_with_the_window_that_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let log = day_log(dir.path());
    let mut h = Harness::new(&log);
    h.open();
    let d = draft(&h.ctx, h.engine()).unwrap();
    assert_eq!((d.from.as_str(), d.to.as_str()), ("", ""));
    // The calendars open on the month of the log.
    assert_eq!((d.from_month.year, d.from_month.month), (2026, 9));
    assert!(h.engine().timestamps_complete(), "opening times the stream");
    h.key(egui::Key::Escape);
    assert!(!h.is_open());

    // With a window set, the popup shows it.
    h.engines[0].apply_time_range_text("15:00", "15:30");
    h.frame(Vec::new());
    h.open();
    let d = draft(&h.ctx, h.engine()).unwrap();
    assert_eq!((d.from.as_str(), d.to.as_str()), ("15:00", "15:30"));
}

#[test]
fn ok_applies_the_draft_and_filters() {
    let dir = tempfile::tempdir().unwrap();
    let log = day_log(dir.path());
    let mut h = Harness::new(&log);
    h.open();
    h.type_into(Side::From, "2026-09-18 15:00");
    h.type_into(Side::To, "2026-09-18 15:30");
    // Nothing changes before OK.
    assert!(!h.engine().is_time_filtered());
    assert_eq!(h.engine().visible_lines(), 181);
    let ok = ok_id(h.engine());
    h.click(ok);
    assert!(!h.is_open(), "OK closes the popup");
    assert!(h.engine().is_time_filtered());
    assert_eq!(h.engine().time_from_text, "2026-09-18 15:00");
    assert_eq!(h.engine().time_to_text, "2026-09-18 15:30");
    // 15:00:05 through 15:30:05.
    assert_eq!(h.engine().visible_lines(), 31);
    assert!(!h.engine().time_range_error);
}

#[test]
fn enter_in_a_field_applies_like_ok() {
    let dir = tempfile::tempdir().unwrap();
    let log = day_log(dir.path());
    let mut h = Harness::new(&log);
    h.open();
    h.type_into(Side::From, "16:00");
    h.key(egui::Key::Enter);
    assert!(!h.is_open());
    assert_eq!(h.engine().time_from_text, "16:00");
    assert_eq!(h.engine().visible_lines(), 61);
}

#[test]
fn esc_cancel_and_a_click_outside_keep_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let log = day_log(dir.path());
    let mut h = Harness::new(&log);
    h.engines[0].apply_time_range_text("15:00", "15:30");
    h.frame(Vec::new());
    let visible = h.engine().visible_lines();
    assert_eq!(visible, 31);

    h.open();
    h.type_into(Side::From, "9");
    h.type_into(Side::To, "9");
    assert_eq!(draft(&h.ctx, h.engine()).unwrap().from, "15:009");
    h.key(egui::Key::Escape);
    assert!(!h.is_open(), "Esc closes the popup");
    assert_eq!(h.engine().time_from_text, "15:00");
    assert_eq!(h.engine().time_to_text, "15:30");
    assert_eq!(h.engine().visible_lines(), visible);

    h.open();
    h.type_into(Side::To, "9");
    let cancel = cancel_id(h.engine());
    h.click(cancel);
    assert!(!h.is_open(), "Cancel closes the popup");
    assert_eq!(h.engine().time_to_text, "15:30");

    h.open();
    h.type_into(Side::To, "9");
    h.click_at(egui::pos2(1500.0, 900.0));
    assert!(!h.is_open(), "a click outside closes the popup");
    assert_eq!(h.engine().time_to_text, "15:30");
    assert_eq!(h.engine().visible_lines(), visible);

    // The popup opens again with the window, not the dropped draft.
    h.open();
    assert_eq!(draft(&h.ctx, h.engine()).unwrap().to, "15:30");
}

#[test]
fn ok_is_disabled_while_a_side_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let log = day_log(dir.path());
    let mut h = Harness::new(&log);
    h.open();
    h.type_into(Side::To, "14:6x");
    let ok = ok_id(h.engine());
    h.click(ok);
    assert!(h.is_open(), "a disabled OK does nothing");
    h.type_into(Side::To, "");
    h.key(egui::Key::Enter);
    assert!(h.is_open(), "neither does Enter");
    assert!(!h.engine().is_time_filtered());
    assert_eq!(h.engine().time_to_text, "");
    assert_eq!(draft(&h.ctx, h.engine()).unwrap().to, "14:6x");
}

#[test]
fn picking_a_day_on_both_calendars_gives_the_whole_day() {
    let dir = tempfile::tempdir().unwrap();
    let log = week_log(dir.path());
    let mut h = Harness::new(&log);
    h.open();
    let day = date_to_days(2026, 5, 27).unwrap();
    let from_cell = day_id(calendar_id(h.engine(), Side::From), day);
    let to_cell = day_id(calendar_id(h.engine(), Side::To), day);
    h.click(from_cell);
    h.click(to_cell);
    let d = draft(&h.ctx, h.engine()).unwrap();
    assert_eq!(
        (d.from.as_str(), d.to.as_str()),
        ("2026-05-27", "2026-05-27")
    );
    let ok = ok_id(h.engine());
    h.click(ok);
    assert!(h.engine().is_time_filtered());
    assert_eq!(h.engine().visible_lines(), 10);
}

#[test]
fn shortcuts_fill_the_draft_and_apply_on_ok() {
    let dir = tempfile::tempdir().unwrap();
    let log = week_log(dir.path());
    let mut h = Harness::new(&log);
    h.open();
    h.frame(Vec::new());

    let last_hour = shortcut_id(h.engine(), "time_range_last_hour");
    h.click(last_hour);
    let d = draft(&h.ctx, h.engine()).unwrap();
    assert_eq!(
        (d.from.as_str(), d.to.as_str()),
        ("2026-05-29 22:38:12", "2026-05-29 23:38:12")
    );
    assert!(!h.engine().is_time_filtered(), "a shortcut is a draft");

    let first_day = shortcut_id(h.engine(), "time_range_first_day");
    h.click(first_day);
    let d = draft(&h.ctx, h.engine()).unwrap();
    assert_eq!(
        (d.from.as_str(), d.to.as_str()),
        ("2026-05-25", "2026-05-25")
    );

    let last_day = shortcut_id(h.engine(), "time_range_last_day");
    h.click(last_day);
    let ok = ok_id(h.engine());
    h.click(ok);
    assert_eq!(h.engine().time_from_text, "2026-05-29");
    assert_eq!(h.engine().visible_lines(), 11, "only 2026-05-29");

    // Whole log empties both sides, and OK removes the window.
    h.open();
    let whole = shortcut_id(h.engine(), "time_range_whole");
    h.click(whole);
    let ok = ok_id(h.engine());
    h.click(ok);
    assert!(!h.engine().is_time_filtered());
    assert_eq!(h.engine().visible_lines(), 51);
}

#[test]
fn the_fields_live_in_the_popup_and_fit_a_full_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let log = day_log(dir.path());
    let mut h = Harness::new(&log);
    let from = field_id(h.engine(), Side::From);
    assert!(
        h.ctx.read_response(from).is_none(),
        "no time field outside the popup"
    );
    h.open();
    h.frame(Vec::new());
    assert!(h.rect(from).width() >= 170.0, "a full timestamp fits");
}
