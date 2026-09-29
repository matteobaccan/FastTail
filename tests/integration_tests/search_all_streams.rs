// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

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
        auto_highlight: leak(false),
        auto_highlight_kinds: leak(fasttail::auto_highlight::TokenKinds::NONE),
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
        palette_action: None,
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

    /// The stream's tab is the one shown in its dock leaf.
    fn stream_shown(&self, idx: usize) -> bool {
        let tab = FastTailTab::LogStream(self.engines[idx].path.clone());
        let Some(path) = self.dock.find_tab(&tab) else {
            return false;
        };
        self.dock
            .leaf(path.node_path())
            .is_ok_and(|leaf| leaf.active == path.tab)
    }

    /// One frame with a palette stream action for stream `idx`; returns whether a
    /// drawn stream took it.
    fn frame_with_palette_action(
        &mut self,
        idx: usize,
        action: fasttail::actions::ActionId,
    ) -> bool {
        for e in &mut self.engines {
            e.poll_updates();
        }
        let target = self.engines[idx].path.clone();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut taken = false;
        let mut out = self.ctx.run_ui(input, |ui| {
            let mut dock_ctx = dock_context(
                &mut self.engines,
                &mut self.open_files,
                &mut self.session,
                None,
            );
            dock_ctx.palette_action = Some((target.clone(), action));
            let mut viewer = fasttail::ui::dock::FastTailTabViewer { ctx: dock_ctx };
            egui_dock::DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
            taken = viewer.ctx.palette_action.is_none();
        });
        out.textures_delta.clear();
        taken
    }

    fn active_is_stream(&mut self, idx: usize) -> bool {
        let path = self.engines[idx].path.clone();
        matches!(
            self.dock.find_active_focused(),
            Some((_, FastTailTab::LogStream(p))) if *p == path
        )
    }
}

/// A palette action for a stream whose tab is hidden stays pending (the app keeps it
/// and brings the tab to front); once the tab is drawn the stream takes and runs it.
#[test]
fn palette_action_waits_for_a_hidden_stream() {
    use fasttail::actions::{ActionId, PendingAction};
    let mut h = Harness::new(&[
        (
            "a.log",
            "a1
a2
"
            .to_string(),
        ),
        (
            "b.log",
            "b1
b2
"
            .to_string(),
        ),
    ]);
    h.frame(Vec::new(), false);
    assert!(h.stream_shown(0) && !h.stream_shown(1));

    let mut pending = Some(PendingAction::new(
        h.engines[1].path.clone(),
        ActionId::BookmarkToggle,
    ));
    let taken = h.frame_with_palette_action(1, ActionId::BookmarkToggle);
    assert!(!taken, "a hidden tab cannot take the action");
    pending = pending.and_then(|p| p.after_frame(taken, true));
    assert!(pending.is_some(), "the action waits for its stream");
    assert!(!h.engines[1].has_bookmarks());

    // What `run_action` does: bring the target tab to front.
    let tab = FastTailTab::LogStream(h.engines[1].path.clone());
    let locator = h.dock.find_tab(&tab).expect("tab");
    let _ = h.dock.set_active_tab(locator);
    let pending = pending.expect("pending");
    assert!(
        h.frame_with_palette_action(1, pending.id),
        "the stream takes it"
    );
    assert!(h.engines[1].has_bookmarks(), "and runs it");
    assert!(!h.engines[0].has_bookmarks());
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
    assert!(
        h.stream_shown(1),
        "the stream's tab is brought to the front"
    );
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
        h.ctx.memory(|m| m.has_focus(list)),
        "the results keep the keyboard"
    );
    assert!(h.active_is_results(), "and the dock focus");

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
    assert!(h.stream_shown(1));
    assert!(h.active_is_results());
}

#[test]
fn typing_a_query_and_enter_runs_the_search() {
    let mut h = Harness::new(&logs());
    open_find_results_tab(&mut h.dock);
    h.frame(Vec::new(), false);
    h.frame(Vec::new(), false);
    h.click(fasttail::ui::find_results::query_input_id());
    assert!(h
        .ctx
        .memory(|m| m.has_focus(fasttail::ui::find_results::query_input_id())));
    h.frame(vec![egui::Event::Text("req-7f3a".into())], false);
    assert_eq!(h.session.input, "req-7f3a");
    h.frame(vec![key(egui::Key::Enter)], false);
    assert_eq!(h.session.query, "req-7f3a", "Enter runs the search");
    h.run_to_end();
    assert_eq!(h.session.total_hits(), 6);
}

#[test]
fn after_a_click_the_keys_walk_the_results_and_the_stream_follows() {
    let mut h = Harness::new(&logs());
    open_find_results_tab(&mut h.dock);
    h.session.input = "req-7f3a".into();
    h.session.start(&h.engines);
    h.run_to_end();
    h.frame(Vec::new(), false);
    let list = results_list_id();
    let still_on_results = |h: &mut Harness| {
        assert!(
            h.ctx.memory(|m| m.has_focus(list)),
            "the list keeps the keyboard"
        );
        assert!(h.active_is_results(), "and the dock focus");
    };
    // Rows: header 0, hits 3 13 23 33 (gateway), header 1, hits 120 250 (payment).
    h.click(GroupedHitList::hit_id(list, 0, 0));
    assert!(
        h.engines[0].is_selected(3),
        "the stream shows the clicked line"
    );
    still_on_results(&mut h);

    h.frame(vec![key(egui::Key::ArrowDown)], false);
    h.frame(Vec::new(), false);
    assert!(
        h.engines[0].is_selected(13),
        "Down: the next result is shown"
    );
    still_on_results(&mut h);

    h.frame(vec![key(egui::Key::End)], false);
    h.frame(Vec::new(), false);
    assert!(h.engines[1].is_selected(250), "End: the last result");
    assert!(h.stream_shown(1));
    still_on_results(&mut h);

    h.frame(vec![key(egui::Key::Home)], false);
    h.frame(vec![key(egui::Key::ArrowDown)], false);
    h.frame(Vec::new(), false);
    assert!(
        h.engines[0].is_selected(3),
        "Home then Down: the first result"
    );
    assert!(h.stream_shown(0));

    h.frame(vec![key(egui::Key::PageDown)], false);
    h.frame(Vec::new(), false);
    assert!(
        h.engines[1].is_selected(250),
        "PageDown past the end: the last"
    );
    still_on_results(&mut h);
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
