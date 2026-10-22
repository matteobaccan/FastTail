// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;
use fasttail::folder_source::{expand_all, FolderGroup};
use fasttail::ui::dock::FastTailTab;
use fasttail::ui::FastTailApp;
use fasttail::wildcard::FolderSpec;
use std::path::{Path, PathBuf};

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect()
}

#[test]
fn several_patterns_follow_the_newest_of_any() {
    let dir = tempfile::tempdir().unwrap();
    write_file_with_mtime(&dir.path().join("app.log"), "log\n", 300);
    write_file_with_mtime(&dir.path().join("trace.txt"), "txt\n", 100);
    write_file_with_mtime(&dir.path().join("notes.md"), "md\n", 0);
    let pattern = dir.path().join("*.log;*.txt");
    let mut engine = TailEngine::open_pattern(&pattern).unwrap();
    engine.pattern_scan_interval = Duration::from_secs(0);
    assert_eq!(engine.current_file_name().as_deref(), Some("trace.txt"));
    write_file_with_mtime(&dir.path().join("app.log"), "log\nnewer\n", 0);
    engine.poll_updates();
    assert_eq!(engine.current_file_name().as_deref(), Some("app.log"));
}

#[test]
fn subfolders_are_walked_to_depth_eight_without_links() {
    let dir = tempfile::tempdir().unwrap();
    let mut deep = dir.path().to_path_buf();
    for level in 1..=9 {
        deep = deep.join(format!("d{level}"));
        std::fs::create_dir(&deep).unwrap();
        std::fs::write(deep.join(format!("at-{level}.log")), "x\n").unwrap();
    }
    std::fs::write(dir.path().join("top.log"), "x\n").unwrap();
    let flat = FolderSpec::new(dir.path().to_path_buf(), "*.log", false);
    assert_eq!(flat.list().files.len(), 1);
    let spec = FolderSpec::new(dir.path().to_path_buf(), "*.log", true);
    let found: Vec<String> = spec
        .list()
        .files
        .iter()
        .map(|m| m.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(found.contains(&"top.log".to_string()));
    assert!(found.contains(&"at-8.log".to_string()));
    assert!(!found.contains(&"at-9.log".to_string()), "{found:?}");

    let limited = spec.list_limited(3);
    assert!(limited.truncated);
    assert!(!spec.list().truncated);

    // A link back to the folder is not entered: the walk ends with the same files.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path(), dir.path().join("d1").join("loop")).unwrap();
        assert_eq!(spec.list().files.len(), found.len());
    }
}

#[test]
fn a_recursive_stream_follows_the_newest_dated_subfolder() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("2026-09-28");
    std::fs::create_dir(&old).unwrap();
    write_file_with_mtime(&old.join("app.log"), "old\n", 100);
    let pattern = dir.path().join("**").join("app.log");
    let mut engine = TailEngine::open_pattern(&pattern).unwrap();
    engine.pattern_scan_interval = Duration::from_secs(0);
    assert_eq!(
        engine.current_file.as_deref(),
        Some(old.join("app.log").as_path())
    );
    let new = dir.path().join("2026-09-29");
    std::fs::create_dir(&new).unwrap();
    std::fs::write(new.join("app.log"), "new\n").unwrap();
    engine.poll_updates();
    assert_eq!(
        engine.current_file.as_deref(),
        Some(new.join("app.log").as_path())
    );
    assert!(engine.active_switch_notice().is_some());
}

#[test]
fn a_group_opens_by_name_caps_by_age_and_skips_archives() {
    let dir = tempfile::tempdir().unwrap();
    for (i, name) in ["worker.log", "api.log", "scheduler.log"]
        .iter()
        .enumerate()
    {
        write_file_with_mtime(&dir.path().join(name), "x\n", 10 * i as u64 + 10);
    }
    std::fs::write(dir.path().join("app.log.1.gz"), b"\x1f\x8b").unwrap();
    let all = FolderSpec::new(dir.path().to_path_buf(), "*", false);
    let (_, scan) = FolderGroup::start(all, 32, None);
    assert_eq!(
        names(&scan.open),
        vec!["api.log", "scheduler.log", "worker.log"]
    );
    let with_gz = FolderSpec::new(dir.path().to_path_buf(), "*;*.gz", false);
    let (_, scan) = FolderGroup::start(with_gz, 32, None);
    assert_eq!(scan.open.len(), 4);

    // Cap 2: the two most recently modified (worker 10 s ago, api 20 s ago), by name.
    let spec = FolderSpec::new(dir.path().to_path_buf(), "*.log", false);
    let (mut group, scan) = FolderGroup::start(spec, 2, None);
    assert_eq!(names(&scan.open), vec!["api.log", "worker.log"]);
    assert_eq!(scan.skipped, 1);
    // A skipped file is never offered again, and a full group opens nothing more.
    group.scan_interval = Duration::ZERO;
    std::fs::write(dir.path().join("job-1.log"), "x\n").unwrap();
    let later = group.poll(2);
    assert!(later.open.is_empty());
    assert_eq!(later.skipped, 1);
    // A closed tab makes room for the next new file.
    std::fs::write(dir.path().join("job-2.log"), "x\n").unwrap();
    assert_eq!(names(&group.poll(1).open), vec!["job-2.log"]);
}

#[test]
fn a_group_reports_files_created_later_once() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("api.log"), "x\n").unwrap();
    let spec = FolderSpec::new(dir.path().to_path_buf(), "*.log", false);
    let (mut group, scan) = FolderGroup::start(spec, 32, None);
    assert_eq!(names(&scan.open), vec!["api.log"]);
    assert!(group.poll(1).open.is_empty(), "not due yet");
    group.scan_interval = Duration::ZERO;
    std::fs::write(dir.path().join("job-1234.log"), "x\n").unwrap();
    std::fs::write(dir.path().join("job-1234.txt"), "x\n").unwrap();
    assert_eq!(names(&group.poll(1).open), vec!["job-1234.log"]);
    assert!(group.poll(2).open.is_empty());
    assert!(group.offered(&dir.path().join("job-1234.log")));
    assert!(!group.has_members(), "the app adopts the streams it opens");
}

#[test]
fn expand_all_replaces_patterns_with_their_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.log"), "x\n").unwrap();
    std::fs::write(dir.path().join("a.log"), "x\n").unwrap();
    let plain = dir.path().join("plain.txt");
    let paths = vec![dir.path().join("*.log"), plain.clone()];
    let out = expand_all(&paths, 32);
    assert_eq!(
        out,
        vec![dir.path().join("a.log"), dir.path().join("b.log"), plain]
    );
    let cli = fasttail::cli::CliArgs::parse(["--all", "x.log"], Path::new("/w")).unwrap();
    assert!(cli.all);
}

#[test]
fn a_deleted_file_keeps_its_lines_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("api.log");
    std::fs::write(&file, "one\ntwo\n").unwrap();
    let mut engine = TailEngine::open(&file).unwrap();
    engine.size_check_interval = Duration::ZERO;
    engine.poll_updates();
    assert!(!engine.file_removed);
    std::fs::remove_file(&file).unwrap();
    engine.poll_updates();
    assert!(engine.file_removed);
    assert_eq!(engine.total_lines(), 2);
    std::fs::write(&file, "fresh\n").unwrap();
    engine.poll_updates();
    assert!(!engine.file_removed);
}

#[test]
fn folder_groups_round_trip_in_config_and_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let group = FolderSpec::new(dir.path().to_path_buf(), "*.log;*.txt", true).path();
    let mut cfg = FastTailConfig {
        folder_max_streams: 7,
        folder_groups: vec![group.clone()],
        ..FastTailConfig::default()
    };
    let loaded = FastTailConfig::from_ini(&cfg.to_ini());
    assert_eq!(loaded.folder_max_streams, 7);
    assert_eq!(loaded.folder_groups, vec![group.clone()]);
    cfg.folder_max_streams = 9999;
    assert_eq!(
        FastTailConfig::from_ini(&cfg.to_ini()).folder_max_streams,
        256
    );

    let session = fasttail::session::Session {
        folders: vec![group.clone()],
        ..Default::default()
    };
    let file = dir.path().join("s.fasttail-session.ini");
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[folder_0]"), "{text}");
    let back = fasttail::session::Session::load_from(&file).unwrap();
    assert_eq!(back.session.folders, vec![group]);
    // An old session without folders loads as before.
    let old = fasttail::session::Session::default();
    old.save_to(&file).unwrap();
    assert!(fasttail::session::Session::load_from(&file)
        .unwrap()
        .session
        .folders
        .is_empty());
}

#[test]
fn open_all_puts_the_files_in_one_new_node_and_adds_new_ones_without_focus() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("other.txt");
    std::fs::write(&other, "x\n").unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir(&logs).unwrap();
    for name in ["api.log", "worker.log"] {
        std::fs::write(logs.join(name), "x\n").unwrap();
    }
    let mut app = FastTailApp::from_config(FastTailConfig::default());
    app.open_log_file(other.clone());
    app.open_folder_group(FolderSpec::new(logs.clone(), "*.log", false));
    let leaf_of = |app: &FastTailApp, path: &Path| {
        app.dock_state
            .iter_all_tabs()
            .find(|(_, t)| matches!(t, FastTailTab::LogStream(p) if p == path))
            .map(|(at, _)| (at.surface, at.node))
            .unwrap()
    };
    let group_node = leaf_of(&app, &logs.join("api.log"));
    assert_eq!(group_node, leaf_of(&app, &logs.join("worker.log")));
    assert_ne!(group_node, leaf_of(&app, &other));
    assert_eq!(app.config.folder_groups.len(), 1);

    let active_before = app.dock_state[group_node.0][group_node.1]
        .get_leaf()
        .unwrap()
        .active;
    app.folder_groups[0].scan_interval = Duration::ZERO;
    std::fs::write(logs.join("job-1234.log"), "x\n").unwrap();
    app.poll_folder_groups();
    assert_eq!(leaf_of(&app, &logs.join("job-1234.log")), group_node);
    let leaf = app.dock_state[group_node.0][group_node.1]
        .get_leaf()
        .unwrap();
    assert_eq!(leaf.active, active_before);
    assert_eq!(leaf.tabs.len(), 3);
}

#[test]
fn a_saved_group_comes_back_at_start_with_the_files_created_since() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir(&logs).unwrap();
    std::fs::write(logs.join("api.log"), "x\n").unwrap();
    let group = FolderSpec::new(logs.clone(), "*.log", false).path();
    std::fs::write(logs.join("new.log"), "x\n").unwrap();
    // No dock layout: the streams reopen one by one, each saving the workspace.
    let app = FastTailApp::from_config(FastTailConfig {
        open_files: vec![logs.join("api.log")],
        folder_groups: vec![group.clone()],
        ..FastTailConfig::default()
    });
    assert_eq!(app.folder_groups.len(), 1);
    assert_eq!(app.config.folder_groups, vec![group]);
    assert!(app.engines.iter().any(|e| e.path == logs.join("new.log")));
    assert!(app.folder_groups[0].is_member(&logs.join("api.log")));
}

#[test]
fn a_waiting_group_opens_its_first_file_without_taking_the_focus() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("other.txt");
    std::fs::write(&other, "x\n").unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir(&logs).unwrap();
    let mut app = FastTailApp::from_config(FastTailConfig::default());
    app.open_log_file(other.clone());
    app.open_folder_group(FolderSpec::new(logs.clone(), "*.log", false));
    app.open_notice = None;
    assert_eq!(app.folder_groups.len(), 1);
    assert!(
        app.config.folder_groups.is_empty(),
        "nothing open yet, nothing kept"
    );
    let focused = app.dock_state.focused_leaf();
    app.folder_groups[0].scan_interval = Duration::ZERO;
    std::fs::write(logs.join("first.log"), "x\n").unwrap();
    app.poll_folder_groups();
    assert!(app.engines.iter().any(|e| e.path == logs.join("first.log")));
    assert_eq!(app.dock_state.focused_leaf(), focused);
    assert_eq!(app.config.folder_groups.len(), 1);
}
