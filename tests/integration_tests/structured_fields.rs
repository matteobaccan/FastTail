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

const JSON_LOG: &[&str] = &[
    r#"{"ts":"2024-05-01T10:00:00Z","level":"info","msg":"started","user":"ann"}"#,
    r#"{"ts":"2024-05-01T10:00:01Z","level":"error","msg":"boom","user":"bob","extra":{"code":7}}"#,
    r#"{"ts":"2024-05-01T10:00:02Z","level":"warn","msg":"slow","user":"cy"}"#,
    r#"{"ts":"2024-05-01T10:00:03Z","level":"info","msg":"done","user":"dee"}"#,
    "not json at all",
];

#[test]
fn the_catalogue_is_sampled_and_gives_the_default_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.json");
    write_lines(&path, JSON_LOG);
    let mut engine = open(&path);
    engine.poll_updates();
    assert_eq!(
        engine.field_catalogue().keys(),
        ["ts", "level", "msg", "user", "extra.code"]
    );
    // The message field is the last column, not one of the defaults.
    assert_eq!(
        engine.field_columns(),
        ["ts", "level", "user", "extra.code"]
    );
    assert_eq!(engine.field_width("ts"), 20);
    assert_eq!(engine.field_width("level"), 5);
    assert!(engine.chosen_field_columns().is_empty());

    engine.set_field_columns(vec!["level".into(), "ts".into(), "level".into(), "".into()]);
    assert_eq!(engine.field_columns(), ["level", "ts"]);
    assert!(engine.fields_dirty);
    engine.set_field_width("ts", 1);
    assert_eq!(engine.field_width("ts"), fasttail::fields::MIN_WIDTH);
    engine.set_field_width("ts", 9999);
    assert_eq!(engine.field_width("ts"), fasttail::fields::MAX_WIDTH);
    engine.reset_field_columns();
    assert_eq!(
        engine.field_columns(),
        ["ts", "level", "user", "extra.code"]
    );
    assert_eq!(engine.field_width("ts"), 20);

    // Parsed rows are kept until the parser changes.
    let text = engine.get_line(1).unwrap().into_owned();
    let first = engine.row_fields(1, &text).unwrap();
    assert_eq!(first.get("extra.code"), Some("7"));
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &engine.row_fields(1, &text).unwrap()
    ));
    engine.set_field_choice(ParserChoice::Logfmt);
    let again = engine.row_fields(1, &text).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &again));
    engine.set_field_choice(ParserChoice::Off);
    assert!(engine.row_fields(1, &text).is_none());
    assert!(!engine.columns_shown());
}

fn app_frame(app: &mut FastTailApp) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(2400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
        out.textures_delta.clear();
        texts.clear();
        for clipped in &out.shapes {
            collect_texts(&clipped.shape, &mut texts);
        }
    }
    texts
}

fn collect_texts(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_texts(s, out)),
        _ => {}
    }
}

#[test]
fn the_column_view_draws_cells_a_header_and_the_message() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.json");
    write_lines(&log, JSON_LOG);
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    });
    app.open_log_file(log.clone());
    let texts = app_frame(&mut app);
    let has = |texts: &[String], s: &str| texts.iter().any(|t| t == s);
    assert!(has(&texts, "[+] JSON"), "text view first: {texts:?}");
    let lang = app.config.language;
    let columns = format!("▦ {}", fasttail::i18n::t(lang, "fields_columns"));
    assert!(has(&texts, &columns));

    app.engines[0].set_fields_view(true);
    let texts = app_frame(&mut app);
    let message = fasttail::i18n::t(lang, "fields_message");
    for cell in [
        "ts",
        "level",
        "user",
        "extra.code",
        message,
        "error",
        "bob",
        "7",
        "boom",
        "started",
    ] {
        assert!(has(&texts, cell), "{cell} missing in {texts:?}");
    }
    assert!(has(&texts, "2024-05-01T10:00:01Z"));
    assert!(
        has(&texts, "not json at all"),
        "an unparsed line runs across"
    );
    assert!(
        !has(&texts, "[+] JSON"),
        "no JSON expander in the column view"
    );

    // Fewer columns: the hidden fields join the message.
    app.engines[0].set_field_columns(vec!["level".into()]);
    let texts = app_frame(&mut app);
    assert!(
        has(&texts, "boom ts=2024-05-01T10:00:01Z user=bob extra.code=7"),
        "{texts:?}"
    );

    // Without a parser the rows are text again.
    app.engines[0].set_field_choice(ParserChoice::Off);
    let texts = app_frame(&mut app);
    assert!(has(&texts, "[+] JSON"));
}

#[test]
fn the_column_view_is_kept_in_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.json");
    write_lines(&log, JSON_LOG);
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    app.open_log_file(log.clone());
    app_frame(&mut app);
    {
        let engine = &mut app.engines[0];
        engine.set_fields_view(true);
        engine.set_field_columns(vec!["user".into(), "level".into()]);
        engine.set_field_width("user", 12);
    }
    app_frame(&mut app);
    let mut buf = Vec::new();
    app.config.to_ini().write_to(&mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    for key in [
        "fields_view=true",
        "fields_columns=user,level",
        "fields_width.user=12",
    ] {
        assert!(text.contains(key), "{key} in {text}");
    }
    let restored = FastTailConfig::from_ini(&app.config.to_ini());
    let app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..restored
    });
    let engine = app.engines.iter().find(|e| e.path == log).unwrap();
    assert!(engine.fields_view());
    assert_eq!(engine.chosen_field_columns(), ["user", "level"]);
    assert_eq!(engine.field_width("user"), 12);
    assert!(!engine.fields_dirty);
}
