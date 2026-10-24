// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::spike::Baseline;
use fasttail::tail_engine::TailEngine;
use std::time::{Duration, Instant};

/// Ten minutes of heartbeats, and a burst of payment errors in minute 5.
fn burst_log() -> Vec<String> {
    let mut lines = Vec::new();
    for s in 0..600 {
        let ts = format!("2026-10-24T10:{:02}:{:02}Z", s / 60, s % 60);
        lines.push(format!("{ts} INFO heartbeat {s} ok"));
        if (300..360).contains(&s) {
            for k in 0..3 {
                lines.push(format!("{ts} ERROR payment {s}{k} failed: gateway timeout"));
                lines.push("    at com.example.Pay.charge(Pay.java:42)".to_string());
            }
        }
    }
    lines
}

fn open(lines: &[String]) -> (tempfile::TempDir, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    (dir, engine)
}

fn wait_explain(engine: &mut TailEngine) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while engine.explain.as_ref().is_some_and(|e| e.running()) {
        assert!(Instant::now() < deadline, "explain did not finish");
        engine.poll_updates();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Milliseconds of `10:mm:ss` UTC on the test day.
fn at(m: i64, s: i64) -> i64 {
    1_792_836_000_000 + (m * 60 + s) * 1000
}

#[test]
fn a_burst_is_explained_against_the_rest_and_the_preceding_minute() {
    let lines = burst_log();
    let (_dir, mut engine) = open(&lines);
    engine.explain_span((at(5, 0), at(5, 59) + 999), Baseline::Rest, false);
    wait_explain(&mut engine);
    let e = engine.explain.as_ref().unwrap();
    assert!(!e.failed);
    let r = e.result.as_ref().unwrap();
    // 60 heartbeats and 180 errors in the span; the stack lines are not entries.
    assert_eq!(r.entries[0], 240);
    assert_eq!(r.entries[1], 540);
    let msgs = r.messages();
    let (top, first) = &msgs[0];
    assert_eq!(
        r.drain.template(top.item).unwrap().text(),
        "ERROR payment <*> failed: gateway timeout"
    );
    assert_eq!(top.span, 180);
    assert!(top.ratio.is_none(), "absent from the baseline: new");
    assert!(lines[*first].contains("payment"));
    assert!(msgs.iter().all(|(m, _)| !r
        .drain
        .template(m.item)
        .unwrap()
        .text()
        .contains("heartbeat")));
    // ERROR is level 5.
    assert_eq!(r.levels[0][5], 180);
    assert_eq!(r.levels[1][5], 0);

    engine.recompute_explain(Baseline::Preceding, false);
    wait_explain(&mut engine);
    let r = engine.explain.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(r.entries[1], 60, "the minute before: its heartbeats");
}

#[test]
fn only_filtered_lines_restricts_both_sides() {
    let lines = burst_log();
    let (_dir, mut engine) = open(&lines);
    engine.set_include_filter("heartbeat");
    super::wait_for_jobs(&mut engine);
    engine.explain_span((at(5, 0), at(5, 59) + 999), Baseline::Rest, true);
    wait_explain(&mut engine);
    let r = engine.explain.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(r.entries, [60, 540]);
    assert!(
        r.messages().is_empty(),
        "heartbeats only: nothing stands out"
    );
}

#[test]
fn field_values_that_stand_out_are_listed_and_ids_are_skipped() {
    use fasttail::fields::ParserChoice;
    let mut lines = Vec::new();
    for s in 0..600 {
        let status = if (300..360).contains(&s) && s % 4 != 0 {
            503
        } else {
            200
        };
        lines.push(format!(
            "ts=2026-10-24T10:{:02}:{:02}Z level=info status={status} req=r{s} msg=done",
            s / 60,
            s % 60
        ));
    }
    let (_dir, mut engine) = open(&lines);
    engine.set_field_choice(ParserChoice::Logfmt);
    super::wait_for_jobs(&mut engine);
    engine.explain_span((at(5, 0), at(5, 59) + 999), Baseline::Rest, false);
    wait_explain(&mut engine);
    let r = engine.explain.as_ref().unwrap().result.as_ref().unwrap();
    let pairs = r.field_pairs();
    assert_eq!(pairs[0].item, ("status".to_string(), "503".to_string()));
    assert_eq!(pairs[0].span, 45);
    assert!(pairs[0].ratio.is_none());
    // A request id per line and a time per line: not differences.
    assert!(pairs.iter().all(|p| p.item.0 != "req" && p.item.0 != "ts"));
}
