// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::wait_for_jobs;
use fasttail::config::FastTailConfig;
use fasttail::i18n::{t, Language};
use fasttail::log_level::LogLevel;
use fasttail::tail_engine::{is_error_level, TailEngine, ERROR_BLOCK_LINES, MAX_SEARCH_MATCHES};
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
                scratchpad: &mut Default::default(),
                filter_presets: &mut Vec::new(),
                preset_events: &mut Default::default(),
                palette_action: None,
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
    h.engines[0].timeline_open = true;
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
    h.engines[0].timeline_open = false;
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
    assert!(cfg.timeline_search_lane, "its search lane is on by default");
    cfg.timeline_search_lane = false;
    let restored = FastTailConfig::from_ini(&cfg.to_ini());
    assert!(!restored.timeline_search_lane);
    // The histogram is shown per stream: no global key is written.
    let ini = cfg.to_ini();
    let general = ini.section(Some("general")).unwrap();
    assert!(general.get("timeline_histogram").is_none());
}

#[test]
fn the_timeline_opens_only_on_its_stream_and_is_saved_with_it() {
    use fasttail::session::{Session, StreamEntry, SESSION_SUFFIX};
    let dir = tempfile::tempdir().unwrap();
    let a = timeline_log(dir.path());
    let b = dir.path().join("other.log");
    std::fs::copy(&a, &b).unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = fasttail::ui::FastTailApp::from_config(config);
    app.open_log_file(a.clone());
    app.open_log_file(b.clone());
    let ctx = egui::Context::default();
    let frame = |app: &mut fasttail::ui::FastTailApp| {
        let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
        out.textures_delta.clear();
    };
    frame(&mut app);
    let first = app.engines.iter_mut().find(|e| e.path == a).unwrap();
    first.timeline_open = true;
    first.timeline_dirty = true;
    frame(&mut app);
    frame(&mut app);
    let engine = |app: &fasttail::ui::FastTailApp, p: &std::path::Path| {
        app.engines
            .iter()
            .find(|e| e.path == p)
            .unwrap()
            .timeline_open
    };
    assert!(engine(&app, &a));
    assert!(!engine(&app, &b), "the other stream keeps its view");
    let other = app.engines.iter().find(|e| e.path == b).unwrap();
    assert!(!other.timestamps_complete(), "and is not timed for it");

    // Saved with the stream: in the workspace entry and in a session file.
    assert!(app.config.stream_state_for(&a).is_some_and(|s| s.timeline));
    let mut entry = StreamEntry::new(a.clone());
    entry.timeline = true;
    let session = Session {
        streams: vec![entry, StreamEntry::new(b.clone())],
        dock_layout: None,
    };
    let file = dir.path().join(format!("t{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    assert_eq!(Session::load_from(&file).unwrap().session, session);
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
