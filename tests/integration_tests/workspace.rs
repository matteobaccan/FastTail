// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! `workspace::open_target` and the engine setup shared by the GUI and the terminal.

use fasttail::config::FastTailConfig;
use fasttail::workspace::{apply_settings, open_target, restore_stream, OpenOutcome};
use std::io::Write;
use std::path::Path;

fn config(dir: &Path) -> FastTailConfig {
    FastTailConfig {
        spool_dir: Some(dir.join("spool")),
        ..FastTailConfig::default()
    }
}

fn zip(path: &Path, names: &[&str]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for name in names {
        zip.start_file(*name, options).unwrap();
        zip.write_all(b"started\n").unwrap();
    }
    zip.finish().unwrap();
}

fn kind(outcome: &OpenOutcome) -> &'static str {
    match outcome {
        OpenOutcome::Opened(_) => "opened",
        OpenOutcome::Missing => "missing",
        OpenOutcome::Failed { .. } => "failed",
        OpenOutcome::OpenEntry(_) => "entry",
        OpenOutcome::EmptyArchive(_) => "empty",
        OpenOutcome::ChooseEntries { .. } => "choose",
        OpenOutcome::ScanTar { .. } => "tar",
        OpenOutcome::ListFailed { .. } => "list failed",
    }
}

#[test]
fn open_target_tells_files_patterns_and_archives_apart() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());

    let plain = dir.path().join("app.log");
    std::fs::write(&plain, "a\nb\n").unwrap();
    assert_eq!(kind(&open_target(&plain, &cfg, None)), "opened");
    assert_eq!(
        kind(&open_target(&dir.path().join("gone.log"), &cfg, None)),
        "missing"
    );
    assert_eq!(
        kind(&open_target(&dir.path().join("*.log"), &cfg, None)),
        "opened",
        "a pattern opens its newest match"
    );

    let gz = dir.path().join("old.log.gz");
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    enc.write_all(b"x\n").unwrap();
    std::fs::write(&gz, enc.finish().unwrap()).unwrap();
    assert_eq!(kind(&open_target(&gz, &cfg, None)), "opened");

    let single = dir.path().join("one.zip");
    zip(&single, &["server.log"]);
    match open_target(&single, &cfg, None) {
        OpenOutcome::OpenEntry(entry) => assert_eq!(
            entry,
            fasttail::compressed::entry_path(&single, "server.log")
        ),
        other => panic!("one entry opens directly, got {}", kind(&other)),
    }
    let entry = fasttail::compressed::entry_path(&single, "server.log");
    assert_eq!(kind(&open_target(&entry, &cfg, None)), "opened");

    let several = dir.path().join("two.zip");
    zip(&several, &["a.log", "b.log"]);
    match open_target(&several, &cfg, None) {
        OpenOutcome::ChooseEntries {
            archive,
            entries,
            partial,
        } => {
            assert_eq!(archive, several);
            assert_eq!(entries.len(), 2);
            assert!(!partial);
        }
        other => panic!("several entries are a choice, got {}", kind(&other)),
    }

    let empty = dir.path().join("empty.zip");
    zip(&empty, &[]);
    assert_eq!(kind(&open_target(&empty, &cfg, None)), "empty");

    let tar_path = dir.path().join("logs.tar");
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(2);
    header.set_mode(0o644);
    builder
        .append_data(&mut header, "a.log", &b"x\n"[..])
        .unwrap();
    std::fs::write(&tar_path, builder.into_inner().unwrap()).unwrap();
    assert_eq!(kind(&open_target(&tar_path, &cfg, None)), "tar");
}

#[test]
fn a_new_engine_gets_the_settings_and_the_saved_stream_state() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(&log, "one\ntwo ERROR\nthree\n").unwrap();
    let mut cfg = config(dir.path());
    cfg.show_line_numbers = false;
    cfg.auto_bookmark_max = 7;
    cfg.set_wrap(&log, true);
    cfg.set_bookmarks_with_notes(&log, &[1], &[(1, "look".to_string())].into());

    let OpenOutcome::Opened(engine) = open_target(&log, &cfg, None) else {
        panic!("a plain file opens");
    };
    let mut engine = *engine;
    apply_settings(&mut engine, &cfg);
    assert!(!engine.show_line_numbers);
    assert_eq!(engine.auto_bookmark_max, 7);
    restore_stream(&mut engine, &cfg, &log);
    assert!(engine.wrap_lines);
    assert!(engine.bookmarks.contains(&1));
    assert_eq!(
        engine.bookmark_notes.get(&1).map(String::as_str),
        Some("look")
    );
}

#[test]
fn snapshot_writes_the_open_files_and_stream_states_into_the_config() {
    use fasttail::workspace::{save_changes, snapshot};
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    std::fs::write(&a, "one\ntwo\n").unwrap();
    std::fs::write(&b, "three\n").unwrap();
    let mut cfg = config(dir.path());
    let gone = dir.path().join("gone.log");
    cfg.set_stream_state(fasttail::session::StreamEntry::new(gone.clone()));

    let mut engines = vec![
        fasttail::tail_engine::TailEngine::open(&a).unwrap(),
        fasttail::tail_engine::TailEngine::open(&b).unwrap(),
    ];
    engines[0].set_include_filter("two");
    engines[0].toggle_bookmark(1);
    let order = vec![b.clone(), a.clone(), a.clone()];
    let state = snapshot(&order, &engines);
    assert_eq!(state.open_files, vec![b.clone(), a.clone()]);
    assert_eq!(state.streams.len(), 2);
    assert_eq!(
        state.streams[0].bookmarks,
        vec![1],
        "sessions get the bookmarks"
    );

    state.write_into(&mut cfg);
    assert_eq!(cfg.open_files, vec![b.clone(), a.clone()]);
    let saved = cfg.stream_state_for(&a).unwrap();
    assert_eq!(saved.include_filter, "two");
    assert!(
        saved.bookmarks.is_empty(),
        "bookmarks have their own section"
    );
    assert!(
        cfg.stream_state_for(&gone).is_none(),
        "closed files are dropped"
    );

    // Bookmarks are written as they change.
    assert!(save_changes(&mut engines[0], &mut cfg, false));
    assert_eq!(
        cfg.saved_bookmarks(&a).map(|(_, lines, _)| lines.clone()),
        Some(vec![1])
    );
    assert!(
        !save_changes(&mut engines[0], &mut cfg, false),
        "nothing new"
    );
}

#[test]
fn restore_reopens_the_saved_open_files_with_their_state() {
    use fasttail::workspace::restore;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.log");
    let b = dir.path().join("b.log");
    std::fs::write(&a, "one\ntwo\n").unwrap();
    std::fs::write(&b, "three\n").unwrap();
    let bundle = dir.path().join("bundle.zip");
    zip(&bundle, &["x.log", "y.log"]);
    let gone = dir.path().join("gone.log");

    let mut cfg = config(dir.path());
    cfg.open_files = vec![
        b.clone(),
        gone.clone(),
        a.clone(),
        b.clone(),
        bundle.clone(),
    ];
    let mut entry = fasttail::session::StreamEntry::new(a.clone());
    entry.include_filter = "two".to_string();
    cfg.set_stream_state(entry);

    let restored = restore(&cfg, None);
    let paths: Vec<_> = restored.engines.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        paths,
        vec![b.clone(), a.clone()],
        "tab order, no duplicates"
    );
    assert_eq!(restored.skipped, vec![gone, bundle]);
    assert_eq!(restored.engines[1].include_filter(), "two");
}
