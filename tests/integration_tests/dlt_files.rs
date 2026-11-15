// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::fields::{row_fields, FieldSpans};
use fasttail::log_level::LogLevel;
use fasttail::tail_engine::TailEngine;
use fasttail::workspace::{open_target, OpenOutcome};
use std::time::{Duration, Instant};

/// A stored verbose log message from `apid` / `ctid` with one string argument.
fn message(seconds: u32, level: u8, apid: &[u8; 4], ctid: &[u8; 4], text: &str) -> Vec<u8> {
    let mut msg = vec![0x01 | 0x04 | (1 << 5), 0, 0, 0];
    msg.extend_from_slice(b"ECU1");
    msg.push(0x01 | (level << 4));
    msg.push(1);
    msg.extend_from_slice(apid);
    msg.extend_from_slice(ctid);
    msg.extend_from_slice(&0x200u32.to_le_bytes());
    msg.extend_from_slice(&((text.len() + 1) as u16).to_le_bytes());
    msg.extend_from_slice(text.as_bytes());
    msg.push(0);
    let len = msg.len() as u16;
    msg[2..4].copy_from_slice(&len.to_be_bytes());
    let mut out = b"DLT\x01".to_vec();
    out.extend_from_slice(&seconds.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"ECU1");
    out.extend_from_slice(&msg);
    out
}

fn wait(engine: &mut TailEngine, done: impl Fn(&TailEngine) -> bool) {
    let started = Instant::now();
    while !done(engine) {
        engine.poll_updates();
        assert!(started.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_dlt_file_opens_as_rows_with_level_time_and_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("trace.dlt");
    let mut bytes = message(1_763_200_800, 4, b"ENG\0", b"MAIN", "engine started");
    bytes.extend(message(
        1_763_200_801,
        2,
        b"BRK\0",
        b"ABS\0",
        "pressure low",
    ));
    std::fs::write(&path, bytes).unwrap();
    let config = fasttail::config::FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let mut engine = match open_target(&path, &config, None) {
        OpenOutcome::Opened(engine) => *engine,
        _ => panic!("the DLT file opens"),
    };
    assert!(engine.dlt.is_some());
    assert_eq!(engine.path, path);
    assert_eq!(fasttail::stream_info::source_kind(&engine), "info_kind_dlt");
    engine.size_check_interval = Duration::ZERO;
    wait(&mut engine, |e| e.total_lines() >= 2);
    assert_eq!(engine.level_of(0), LogLevel::Info);
    assert_eq!(engine.level_of(1), LogLevel::Error);
    let line = engine.get_line(1).unwrap().to_string();
    assert!(
        line.starts_with("2025-11-15T10:00:01.000000Z ERROR"),
        "{line}"
    );
    wait(&mut engine, |e| e.field_parser().is_some());
    assert!(
        matches!(
            engine.field_parser().map(|p| p.as_ref()),
            Some(fasttail::fields::FieldParser::Logfmt)
        ),
        "the logfmt parser is detected"
    );
    let parser = fasttail::fields::FieldParser::Logfmt;
    let fields = row_fields(&parser, &line, &mut FieldSpans::new());
    assert_eq!(fields.get("apid"), Some("BRK"));
    assert_eq!(fields.get("ctid"), Some("ABS"));
    assert_eq!(fields.get("msg"), Some("pressure low"));
}
