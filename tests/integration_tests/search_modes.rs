// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::tail_engine::{SearchMode, TailEngine};

fn log() -> Vec<String> {
    (0..400)
        .map(|i| match i % 4 {
            0 => format!("INFO req={i} took {}ms", i * 3),
            1 => format!("WARN Disk at {}%", i % 100),
            2 => format!("error code=E{:03}", i % 50),
            _ => format!("ERROR code=E{:03} fatal", i % 50),
        })
        .collect()
}

fn open(lines: &[String], threshold: u64) -> (tempfile::TempDir, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.log");
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.job_threshold_bytes = threshold;
    wait_for_jobs(&mut engine);
    (dir, engine)
}

fn search(engine: &mut TailEngine, query: &str) -> Vec<usize> {
    engine.search_query = query.to_string();
    engine.update_search(query);
    wait_for_jobs(engine);
    engine.search_matches.clone()
}

#[test]
fn regex_search_matches_lines_on_the_ui_thread_and_in_a_job_alike() {
    let lines = log();
    let expected: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            regex::Regex::new(r"(?i)code=E0[0-4]\d")
                .unwrap()
                .is_match(l)
        })
        .map(|(i, _)| i)
        .collect();
    assert!(!expected.is_empty());
    for threshold in [u64::MAX, 0] {
        let (_dir, mut engine) = open(&lines, threshold);
        engine.set_search_mode(SearchMode::Regex, false);
        assert_eq!(
            search(&mut engine, r"code=e0[0-4]\d"),
            expected,
            "threshold {threshold}"
        );
        assert_eq!(engine.search_total(), expected.len());
        // Case: with Aa on, `e` no longer matches `E`.
        engine.set_search_mode(SearchMode::Regex, true);
        wait_for_jobs(&mut engine);
        assert!(engine.search_matches.is_empty(), "threshold {threshold}");
        // The text mode reads the same query as text again.
        engine.set_search_mode(SearchMode::Text, false);
        wait_for_jobs(&mut engine);
        assert!(engine.search_matches.is_empty());
        assert_eq!(search(&mut engine, "Disk at").len(), 100);
    }
}

#[test]
fn an_invalid_regex_keeps_the_last_hits_and_says_why() {
    let (_dir, mut engine) = open(&log(), u64::MAX);
    engine.set_search_mode(SearchMode::Regex, false);
    let hits = search(&mut engine, r"took \d+ms");
    assert_eq!(hits.len(), 100);
    assert!(engine.search_error.is_none());
    search(&mut engine, r"took (\d+ms");
    assert!(engine.search_error.is_some());
    assert_eq!(engine.search_matches, hits, "the last valid search stays");
    search(&mut engine, r"took (\d+)ms");
    assert!(engine.search_error.is_none());
    assert_eq!(engine.search_matches, hits);
    // The matcher tints what the regex finds.
    let m = engine.search_matcher().unwrap();
    assert_eq!(
        m.ranges("x took 12ms took 3ms", 10),
        vec![(2, 11), (12, 20)]
    );
}

fn tx_log() -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..300 {
        lines.push(format!("BEGIN tx{i}"));
        lines.push("noise".to_string());
        if i % 3 == 0 {
            lines.push(format!("deadlock in tx{i}"));
        } else {
            lines.push("commit".to_string());
        }
    }
    lines
}

#[test]
fn multiline_hits_cross_lines_alike_on_the_ui_thread_and_in_a_job() {
    let lines = tx_log();
    let expected: Vec<(usize, usize)> = (0..300)
        .filter(|i| i % 3 == 0)
        .map(|i| (i * 3, i * 3 + 2))
        .collect();
    for threshold in [u64::MAX, 0] {
        let (_dir, mut engine) = open(&lines, threshold);
        engine.set_search_mode(SearchMode::Multiline, false);
        let firsts = search(&mut engine, r"^BEGIN tx\d+\n.*\ndeadlock");
        let pairs: Vec<(usize, usize)> = firsts
            .iter()
            .copied()
            .zip(engine.search_ends.iter().copied())
            .collect();
        assert_eq!(pairs, expected, "threshold {threshold}");
        assert_eq!(engine.search_total(), 100);
        // The rows a hit continues on are marked too, the others not.
        assert!(engine.search_hit_covers(1) && engine.search_hit_covers(2));
        assert!(!engine.search_hit_covers(3));
        // Continuation rows are told apart from a hit's first row; the list says `+2`.
        assert!(!engine.search_hit_continues(0));
        assert!(engine.search_hit_continues(1) && engine.search_hit_continues(2));
        assert_eq!(engine.search_hit_extra_lines(0), 2);
        // Hidden lines are not part of the text: without the noise lines the pattern
        // with one line between no longer matches, the one without does.
        engine.set_exclude_filter("noise");
        wait_for_jobs(&mut engine);
        // The filter change runs the same query again.
        assert!(engine.search_matches.is_empty(), "threshold {threshold}");
        engine.search_query.clear();
        engine.update_search("");
        let gap = search(&mut engine, r"^BEGIN tx\d+\ndeadlock");
        assert_eq!(gap.len(), 100, "threshold {threshold}");
        assert_eq!(engine.search_ends[0], 2, "the hit spans the hidden line");
        // Counted in visible lines: the hidden line between is not one of them.
        assert_eq!(engine.search_hit_extra_lines(0), 1);
    }
}

#[test]
fn a_multiline_hit_across_the_old_end_is_found_once_and_long_matches_are_counted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("grow.log");
    write_lines(&path, &["BEGIN tx1", "deadlock", "BEGIN tx2"]);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    wait_for_jobs(&mut engine);
    engine.set_search_mode(SearchMode::Multiline, false);
    assert_eq!(search(&mut engine, r"BEGIN tx\d\ndeadlock"), vec![0]);
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "deadlock").unwrap();
    drop(f);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while engine.total_lines() < 4 {
        engine.poll_updates();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    wait_for_jobs(&mut engine);
    assert_eq!(engine.search_matches, vec![0, 2]);
    assert_eq!(engine.search_ends, vec![1, 3]);
    assert_eq!(engine.search_total(), 2);

    // A match over more than 64 lines is counted, not listed.
    let mut lines = vec!["A".to_string()];
    lines.extend((0..80).map(|i| format!("x{i}")));
    lines.push("B".to_string());
    let (_d, mut long) = open(&lines, u64::MAX);
    long.set_search_mode(SearchMode::Multiline, false);
    assert!(search(&mut long, r"(?s)A.*B").is_empty());
    assert_eq!(long.search_skipped, 1);
}
