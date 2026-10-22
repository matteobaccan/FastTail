// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::config::FastTailConfig;
use fasttail::fields::{FieldParser, ParserChoice};
use fasttail::log_format::{self, Library};
use fasttail::log_level::LogLevel;
use fasttail::tail_engine::TailEngine;
use fasttail::ui::FastTailApp;
use std::path::Path;

/// A format for an odd date (`28.09.26 14:02:11,123`) and one-letter levels, for files
/// named `payments-*.log`. No other test's lines match it.
const PAYMENTS: &str = r#"[format]
name = Payments
regex = ^(?P<ts>\d\d\.\d\d\.\d\d \d\d:\d\d:\d\d,\d{3}) \[(?P<thread>[^\]]+)\] (?P<level>[A-Z]) (?P<msg>.*)$
timestamp_field = ts
timestamp_format = %d.%m.%y %H:%M:%S,%3f
level_field = level
level_map = W=warn,E=error,I=info
message_field = msg
files = payments-*.log
columns = ts,level
sample.1 = 28.09.26 14:02:11,123 [main] E card declined
"#;

/// A device log whose entries carry XML bodies over several lines: a line starting with
/// `DEV ` starts an entry, any other continues it.
const DEVICE: &str = r#"[format]
name = Device
regex = ^DEV (?P<ts>\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) (?P<level>[A-Z]+) (?P<msg>.*)$
timestamp_field = ts
level_field = level
entry_start = ^DEV 
files = device-*.log
timestamp_zone = utc
"#;

const LINES: [&str; 4] = [
    "28.09.26 14:02:11,123 [main] I card check",
    "28.09.26 14:02:12,000 [pool-1] E card declined",
    "  at Card.check(Card.java:12)",
    "28.09.26 14:02:13,500 [main] W retry",
];

/// Installs the formats of a folder holding `PAYMENTS` (every test installs the same, so
/// tests running at once do not disturb each other).
fn install(dir: &Path) {
    let formats = dir.join("formats");
    std::fs::create_dir_all(&formats).unwrap();
    std::fs::write(formats.join("payments.fasttail-format.ini"), PAYMENTS).unwrap();
    std::fs::write(formats.join("device.fasttail-format.ini"), DEVICE).unwrap();
    let lib = Library::load(&formats);
    assert!(lib.errors.is_empty(), "{:?}", lib.errors);
    log_format::set_library(lib);
}

fn open(path: &Path) -> TailEngine {
    let mut engine = TailEngine::open(path).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    wait_for_jobs(&mut engine);
    engine.poll_updates();
    engine
}

fn format_name(engine: &TailEngine) -> Option<String> {
    match engine.field_parser().map(|p| &**p) {
        Some(FieldParser::Format(f)) => Some(f.name().to_string()),
        _ => None,
    }
}

fn ms(d: u32, h: i64, m: i64, s: i64, milli: i64) -> i64 {
    fasttail::timestamp::date_to_days(2026, 9, d).unwrap() * 86_400_000
        + h * 3_600_000
        + m * 60_000
        + s * 1000
        + milli
}

#[test]
fn a_format_named_after_the_file_reads_its_times_levels_and_columns() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path());
    let file = dir.path().join("payments-2026-09-28.log");
    write_lines(&file, &LINES);
    let mut e = open(&file);
    assert_eq!(format_name(&e).as_deref(), Some("Payments"));
    assert_eq!(
        e.field_auto(),
        Some(&ParserChoice::Format("Payments".into()))
    );
    e.ensure_timestamps();
    wait_for_jobs(&mut e);
    assert_eq!(e.level_of(1), LogLevel::Error, "E through the level map");
    assert_eq!(e.level_of(3), LogLevel::Warn);
    assert_eq!(e.line_timestamp(1), Some(ms(28, 14, 2, 12, 0)));
    assert_eq!(e.line_timestamp(3), Some(ms(28, 14, 2, 13, 500)));
    assert_eq!(e.field_columns(), vec!["ts", "level"]);
    // The message field is the last column.
    let text = e.get_line(1).unwrap().into_owned();
    let row = e.row_fields(1, &text).unwrap();
    assert_eq!(
        row.message(&e.field_columns()),
        "card declined thread=pool-1"
    );
}

#[test]
fn a_saved_format_comes_back_and_a_missing_one_falls_back_to_detection() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path());
    let log = dir.path().join("app.log");
    write_lines(&log, &LINES);
    let config = || FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config());
    app.open_log_file(log.clone());
    app.engines[0].set_field_choice(ParserChoice::Format("Payments".into()));
    fasttail::workspace::save_changes(&mut app.engines[0], &mut app.config, false);
    app.save_dock_layout();
    let entry = app.config.stream_state_for(&log).unwrap();
    assert_eq!(entry.fields_parser.as_deref(), Some("format:Payments"));
    assert_eq!(
        ParserChoice::from_name(" Format:Payments ", ""),
        Some(ParserChoice::Format("Payments".into())),
        "a hand-edited prefix in another case"
    );
    let again = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..FastTailConfig::from_ini(&app.config.to_ini())
    });
    let engine = again.engines.iter().find(|e| e.path == log).unwrap();
    assert_eq!(
        engine.field_choice(),
        &ParserChoice::Format("Payments".into())
    );
    assert_eq!(format_name(engine).as_deref(), Some("Payments"));

    // A format that is gone: kept as chosen, detection picks the parser meanwhile.
    let mut e = open(&log);
    e.set_field_choice(ParserChoice::Format("Gone".into()));
    assert!(e.field_error().is_some());
    e.poll_updates();
    assert_eq!(format_name(&e).as_deref(), Some("Payments"), "detected");
    assert_eq!(e.field_choice(), &ParserChoice::Format("Gone".into()));
}

#[test]
fn the_presets_are_the_builtin_formats() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path());
    let file = dir.path().join("syslog");
    write_lines(
        &file,
        &["Sep 18 14:02:05 host sshd[812]: Accepted publickey"],
    );
    let mut e = open(&file);
    e.set_field_choice(ParserChoice::Syslog);
    assert_eq!(format_name(&e).as_deref(), Some(log_format::SYSLOG_NAME));
    let text = e.get_line(0).unwrap().into_owned();
    let row = e.row_fields(0, &text).unwrap();
    assert_eq!(row.get("app"), Some("sshd"));
}

/// Two equal entries of five lines (header, XML body with a date and an ERROR word that
/// the stack-trace heuristic would take for entries of their own), then another entry.
const DEVICE_LINES: [&str; 12] = [
    "DEV 2026-09-28 14:02:11 INFO sent order",
    "<order>",
    "  <when>2026-09-29 10:00:00</when>",
    "ERROR in the body, not an entry",
    "</order>",
    "DEV 2026-09-28 14:02:11 INFO sent order",
    "<order>",
    "  <when>2026-09-29 10:00:00</when>",
    "ERROR in the body, not an entry",
    "</order>",
    "DEV 2026-09-28 14:02:15 WARN other",
    "detail",
];

#[test]
fn an_entry_start_pattern_keeps_multi_line_entries_together() {
    use fasttail::collapse::CollapseMode;
    let dir = tempfile::tempdir().unwrap();
    install(dir.path());
    let file = dir.path().join("device-1.log");
    write_lines(&file, &DEVICE_LINES);
    // The synchronous path, then the background jobs.
    for thresholds in [None, Some(0)] {
        let mut e = match thresholds {
            None => open(&file),
            Some(t) => {
                let mut e = TailEngine::open_with_thresholds(&file, t, t).unwrap();
                e.size_check_interval = std::time::Duration::ZERO;
                wait_for_jobs(&mut e);
                e.poll_updates();
                wait_for_jobs(&mut e);
                e
            }
        };
        assert_eq!(format_name(&e).as_deref(), Some("Device"), "{thresholds:?}");
        assert_eq!(
            e.effective_source_zone(),
            fasttail::timestamp::SourceZone::Utc,
            "the format's clock"
        );
        assert_eq!(
            e.time_source_zone(),
            fasttail::timestamp::SourceZone::Local,
            "not the stream's own, so not saved"
        );
        e.ensure_timestamps();
        wait_for_jobs(&mut e);
        let header = ms(28, 14, 2, 11, 0);
        assert_eq!(
            e.line_timestamp(2),
            Some(header),
            "the body's date is no entry"
        );
        assert_eq!(e.line_timestamp(4), Some(header));
        assert_eq!(e.level_of(3), LogLevel::Unknown, "{thresholds:?}");
        assert_eq!(e.level_of(10), LogLevel::Warn);

        // The header matches: its body comes along; only the other entry's line: its own.
        e.set_include_filter("other");
        wait_for_jobs(&mut e);
        let shown: Vec<usize> = (0..e.visible_line_count())
            .filter_map(|r| e.get_actual_line_idx(r))
            .collect();
        assert_eq!(shown, vec![10, 11], "{thresholds:?}");
        e.set_include_filter("sent");
        wait_for_jobs(&mut e);
        assert_eq!(e.visible_line_count(), 10, "{thresholds:?}");

        // The two equal entries of five lines collapse into one group.
        e.set_include_filter("");
        e.set_collapse_mode(CollapseMode::Exact);
        wait_for_jobs(&mut e);
        let groups: Vec<(usize, u32)> = (0..e.visible_line_count())
            .filter_map(|r| e.collapsed_row(r))
            .map(|c| (c.first_line, c.count))
            .collect();
        assert_eq!(groups, vec![(0, 2)], "{thresholds:?}");
    }
}

mod dialog {
    use super::*;
    use fasttail::ui::log_formats::{preview, Editing, FormatsDialog};

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn the_preview_reads_samples_and_stream_lines_without_touching_the_streams() {
        let dir = tempfile::tempdir().unwrap();
        let stream = lines(&[
            "2026-09-28 14:02:11 ERROR boom",
            "2026-09-28 14:02:12 INFO fine",
            "  at Foo.bar(Foo.java:1)",
            "garbage line",
        ]);
        let mut d = FormatsDialog::from_lines(
            dir.path().join("formats"),
            stream,
            &lines(&["2026-09-28 14:02:10 WARN first"]),
        );
        assert_eq!(d.draft.samples, "2026-09-28 14:02:10 WARN first");
        let p = &d.preview;
        assert_eq!(p.error, None);
        assert_eq!(p.rows.len(), 5);
        assert!(p.rows[0].sample);
        assert_eq!(p.entries, 4, "the trace line starts no entry");
        assert_eq!(p.matched, 3);
        assert_eq!(p.ratio(), 75);
        assert_eq!(p.rows[1].level, LogLevel::Error);
        assert_eq!(p.rows[1].time, Some(ms(28, 14, 2, 11, 0)));
        assert!(p.rows[1]
            .fields
            .contains(&("msg".to_string(), "boom".to_string())));
        assert!(!p.rows[4].matched, "unmatched");

        // A pattern that does not compile says why, with its position.
        d.draft.patterns = "^(?P<ts>\\S+".into();
        d.refresh_preview();
        assert!(d.preview.error.as_deref().unwrap().contains("pattern 1"));
        // An entry start that matches nearly nothing is flagged.
        let mut spec = d.draft.to_spec();
        spec.patterns = vec![r"^(?P<msg>.*)$".into()];
        spec.entry_start = "^NEVER".into();
        spec.timestamp_field.clear();
        spec.level_field.clear();
        assert!(preview(&spec, &lines(&["a", "b"])).few_entries());
    }

    #[test]
    fn save_duplicate_import_export_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let formats = dir.path().join("formats");
        let mut d = FormatsDialog::new(formats.clone(), Vec::new());
        d.draft.name = "Orders".into();
        let lib = Library::load(&formats);
        let path = d.save(&lib).unwrap();
        assert_eq!(path, formats.join("Orders.fasttail-format.ini"));
        assert_eq!(d.editing, Editing::File(path.clone()));
        let lib = Library::load(&formats);
        let saved = lib.find("Orders").unwrap();
        assert_eq!(saved.spec.timestamp_field, "ts");

        // A rename moves the format to the file of its new name.
        d.draft.name = "Sales".into();
        let renamed = d.save(&lib).unwrap();
        assert_eq!(renamed, formats.join("Sales.fasttail-format.ini"));
        assert!(!path.exists());
        d.draft.name = "Orders".into();
        let lib = Library::load(&formats);
        assert_eq!(d.save(&lib).unwrap(), path, "and back");
        assert!(!renamed.exists());
        let lib = Library::load(&formats);
        let saved = lib.find("Orders").unwrap();

        // Saving again rewrites the same file; a new format with the name is refused.
        d.draft.level_map = "E=error".into();
        assert_eq!(d.save(&lib).unwrap(), path);
        d.new_format();
        d.draft.name = "orders".into();
        assert!(d.save(&lib).unwrap_err().ends_with("exists"));
        // A built-in format is read-only.
        d.select(lib.find(log_format::SYSLOG_NAME).unwrap());
        assert!(!d.editable());
        assert!(d.save(&lib).is_err());
        d.duplicate();
        assert_eq!(d.draft.name, "Syslog copy");
        assert!(d.editable());

        // Export, then import into another folder.
        d.select(saved);
        let out = dir.path().join("orders-export.ini");
        d.export(&out).unwrap();
        let other = dir.path().join("elsewhere");
        let mut d2 = FormatsDialog::new(other.clone(), Vec::new());
        let imported = d2.import(&Library::load(&other), &out).unwrap();
        assert!(Library::load(&other).find("Orders").is_some());
        assert_eq!(imported, other.join("Orders.fasttail-format.ini"));
        assert!(
            d2.import(&Library::load(&other), &out).is_err(),
            "already there"
        );

        // The dialog draws, its preview and editor included.
        let ctx = egui::Context::default();
        let lib = Library::load(&formats);
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let action = d.show(
                ui.ctx(),
                &lib,
                fasttail::i18n::Language::En,
                fasttail::theme::CyberTheme::Tron,
            );
            assert_eq!(action, None);
        });
        out.textures_delta.clear();

        d.delete(&path).unwrap();
        assert!(Library::load(&formats).find("Orders").is_none());
        assert_eq!(d.editing, Editing::New);
    }

    #[test]
    fn the_row_menu_opens_a_draft_with_the_lines_and_a_reload_refreshes_the_streams() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path());
        let log = dir.path().join("app.log");
        write_lines(&log, &LINES);
        let mut app = FastTailApp::from_config(FastTailConfig {
            spool_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        });
        app.open_log_file(log.clone());
        app.engines[0].select_row(1);
        app.engines[0].format_request = Some(1);
        app.apply_merged_views();
        let dialog = app.log_formats.as_ref().expect("the dialog opened");
        assert_eq!(dialog.draft.samples, LINES[1]);
        assert_eq!(dialog.stream_lines.len(), LINES.len(), "the focused stream");

        // An unchanged format is kept as it is; a changed one is read with its new version,
        // on a stream that chose it and on one that detected it.
        let formats = dir.path().join("formats");
        write_lines(&dir.path().join("payments-1.log"), &LINES);
        app.open_log_file(dir.path().join("payments-1.log"));
        assert_eq!(app.engines.len(), 2);
        let auto = 1;
        assert_eq!(app.engines[auto].field_choice(), &ParserChoice::Auto);
        app.engines[auto].poll_updates();
        app.engines[0].set_field_choice(ParserChoice::Format("Payments".into()));
        let before = app.engines[0].field_parser().cloned().unwrap();
        app.reload_log_formats(&formats);
        let same = app.engines[0].field_parser().cloned().unwrap();
        assert!(std::sync::Arc::ptr_eq(&before, &same), "nothing to rebuild");
        let edited = PAYMENTS.replace("I=info", "I=debug");
        std::fs::write(formats.join("payments.fasttail-format.ini"), edited).unwrap();
        app.reload_log_formats(&formats);
        for idx in [0, auto] {
            match app.engines[idx].field_parser().map(|p| &**p) {
                Some(FieldParser::Format(f)) => {
                    assert!(f.spec.level_map.contains("I=debug"), "stream {idx}")
                }
                other => panic!("stream {idx}: {other:?}"),
            }
        }
        // Back as the other tests expect it.
        install(dir.path());
    }
}
