// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::config::FastTailConfig;
use fasttail::fields::{FieldParser, ParserChoice, DETECT_LINES};
use fasttail::tail_engine::TailEngine;
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::path::Path;

fn open(path: &Path) -> TailEngine {
    let mut engine = TailEngine::open(path).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    wait_for_jobs(&mut engine);
    engine
}

fn append(path: &Path, lines: &[String]) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

fn kind(engine: &TailEngine) -> &'static str {
    match engine.field_parser().map(|p| &**p) {
        Some(FieldParser::Json) => "json",
        Some(FieldParser::Logfmt) => "logfmt",
        Some(FieldParser::Regex(_)) => "regex",
        None => "none",
    }
}

#[test]
fn json_and_logfmt_streams_are_detected_plain_logs_are_not() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("app.json");
    write_lines(
        &json,
        &[
            r#"{"ts":"2024-05-01T10:00:00Z","level":"info","msg":"start"}"#,
            r#"{"ts":"2024-05-01T10:00:01Z","level":"error","msg":"boom"}"#,
            "    at com.example.Main.run(Main.java:10)",
        ],
    );
    let engine = open(&json);
    assert_eq!(kind(&engine), "json");
    assert_eq!(engine.field_choice(), &ParserChoice::Auto);
    assert_eq!(engine.field_auto(), Some(&ParserChoice::Json));

    let logfmt = dir.path().join("app.log");
    write_lines(&logfmt, &["ts=1 level=info msg=\"a b\" status=200"; 3]);
    assert_eq!(kind(&open(&logfmt)), "logfmt");

    let plain = dir.path().join("plain.log");
    write_lines(
        &plain,
        &["2024-05-01 10:00:00 INFO started", "retry=0 once"],
    );
    let engine = open(&plain);
    assert_eq!(kind(&engine), "none");
    assert_eq!(engine.field_auto(), None);
}

#[test]
fn a_short_stream_is_detected_again_once_at_200_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("grow.log");
    write_lines(&path, &["booting", "still booting"]);
    let mut engine = open(&path);
    assert_eq!(kind(&engine), "none");
    let generation = engine.parser_generation;

    let json: Vec<String> = (0..DETECT_LINES)
        .map(|i| format!(r#"{{"i":{i},"level":"info"}}"#))
        .collect();
    append(&path, &json);
    engine.poll_updates();
    wait_for_jobs(&mut engine);
    assert!(engine.total_lines() >= DETECT_LINES);
    assert_eq!(kind(&engine), "json", "re-detected on reaching 200 lines");
    assert_ne!(engine.parser_generation, generation);
    assert!(!engine.fields_dirty, "detection is not a user choice");

    // Never again: plain lines appended later do not switch the parser off.
    let plain: Vec<String> = (0..2 * DETECT_LINES)
        .map(|i| format!("plain {i}"))
        .collect();
    append(&path, &plain);
    engine.poll_updates();
    wait_for_jobs(&mut engine);
    assert_eq!(kind(&engine), "json");
}

#[test]
fn a_forced_parser_wins_over_detection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.json");
    write_lines(&path, &[r#"{"level":"info"}"#; 3]);
    let mut engine = open(&path);
    assert_eq!(kind(&engine), "json");

    engine.set_field_choice(ParserChoice::Off);
    assert_eq!(kind(&engine), "none");
    assert!(engine.fields_dirty);
    engine.poll_updates();
    assert_eq!(kind(&engine), "none", "off is not detected again");

    engine.set_field_choice(ParserChoice::Regex("(unnamed)".into()));
    assert_eq!(kind(&engine), "none");
    assert!(engine.field_error().is_some());
    engine.set_field_choice(ParserChoice::Regex(r#"^\{"level":"(?P<level>\w+)""#.into()));
    assert_eq!(kind(&engine), "regex");
    assert_eq!(engine.field_error(), None);

    engine.set_field_choice(ParserChoice::Syslog);
    assert_eq!(kind(&engine), "regex");
    engine.set_field_choice(ParserChoice::Auto);
    assert_eq!(kind(&engine), "json", "back to auto detects again");
}

#[test]
fn the_forced_parser_is_kept_in_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &["WARN: disk low", "INFO: ok"]);
    let config = || FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config());
    app.open_log_file(log.clone());
    let frame = |app: &mut FastTailApp| {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
        out.textures_delta.clear();
    };
    frame(&mut app);
    // Auto writes no key.
    let mut buf = Vec::new();
    app.config.to_ini().write_to(&mut buf).unwrap();
    assert!(!String::from_utf8_lossy(&buf).contains("fields_"));

    let pattern = r"^(?P<level>[A-Z]+): (?P<msg>.*)";
    app.engines[0].set_field_choice(ParserChoice::Regex(pattern.into()));
    frame(&mut app);
    let entry = app.config.stream_state_for(&log).unwrap();
    assert_eq!(entry.fields_parser.as_deref(), Some("regex"));
    assert_eq!(entry.fields_regex.as_deref(), Some(pattern));

    let restored = FastTailConfig::from_ini(&app.config.to_ini());
    let app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..restored
    });
    let engine = app.engines.iter().find(|e| e.path == log).unwrap();
    assert_eq!(engine.field_choice(), &ParserChoice::Regex(pattern.into()));
    assert_eq!(kind(engine), "regex");
    assert!(!engine.fields_dirty);
}
