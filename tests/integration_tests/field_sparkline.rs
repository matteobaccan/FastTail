// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::fields::ParserChoice;
use fasttail::plot::{PlotSpec, PlotStat};
use fasttail::tail_engine::TailEngine;
use std::io::Write;
use std::time::{Duration, Instant};

/// Ten minutes of requests, one per second: 10 ms, then 500 ms in minute 5.
fn latency_log(seconds: usize) -> Vec<String> {
    (0..seconds)
        .map(|s| {
            let ms = if (300..360).contains(&s) { 500 } else { 10 };
            let path = if s % 2 == 0 { "/a" } else { "/b" };
            format!(
                "ts=2026-10-24T10:{:02}:{:02}Z level=info path={path} took={ms}ms size={}KB",
                s / 60 % 60,
                s % 60,
                s % 4
            )
        })
        .collect()
}

fn open(lines: &[String]) -> (tempfile::TempDir, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("access.log");
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    engine.set_field_choice(ParserChoice::Logfmt);
    super::wait_for_jobs(&mut engine);
    (dir, engine)
}

fn wait_plot(engine: &mut TailEngine, lines: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.poll_updates();
        let p = engine.plot.as_ref().unwrap();
        let total: u64 = p
            .series
            .first()
            .map_or(0, |s| s.agg_between(i64::MIN / 2, i64::MAX / 2).count);
        if !p.running() && total as usize >= lines {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "plot did not finish: running {} total {total} series {} skipped {:?} lines {}",
            p.running(),
            p.series.len(),
            p.series.iter().map(|s| s.skipped).collect::<Vec<_>>(),
            engine.total_lines()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Milliseconds of 10:mm:ss UTC on the test day.
fn at(m: i64, s: i64) -> i64 {
    1792836000000 + (m * 60 + s) * 1000
}

#[test]
fn a_latency_field_is_aggregated_per_time_and_follows_filters_and_appends() {
    let lines = latency_log(600);
    let (dir, mut engine) = open(&lines);
    assert!(engine.add_plot("took"));
    assert!(!engine.add_plot("took"), "plotted once");
    wait_plot(&mut engine, 600);
    let s = &engine.plot.as_ref().unwrap().series[0];
    assert_eq!(s.unit(), "ms");
    assert!(!s.mixed());
    assert_eq!(s.value_between(at(5, 0), at(6, 0)), Some(500.0));
    assert_eq!(s.value_between(at(0, 0), at(1, 0)), Some(10.0));
    let whole = s.agg_between(at(0, 0), at(10, 0));
    assert_eq!((whole.count, whole.min, whole.max), (600, 10.0, 500.0));
    // Max of a minute holding both.
    engine.set_plot_stat("took", PlotStat::Max);
    wait_plot(&mut engine, 600);
    let s = &engine.plot.as_ref().unwrap().series[0];
    assert_eq!(s.value_between(at(4, 30), at(5, 30)), Some(500.0));

    // Only the lines the filters show: half the requests.
    engine.set_include_filter("path=/a");
    super::wait_for_jobs(&mut engine);
    wait_plot(&mut engine, 300);
    let s = &engine.plot.as_ref().unwrap().series[0];
    assert_eq!(s.agg_between(at(0, 0), at(10, 0)).count, 300);
    engine.set_include_filter("");
    super::wait_for_jobs(&mut engine);
    wait_plot(&mut engine, 600);

    // A growing file: appended lines are added.
    {
        let path = dir.path().join("access.log");
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(
            f,
            "ts=2026-10-24T10:10:00Z level=info path=/a took=2s size=1KB"
        )
        .unwrap();
    }
    wait_plot(&mut engine, 601);
    let s = &engine.plot.as_ref().unwrap().series[0];
    assert_eq!(s.value_between(at(10, 0), at(10, 1)), Some(2000.0));

    // Sizes in bytes, a second field beside the first.
    assert!(engine.add_plot("size"));
    wait_plot(&mut engine, 601);
    let p = engine.plot.as_ref().unwrap();
    assert_eq!(p.series[1].unit(), "B");
    assert_eq!(
        PlotSpec::list_text(engine.plots()),
        "took:max,size:avg",
        "what plot= saves"
    );
    engine.remove_plot("took");
    engine.remove_plot("size");
    assert!(engine.plot.is_none());
}
