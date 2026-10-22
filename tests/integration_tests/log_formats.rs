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
