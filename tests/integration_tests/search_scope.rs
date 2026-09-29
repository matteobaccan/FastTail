use super::{wait_for_jobs, write_lines};
use fasttail::tail_engine::{SearchScope, TailEngine};
use std::io::Write;
use std::path::Path;

fn append(path: &Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

/// `n` lines, every one a hit for "hit": `hit 0` .. `hit n-1`.
fn hits_log(n: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("scope.log");
    let lines: Vec<String> = (0..n).map(|i| format!("hit {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&log, &refs);
    (dir, log)
}

fn lines(first: usize, last: Option<usize>) -> SearchScope {
    SearchScope::Lines { first, last }
}

#[test]
fn typed_line_ranges_parse_one_based() {
    assert_eq!(
        SearchScope::parse_lines("1200-5000"),
        Some(lines(1199, Some(4999)))
    );
    assert_eq!(
        SearchScope::parse_lines(" 1,200 - "),
        Some(lines(1199, None))
    );
    assert_eq!(
        SearchScope::parse_lines("-5000"),
        Some(lines(0, Some(4999)))
    );
    assert_eq!(SearchScope::parse_lines("7"), Some(lines(6, Some(6))));
    assert_eq!(
        SearchScope::parse_lines("1.200–1.300"),
        Some(lines(1199, Some(1299)))
    );
    for bad in ["", "-", "0", "0-4", "9-3", "abc", "3-x"] {
        assert_eq!(SearchScope::parse_lines(bad), None, "{bad:?}");
    }
}

#[test]
fn a_line_scope_bounds_hits_counter_and_wrapping() {
    let (_dir, log) = hits_log(100);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.update_search("hit");
    assert_eq!(engine.search_total(), 100);

    assert_eq!(engine.set_search_scope(lines(10, Some(19))), (true, true));
    assert_eq!(engine.search_total(), 10);
    assert_eq!(engine.search_matches, (10..20).collect::<Vec<_>>());
    assert_eq!(engine.current_search_line(), Some(10));
    for want in 11..20 {
        assert_eq!(engine.search_next(false), Some(want));
    }
    assert_eq!(
        engine.search_next(false),
        Some(10),
        "wraps inside the scope"
    );
    assert_eq!(engine.search_prev(false), Some(19));

    // Back to the whole view.
    engine.set_search_scope(SearchScope::All);
    assert_eq!(engine.search_total(), 100);
}

#[test]
fn an_open_line_scope_grows_with_appends_a_closed_one_does_not() {
    let (_dir, log) = hits_log(20);
    let mut open = TailEngine::open(&log).unwrap();
    open.update_search("hit");
    open.set_search_scope(lines(15, None));
    let mut closed = TailEngine::open(&log).unwrap();
    closed.update_search("hit");
    closed.set_search_scope(lines(15, Some(19)));
    assert_eq!((open.search_total(), closed.search_total()), (5, 5));

    append(&log, "hit 20\nhit 21\nmiss\n");
    open.poll_updates();
    closed.poll_updates();
    assert_eq!(open.search_total(), 7);
    assert_eq!(closed.search_total(), 5);
    assert!(!open.search_scope_reset && !closed.search_scope_reset);
}

#[test]
fn a_scope_stays_within_the_filter() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("f.log");
    write_lines(
        &log,
        &[
            "ERROR hit 0",
            "INFO hit 1",
            "ERROR hit 2",
            "INFO hit 3",
            "ERROR hit 4",
        ],
    );
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_include_filter("ERROR");
    engine.update_search("hit");
    engine.set_search_scope(lines(1, Some(3)));
    assert_eq!(
        engine.search_matches,
        vec![2],
        "hidden lines are never hits"
    );
}

#[test]
fn a_time_scope_counts_continuation_lines_and_skips_untimed_ones() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("t.log");
    write_lines(
        &log,
        &[
            "banner hit, no time",
            "2026-09-18T14:00:00.000Z INFO hit early",
            "2026-09-18T14:05:00.000Z ERROR hit inside",
            "    at hit.Frame(Frame.java:1)",
            "2026-09-18T14:10:59.000Z INFO hit end of the minute",
            "2026-09-18T14:11:00.000Z INFO hit late",
        ],
    );
    let mut engine = TailEngine::open(&log).unwrap();
    engine.update_search("hit");
    assert_eq!(engine.search_total(), 6);
    let scope = SearchScope::Time {
        from: "14:05".into(),
        to: "14:10".into(),
    };
    assert_eq!(engine.set_search_scope(scope), (true, true));
    assert_eq!(engine.search_matches, vec![2, 3, 4]);

    // A side that does not parse is refused and changes nothing.
    let bad = SearchScope::Time {
        from: "yesterday-ish".into(),
        to: String::new(),
    };
    assert_eq!(engine.set_search_scope(bad), (false, true));
    assert_eq!(engine.search_matches, vec![2, 3, 4]);

    // Both sides empty: the whole view.
    let empty = SearchScope::Time {
        from: " ".into(),
        to: String::new(),
    };
    engine.set_search_scope(empty);
    assert!(engine.search_scope().is_all());
    assert_eq!(engine.search_total(), 6);
}

#[test]
fn a_time_scope_waits_for_the_background_timing() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("big.log");
    super::background_timestamps::write_timed_log(&log, 20_000);
    let mut engine = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut engine);
    engine.update_search("ERROR");
    wait_for_jobs(&mut engine);
    let everywhere = engine.search_total();
    engine.set_search_scope(SearchScope::Time {
        from: "10:01".into(),
        to: "10:01".into(),
    });
    wait_for_jobs(&mut engine);
    assert!(engine.timestamps_complete());
    // Entries 60..119 are stamped 10:01:xx; every tenth is an ERROR (none of those is
    // one of the entries stamped a minute early).
    assert_eq!(engine.search_total(), 6, "of {everywhere}");
}

#[test]
fn synchronous_and_background_scoped_searches_agree() {
    let (_dir, log) = hits_log(5_000);
    let mut sync = TailEngine::open(&log).unwrap();
    let mut bg = TailEngine::open_with_thresholds(&log, 0, u64::MAX).unwrap();
    wait_for_jobs(&mut bg);
    for engine in [&mut sync, &mut bg] {
        engine.update_search("hit 1");
        engine.set_search_scope(lines(100, None));
        wait_for_jobs(engine);
    }
    assert_eq!(sync.search_matches, bg.search_matches);
    assert_eq!(sync.search_total(), bg.search_total());
    assert!(sync.search_matches.iter().all(|&i| i >= 100));
    assert_eq!(sync.search_total(), 1_111 - 11);
}

#[test]
fn a_small_scope_set_while_a_whole_file_search_runs_drops_that_job() {
    let (_dir, log) = hits_log(200_000);
    // The whole file goes to a job, a 20-line scope is searched on the spot.
    let mut engine = TailEngine::open_with_thresholds(&log, 64 * 1024, u64::MAX).unwrap();
    engine.update_search("hit");
    assert!(engine.scan_progress().is_some(), "a search job runs");
    engine.set_search_scope(lines(100, Some(119)));
    wait_for_jobs(&mut engine);
    assert_eq!(engine.search_matches, (100..120).collect::<Vec<_>>());
    assert_eq!(engine.search_total(), 20);

    // A line scope too large for the UI thread scans from its first line.
    engine.set_search_scope(lines(150_000, None));
    wait_for_jobs(&mut engine);
    assert_eq!(engine.search_total(), 50_000);
    assert_eq!(engine.search_matches.first(), Some(&150_000));
}

#[test]
fn selection_bounds_without_listing_the_selection() {
    let (_dir, log) = hits_log(50);
    let mut engine = TailEngine::open(&log).unwrap();
    assert_eq!(engine.selection_bounds(), None);
    engine.selection.extend([7, 3, 12]);
    assert_eq!(engine.selection_bounds(), Some((3, 12)));
    engine.selection.clear();
    engine.selection_all = true;
    assert_eq!(engine.selection_bounds(), Some((0, 49)));
}

#[test]
fn truncation_resets_the_scope_and_says_so() {
    let (_dir, log) = hits_log(50);
    let mut engine = TailEngine::open(&log).unwrap();
    engine.update_search("hit");
    engine.set_search_scope(lines(10, Some(20)));
    std::fs::write(&log, "hit a\nhit b\n").unwrap();
    engine.poll_updates();
    assert!(engine.search_scope().is_all());
    assert!(engine.search_scope_reset);
    assert_eq!(engine.search_total(), 2);
    engine.set_search_scope(lines(0, Some(0)));
    assert!(!engine.search_scope_reset);
    assert_eq!(engine.search_total(), 1);
}
