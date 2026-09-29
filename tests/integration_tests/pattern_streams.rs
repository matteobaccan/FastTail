use super::*;

#[test]
fn test_wildcard_matcher_public_api() {
    use fasttail::wildcard::{is_pattern_path, wildcard_match};
    use std::path::Path;
    assert!(wildcard_match("app-*.log", "app-2026-09-18.log"));
    assert!(wildcard_match("app-????-??-??.log", "app-2026-09-18.log"));
    assert!(!wildcard_match("app-*.log", "other-2026-09-18.log"));
    assert!(!wildcard_match("app-?.log", "app-10.log"));
    assert!(is_pattern_path(Path::new("logs/app-*.log")));
    assert!(!is_pattern_path(Path::new("logs/app.log")));
}

#[test]
fn test_resolve_newest_by_mtime_then_name() {
    use fasttail::wildcard::resolve_newest;
    let dir = tempfile::tempdir().unwrap();
    write_file_with_mtime(&dir.path().join("app-2026-09-17.log"), "old\n", 300);
    write_file_with_mtime(&dir.path().join("app-2026-09-18.log"), "mid\n", 200);
    write_file_with_mtime(&dir.path().join("app-2026-09-19.log"), "new\n", 100);
    write_file_with_mtime(&dir.path().join("other-2026-09-20.log"), "x\n", 0);
    std::fs::create_dir(dir.path().join("app-2026-09-30.log")).unwrap(); // a directory, ignored
    let newest = resolve_newest(dir.path(), "app-*.log").unwrap();
    assert_eq!(newest, dir.path().join("app-2026-09-19.log"));

    // Same modification time: the greater name wins.
    write_file_with_mtime(&dir.path().join("app-2026-09-21.log"), "a\n", 100);
    write_file_with_mtime(&dir.path().join("app-2026-09-20.log"), "b\n", 100);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(dir.path().join("app-2026-09-19.log"))
        .unwrap();
    let t = std::fs::metadata(dir.path().join("app-2026-09-21.log"))
        .unwrap()
        .modified()
        .unwrap();
    f.set_modified(t).unwrap();
    for name in ["app-2026-09-20.log", "app-2026-09-19.log"] {
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join(name))
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    let newest = resolve_newest(dir.path(), "app-*.log").unwrap();
    assert_eq!(newest, dir.path().join("app-2026-09-21.log"));

    assert!(resolve_newest(dir.path(), "nothing-*.log").is_none());
    assert!(resolve_newest(&dir.path().join("missing"), "*.log").is_none());
}

#[test]
fn test_pattern_stream_switches_to_newer_file_keeping_filters() {
    let dir = tempfile::tempdir().unwrap();
    write_file_with_mtime(
        &dir.path().join("app-2026-09-18.log"),
        "INFO start\nERROR one\nINFO middle\nERROR two\n",
        100,
    );
    let pattern = dir.path().join("app-*.log");
    let mut engine = TailEngine::open_pattern(&pattern).unwrap();
    engine.pattern_scan_interval = Duration::from_secs(0);
    assert!(engine.is_pattern());
    assert_eq!(engine.path, pattern);
    assert_eq!(
        engine.current_file_name().as_deref(),
        Some("app-2026-09-18.log")
    );
    assert_eq!(engine.total_lines(), 4);
    assert!(engine.switch_notice.is_none(), "opening is not a switch");

    engine.set_include_filter("ERROR");
    engine.update_search("two");
    engine.wrap_lines = true;
    engine.toggle_bookmark(1);
    engine.select_row(3);
    assert_eq!(engine.visible_line_count(), 2);
    assert!(engine.has_bookmarks());
    assert!(engine.has_selection());

    // A newer file appears: the stream follows it within one poll.
    write_file_with_mtime(
        &dir.path().join("app-2026-09-19.log"),
        "ERROR fresh\nINFO quiet\nERROR two again\n",
        0,
    );
    engine.poll_updates();
    assert_eq!(
        engine.current_file_name().as_deref(),
        Some("app-2026-09-19.log")
    );
    assert_eq!(
        engine.path, pattern,
        "the stream identity stays the pattern"
    );
    assert_eq!(engine.total_lines(), 3);
    assert_eq!(
        engine.include_filter(),
        "ERROR",
        "filters survive the switch"
    );
    assert_eq!(
        engine.visible_line_count(),
        2,
        "the include filter is re-applied"
    );
    assert_eq!(
        engine.last_searched_query, "two",
        "the search query survives"
    );
    assert_eq!(
        engine.search_matches,
        vec![2],
        "search is re-run on the new file"
    );
    assert!(engine.wrap_lines, "wrap survives");
    assert!(!engine.has_bookmarks(), "bookmarks reset");
    assert!(!engine.has_selection(), "selection reset");
    assert_eq!(engine.unseen_lines, 0);
    assert_eq!(engine.active_switch_notice(), Some("app-2026-09-19.log"));

    // The same newest file does not switch again, and appends are still followed.
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 3);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join("app-2026-09-19.log"))
        .unwrap();
    writeln!(f, "ERROR appended").unwrap();
    f.flush().unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 4);
    assert_eq!(engine.visible_line_count(), 3);
}

#[test]
fn test_pattern_stream_waits_for_first_match() {
    let dir = tempfile::tempdir().unwrap();
    let pattern = dir.path().join("svc-*.log");
    let mut engine = TailEngine::open_pattern(&pattern).unwrap();
    engine.pattern_scan_interval = Duration::from_secs(0);
    assert!(engine.is_pattern());
    assert!(engine.current_file.is_none());
    assert_eq!(engine.total_lines(), 0);
    engine.set_include_filter("WARN");
    engine.poll_updates();
    assert!(engine.current_file.is_none(), "still nothing to tail");

    write_file_with_mtime(&dir.path().join("svc-1.log"), "WARN a\nINFO b\n", 0);
    engine.poll_updates();
    assert_eq!(engine.current_file_name().as_deref(), Some("svc-1.log"));
    assert_eq!(engine.total_lines(), 2);
    assert_eq!(engine.visible_line_count(), 1);
    assert_eq!(engine.active_switch_notice(), Some("svc-1.log"));

    // Pattern with no directory part is rejected only when the directory is missing.
    assert!(TailEngine::open_pattern(dir.path().join("missing").join("*.log")).is_err());
    assert!(TailEngine::open_pattern(dir.path().join("plain.log")).is_err());
}

#[test]
fn test_pattern_entries_persist_in_ini() {
    use std::path::PathBuf;
    let dir = tempfile::tempdir().unwrap();
    let pattern = dir.path().join("app-*.log");
    let plain_missing = dir.path().join("gone.log");
    let mut config = FastTailConfig::default();
    config.open_files = vec![pattern.clone(), plain_missing.clone()];
    config.recent_files = vec![pattern.clone(), PathBuf::from("recent.log")];

    let ini = config.to_ini();
    let loaded = FastTailConfig::from_ini(&ini);
    assert_eq!(
        loaded.open_files,
        vec![pattern.clone()],
        "the pattern is kept, the missing plain file is dropped"
    );
    assert_eq!(
        loaded.recent_files,
        vec![pattern, PathBuf::from("recent.log")]
    );
}
