// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::field_stats::StatsSource;
use fasttail::fields::ParserChoice;
use fasttail::operations::{NameSource, OpMap, Operation};
use fasttail::tail_engine::TailEngine;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn open(path: &Path) -> TailEngine {
    let mut engine = TailEngine::open(path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    engine
}

fn wait_ops(engine: &mut TailEngine) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.poll_updates();
        let o = engine.operations.as_ref().unwrap();
        if o.done() && o.covered_all(engine.total_lines()) {
            return;
        }
        assert!(Instant::now() < deadline, "operations did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn op<'a>(engine: &'a TailEngine, id: &str) -> &'a Operation {
    engine
        .operations
        .as_ref()
        .unwrap()
        .map
        .get(id)
        .unwrap_or_else(|| panic!("no operation {id}"))
}

fn map(engine: &TailEngine) -> &OpMap {
    &engine.operations.as_ref().unwrap().map
}

/// Interleaved requests: a and c fast, b slow with an error and a stack trace.
const REQUESTS: [&str; 9] = [
    "2026-10-28T10:00:00.000Z INFO req=a GET /orders",
    "2026-10-28T10:00:00.100Z INFO req=b POST /pay",
    "2026-10-28T10:00:00.200Z INFO req=a done",
    "2026-10-28T10:00:01.000Z ERROR req=b gateway timeout",
    "    at Gateway.call(Gateway.java:42)",
    "2026-10-28T10:00:01.050Z INFO healthcheck ok",
    "2026-10-28T10:00:03.000Z INFO req=b done",
    "2026-10-28T10:00:03.500Z INFO req=c GET /health",
    "2026-10-28T10:00:03.400Z INFO req=c done",
];

#[test]
fn entries_are_grouped_by_id_with_span_duration_and_worst_level() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    write_lines(&path, &REQUESTS);
    let mut engine = open(&path);
    let re = regex::Regex::new(r"req=(\w+)").unwrap();
    engine.open_operations(StatsSource::Capture(re), NameSource::Message);
    wait_ops(&mut engine);
    assert_eq!(map(&engine).len(), 3);
    let b = op(&engine, "b");
    assert_eq!(b.duration(), Some(2_900));
    // The stack trace line belongs to b's entry.
    assert_eq!(
        (b.lines, b.entries, b.first_line, b.last_line),
        (4, 3, 1, 6)
    );
    assert_eq!(b.worst, 5, "ERROR is its worst level");
    assert_eq!(b.name.as_ref(), "INFO req=b POST /pay");
    // c's timestamps go back: the earliest is its start.
    let c = op(&engine, "c");
    assert_eq!(c.duration(), Some(100));
    assert_eq!(op(&engine, "a").duration(), Some(200));
    // The health check has no id: it is no operation.
    assert_eq!(map(&engine).lines, 9);

    // A filter that hides a request's lines drops it.
    engine.set_exclude_filter("GET");
    wait_ops(&mut engine);
    assert!(map(&engine).get("a").is_some(), "a keeps its done line");
    engine.set_exclude_filter("req=c");
    wait_ops(&mut engine);
    assert!(map(&engine).get("c").is_none());
    engine.set_exclude_filter("");
    wait_ops(&mut engine);
    assert_eq!(map(&engine).len(), 3);

    // Appended lines extend the operations in place, continuation lines included.
    let generation = engine.operations.as_ref().unwrap().generation;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "2026-10-28T10:00:09.000Z WARN req=b retried").unwrap();
    writeln!(f, "    at Retry.run(Retry.java:7)").unwrap();
    writeln!(f, "2026-10-28T10:00:10.000Z INFO req=d GET /new").unwrap();
    drop(f);
    let deadline = Instant::now() + Duration::from_secs(30);
    while engine.total_lines() < 12 {
        assert!(Instant::now() < deadline, "the append was not read");
        engine.poll_updates();
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_ops(&mut engine);
    let o = engine.operations.as_ref().unwrap();
    assert!(o.generation > generation);
    let b = op(&engine, "b");
    assert_eq!((b.duration(), b.lines), (Some(8_900), 6));
    assert_eq!(op(&engine, "d").lines, 1);
}

#[test]
fn a_field_source_reads_the_parser_and_the_scan_matches_line_by_line_adds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("api.log");
    let lines: Vec<String> = (0..3000)
        .map(|i| {
            format!(
                "ts=2026-10-28T10:{:02}:{:02}Z level={} trace={} msg=step{}",
                i / 60 % 60,
                i % 60,
                if i % 97 == 0 { "error" } else { "info" },
                i % 250,
                i
            )
        })
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = open(&path);
    engine.set_field_choice(ParserChoice::Logfmt);
    super::wait_for_jobs(&mut engine);
    engine.open_operations(
        StatsSource::Field("trace".into()),
        NameSource::Field("msg".into()),
    );
    wait_ops(&mut engine);
    let scanned = map(&engine).clone();
    assert_eq!(scanned.len(), 250);
    let t0 = scanned.get("0").unwrap();
    assert_eq!(
        (t0.lines, t0.first_line, t0.name.as_ref()),
        (12, 0, "step0")
    );
    assert_eq!(t0.worst, 5, "line 0 is an error");
    // Lines 0, 250, ..., 2750: 10:00:00 to 10:45:50 read from the `ts` field.
    assert_eq!(t0.duration(), Some(2_750_000));
    // The same operations, built line by line as appends are (times aside).
    let mut by_hand = OpMap::default();
    for (i, l) in lines.iter().enumerate() {
        let trace = l.split("trace=").nth(1).unwrap().split(' ').next().unwrap();
        let level = if l.contains("level=error") { 5 } else { 3 };
        by_hand.add_entry(trace, || Box::from(format!("step{i}")), i, 0, level);
    }
    let untimed = |m: &OpMap| -> Vec<(String, String, usize, usize, u64, u8)> {
        m.ops
            .iter()
            .map(|o| {
                (
                    o.id.to_string(),
                    o.name.to_string(),
                    o.first_line,
                    o.last_line,
                    o.lines,
                    o.worst,
                )
            })
            .collect()
    };
    assert_eq!(untimed(&scanned), untimed(&by_hand));
}

#[test]
fn ids_past_the_cap_are_counted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ids.log");
    let n = fasttail::operations::MAX_OPERATIONS + 1500;
    let lines: Vec<String> = (0..n).map(|i| format!("INFO id={i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = open(&path);
    let re = regex::Regex::new(r"id=(\d+)").unwrap();
    engine.open_operations(StatsSource::Capture(re), NameSource::None);
    wait_ops(&mut engine);
    let m = map(&engine);
    assert_eq!(
        (m.len(), m.untracked()),
        (fasttail::operations::MAX_OPERATIONS, 1500)
    );
}

#[test]
fn the_tab_draws_rows_and_bars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    write_lines(&path, &REQUESTS);
    let mut engine = open(&path);
    let re = regex::Regex::new(r"req=(\w+)").unwrap();
    engine.open_operations(StatsSource::Capture(re), NameSource::Message);
    wait_ops(&mut engine);
    let theme = fasttail::theme::CyberTheme::Tron;
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            fasttail::ui::operations_tab::render(
                ui,
                Some(&mut engine),
                &theme,
                fasttail::i18n::Language::En,
            );
        });
        output.textures_delta.clear();
    }
    assert_eq!(
        fasttail::ui::operations_tab::title(Some(&engine), fasttail::i18n::Language::En),
        "⏱ Operations · app.log"
    );
}
