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

/// Draws the stream in a dock, as the app does.
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
            focused_stream: Some(path.clone()),
            search_view: &mut fasttail::ui::dock::SearchViewPrefs::default(),
            time_delta: prefs,
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

#[test]
fn the_column_times_the_stream_when_shown() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("ui.log");
    super::write_lines(&log, LOG);
    let mut engines = vec![TailEngine::open(&log).unwrap()];
    let mut prefs = fasttail::ui::dock::TimeDeltaPrefs::default();
    assert!(!prefs.show && !engines[0].show_time_delta, "off by default");
    assert_eq!(prefs.gap_ms, 1000);

    // Off: nothing asks for the timing.
    render(&mut engines, &mut prefs);
    assert_eq!(engines[0].row_time_delta(1), TimeDelta::Pending);

    // On, extended and wrapped rows, with and without an anchor.
    engines[0].set_show_time_delta(true);
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
