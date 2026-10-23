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
