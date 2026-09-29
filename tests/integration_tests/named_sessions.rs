// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::cli::CliArgs;
use fasttail::config::FastTailConfig;
use fasttail::i18n::{t, Language};
use fasttail::session::{Session, StreamEntry, SESSION_SUFFIX};
use std::path::{Path, PathBuf};

fn entry(path: PathBuf) -> StreamEntry {
    StreamEntry {
        path,
        include_filter: "ERROR".to_string(),
        exclude_filter: "health".to_string(),
        include_extra: vec!["payment".to_string(), "timeout".to_string()],
        exclude_extra: vec!["retry=0".to_string()],
        search_query: "timeout".to_string(),
        wrap: true,
        encoding: Some("ANSI".to_string()),
        ansi: Some("strip".to_string()),
        timeline: true,
        collapse: Some("numbers".to_string()),
        context_lines: 4,
        line_numbers: Some(false),
        time_delta: Some(true),
        time_display: Some("+02:00".to_string()),
        time_source_zone: Some("utc".to_string()),
        bookmarks: vec![3, 7, 42],
        bookmark_notes: [(7, "deploy start".to_string())].into_iter().collect(),
        archive_entry: None,
        fields_parser: Some("regex".to_string()),
        fields_regex: Some(r" ^(?P<lvl>\w+) ".to_string()),
        fields_view: true,
        fields_columns: vec!["ts".to_string(), "http.status".to_string()],
        fields_widths: [("http.status".to_string(), 6), ("ts".to_string(), 24)]
            .into_iter()
            .collect(),
    }
}

#[test]
fn every_session_field_round_trips_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(&log, "a\nb\n").unwrap();
    let pattern = dir.path().join("app-*.log");
    let mut session = Session {
        streams: vec![entry(log.clone()), StreamEntry::new(pattern.clone())],
        dock_layout: Some("(layout)".to_string()),
    };
    session.streams[1].wrap = false;
    let file = dir.path().join(format!("incident{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();

    let loaded = Session::load_from(&file).unwrap();
    assert!(loaded.missing.is_empty());
    assert!(!loaded.relocated);
    assert_eq!(loaded.session, session);
    assert_eq!(Session::name_of(&file), "incident");
}

#[test]
fn filter_terms_and_search_keep_quotes_and_edge_spaces_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(
        &log, "a
",
    )
    .unwrap();
    let tricky: Vec<String> = TRICKY.iter().map(|t| t.to_string()).collect();
    let mut stream = entry(log);
    stream.include_filter = tricky[0].clone();
    stream.exclude_filter = tricky[1].clone();
    stream.search_query = tricky[2].clone();
    stream.include_extra = tricky[3..].to_vec();
    stream.exclude_extra = tricky[3..].to_vec();
    let session = Session {
        streams: vec![stream],
        dock_layout: None,
    };
    let file = dir.path().join(format!("quotes{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    assert_eq!(Session::load_from(&file).unwrap().session, session);
}

const TRICKY: [&str; 7] = [
    "\"status\":500",
    " ERROR ",
    "'user'",
    "it's \"x\" 'y'",
    "''\"",
    "\tpad\\d+\t",
    "a\\\\b",
];

/// A zip holding `server.log` and `logs/worker.log`.
fn write_bundle(path: &Path) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for name in ["server.log", "logs/worker.log"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(b"started\n").unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn compressed_streams_are_saved_as_the_archive_and_the_entry() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle.zip");
    write_bundle(&bundle);
    let gz = dir.path().join("app.log.1.gz");
    std::fs::write(&gz, [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    let worker = fasttail::compressed::entry_path(&bundle, "logs/worker.log");
    let mut zipped = entry(worker.clone());
    zipped.archive_entry = Some("logs/worker.log".to_string());
    let session = Session {
        streams: vec![zipped, entry(gz.clone())],
        dock_layout: None,
    };
    let file = dir.path().join(format!("bundle{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();

    // The file names the archive, never the entry path, plus the entry.
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("entry=logs/worker.log"), "{text}");
    assert!(text.contains("rel=bundle.zip"), "{text}");
    let loaded = Session::load_from(&file).unwrap();
    assert!(loaded.missing.is_empty());
    assert_eq!(loaded.session, session);

    // The bundle and its session move together: the entry follows the archive.
    let moved = dir.path().join("moved");
    std::fs::create_dir_all(&moved).unwrap();
    std::fs::rename(&bundle, moved.join("bundle.zip")).unwrap();
    std::fs::rename(&gz, moved.join("app.log.1.gz")).unwrap();
    let moved_file = moved.join(format!("bundle{SESSION_SUFFIX}"));
    std::fs::rename(&file, &moved_file).unwrap();
    let loaded = Session::load_from(&moved_file).unwrap();
    assert!(loaded.missing.is_empty());
    assert_eq!(
        loaded.session.streams[0].path,
        fasttail::compressed::entry_path(&moved.join("bundle.zip"), "logs/worker.log")
    );
    assert_eq!(
        loaded.session.streams[0].archive_entry.as_deref(),
        Some("logs/worker.log")
    );
}

#[test]
fn a_backslash_zip_entry_survives_a_session_round_trip() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("win.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&bundle).unwrap());
    zip.start_file("dir\\file.log", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"started\n").unwrap();
    zip.finish().unwrap();
    let settings = fasttail::compressed::Settings {
        spool_dir: dir.path().join("spool"),
        limits: fasttail::compressed::Limits::default(),
    };
    let engine =
        fasttail::compressed::open_engine(&bundle, Some("dir/file.log"), &settings, None).unwrap();
    // What the app saves for the stream: its path and its entry identity.
    let mut zipped = entry(engine.path.clone());
    zipped.archive_entry = engine.compressed.as_ref().unwrap().entry.clone();
    let session = Session {
        streams: vec![zipped],
        dock_layout: None,
    };
    let file = dir.path().join(format!("win{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("entry=dir/file.log"), "{text}");
    let loaded = Session::load_from(&file).unwrap();
    assert!(loaded.missing.is_empty(), "{:?}", loaded.missing);
    assert_eq!(loaded.session, session);

    // A session written by 0.10.0 saved the archive spelling: it still resolves.
    let mut old = session.clone();
    old.streams[0].archive_entry = Some("dir\\file.log".to_string());
    old.save_to(&file).unwrap();
    let loaded = Session::load_from(&file).unwrap();
    assert!(loaded.missing.is_empty(), "{:?}", loaded.missing);
    assert_eq!(loaded.session.streams[0].path, engine.path);
}

#[test]
fn the_default_session_keeps_zip_entries_open() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle.zip");
    write_bundle(&bundle);
    let server = fasttail::compressed::entry_path(&bundle, "server.log");
    let mut cfg = FastTailConfig::default();
    cfg.open_files = vec![server.clone()];
    let loaded = FastTailConfig::from_ini(&cfg.to_ini());
    assert_eq!(loaded.open_files, vec![server.clone()]);
    assert_eq!(
        Session::from_config(&loaded).streams[0]
            .archive_entry
            .as_deref(),
        Some("server.log")
    );
    // Once the archive is gone the entry is dropped like any missing file.
    std::fs::remove_file(&bundle).unwrap();
    assert!(FastTailConfig::from_ini(&cfg.to_ini())
        .open_files
        .is_empty());
}

#[test]
fn a_7z_entry_survives_a_session_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs.7z");
    super::write_7z(
        &logs,
        &[("server.log", b"started\n"), ("logs/worker.log", b"w\n")],
    );
    let settings = fasttail::compressed::Settings {
        spool_dir: dir.path().join("spool"),
        limits: fasttail::compressed::Limits::default(),
    };
    let engine =
        fasttail::compressed::open_engine(&logs, Some("logs/worker.log"), &settings, None).unwrap();
    let mut stream = entry(engine.path.clone());
    stream.archive_entry = engine.compressed.as_ref().unwrap().entry.clone();
    let session = Session {
        streams: vec![stream],
        dock_layout: None,
    };
    let file = dir.path().join(format!("logs{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("entry=logs/worker.log"), "{text}");
    assert!(text.contains("rel=logs.7z"), "{text}");
    let loaded = Session::load_from(&file).unwrap();
    assert!(loaded.missing.is_empty(), "{:?}", loaded.missing);
    assert_eq!(loaded.session, session);
    // The default session (fasttail.ini) keeps it too.
    let mut cfg = FastTailConfig::default();
    cfg.open_files = vec![engine.path.clone()];
    let loaded = FastTailConfig::from_ini(&cfg.to_ini());
    assert_eq!(loaded.open_files, vec![engine.path.clone()]);
    assert_eq!(
        Session::from_config(&loaded).streams[0]
            .archive_entry
            .as_deref(),
        Some("logs/worker.log")
    );
}

#[test]
fn files_that_no_longer_exist_are_listed_and_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let kept = dir.path().join("kept.log");
    let gone = dir.path().join("gone.log");
    std::fs::write(&kept, "a\n").unwrap();
    std::fs::write(&gone, "b\n").unwrap();
    // A pattern whose directory disappears counts as missing too.
    let gone_dir = dir.path().join("rotated");
    std::fs::create_dir_all(&gone_dir).unwrap();
    let pattern = gone_dir.join("app-*.log");
    let file = dir.path().join(format!("partial{SESSION_SUFFIX}"));
    Session {
        streams: vec![
            entry(kept.clone()),
            entry(gone.clone()),
            StreamEntry::new(pattern.clone()),
        ],
        dock_layout: Some("(layout)".to_string()),
    }
    .save_to(&file)
    .unwrap();

    std::fs::remove_file(&gone).unwrap();
    std::fs::remove_dir_all(&gone_dir).unwrap();

    let loaded = Session::load_from(&file).unwrap();
    let opened: Vec<&Path> = loaded
        .session
        .streams
        .iter()
        .map(|s| s.path.as_path())
        .collect();
    assert_eq!(
        opened,
        vec![kept.as_path()],
        "only the file still on disk is opened"
    );
    assert_eq!(loaded.missing.len(), 2, "{:?}", loaded.missing);
    let named = |p: &Path| {
        loaded
            .missing
            .iter()
            .any(|m| m.file_name() == p.file_name())
    };
    assert!(named(&gone), "the deleted file is reported");
    assert!(
        named(&pattern),
        "the pattern of a deleted directory is reported"
    );
    // The surviving stream keeps what was saved for it.
    assert_eq!(loaded.session.streams[0], entry(kept));
}

#[test]
fn a_moved_bundle_opens_through_the_relative_path() {
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("bundle");
    std::fs::create_dir_all(bundle.join("logs")).unwrap();
    let log = bundle.join("logs").join("app.log");
    std::fs::write(&log, "x\n").unwrap();
    let file = bundle.join(format!("dev{SESSION_SUFFIX}"));
    Session {
        streams: vec![entry(log.clone())],
        dock_layout: Some("(layout)".to_string()),
    }
    .save_to(&file)
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("rel=logs/app.log"), "{text}");

    // Move the whole bundle: the absolute path no longer exists.
    let moved = root.path().join("moved");
    std::fs::rename(&bundle, &moved).unwrap();
    let loaded = Session::load_from(&moved.join(format!("dev{SESSION_SUFFIX}"))).unwrap();
    assert!(loaded.missing.is_empty());
    assert_eq!(loaded.session.streams.len(), 1);
    assert_eq!(
        loaded.session.streams[0].path,
        moved.join("logs").join("app.log")
    );
    assert_eq!(loaded.session.streams[0].include_filter, "ERROR");
    assert!(loaded.relocated);
    assert_eq!(
        loaded.session.dock_layout, None,
        "a layout naming the old paths is dropped"
    );
}

#[test]
fn missing_files_are_skipped_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let present = dir.path().join("present.log");
    std::fs::write(&present, "x\n").unwrap();
    let gone = dir.path().join("gone.log");
    let no_dir_pattern = dir.path().join("nowhere").join("*.log");
    let file = dir.path().join(format!("s{SESSION_SUFFIX}"));
    Session {
        streams: vec![
            StreamEntry::new(present.clone()),
            StreamEntry::new(gone.clone()),
            StreamEntry::new(no_dir_pattern.clone()),
        ],
        dock_layout: None,
    }
    .save_to(&file)
    .unwrap();
    let loaded = Session::load_from(&file).unwrap();
    assert_eq!(loaded.session.streams.len(), 1);
    assert_eq!(loaded.session.streams[0].path, present);
    assert_eq!(loaded.missing, vec![gone, no_dir_pattern]);
}

#[test]
fn serialized_text_detects_changes() {
    let a = Session {
        streams: vec![entry(PathBuf::from("C:/x/a.log"))],
        dock_layout: None,
    };
    let mut b = a.clone();
    assert_eq!(a.serialized(None), b.serialized(None));
    b.streams[0].bookmarks.push(99);
    assert_ne!(a.serialized(None), b.serialized(None));
    let mut c = a.clone();
    c.dock_layout = Some("(other)".to_string());
    assert_ne!(a.serialized(None), c.serialized(None));
}

#[test]
fn default_session_is_embedded_in_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("a.log");
    std::fs::write(&log, "x\n").unwrap();
    let mut cfg = FastTailConfig::default();
    cfg.open_files = vec![log.clone()];
    let mut e = entry(log.clone());
    e.wrap = false;
    e.bookmarks.clear();
    cfg.set_stream_state(e);
    cfg.set_wrap(&log, true);
    cfg.set_bookmarks(&log, &[3, 7, 42]);
    cfg.current_session = Some(dir.path().join(format!("cur{SESSION_SUFFIX}")));
    cfg.add_recent_session(&dir.path().join(format!("one{SESSION_SUFFIX}")));
    cfg.add_recent_session(&dir.path().join(format!("two{SESSION_SUFFIX}")));

    let ini = cfg.to_ini();
    let loaded = FastTailConfig::from_ini(&ini);
    assert_eq!(loaded.open_files, vec![log.clone()]);
    let state = loaded
        .stream_state_for(&log)
        .expect("stream state persisted");
    assert_eq!(state.include_filter, "ERROR");
    assert_eq!(state.exclude_filter, "health");
    assert_eq!(state.search_query, "timeout");
    assert_eq!(state.encoding.as_deref(), Some("ANSI"));
    assert!(loaded.wrap_for(&log));
    assert_eq!(loaded.bookmarks_for(&log, 100), Some(vec![3, 7, 42]));
    assert_eq!(loaded.current_session, cfg.current_session);
    assert_eq!(loaded.recent_sessions.len(), 2);
    assert_eq!(
        Session::name_of(&loaded.recent_sessions[0]),
        "two",
        "most recent first"
    );

    // The default session assembled from the config carries everything.
    let default = Session::from_config(&loaded);
    assert_eq!(default.streams.len(), 1);
    assert!(default.streams[0].wrap);
    assert_eq!(default.streams[0].bookmarks, vec![3, 7, 42]);
}

#[test]
fn config_without_session_sections_still_loads() {
    let text = "[general]\ntheme=Tron\nlanguage=en\n\n[open_files]\nfile_0=missing-for-sure.log\n";
    let ini = ini::Ini::load_from_str(text).unwrap();
    let cfg = FastTailConfig::from_ini(&ini);
    assert!(cfg.streams.is_empty());
    assert!(cfg.current_session.is_none());
    assert!(cfg.recent_sessions.is_empty());
}

#[test]
fn apply_to_config_replaces_the_default_workspace() {
    let mut cfg = FastTailConfig::default();
    cfg.open_files = vec![PathBuf::from("C:/old/x.log")];
    cfg.dock_layout = Some("(old)".to_string());
    let session = Session {
        streams: vec![entry(PathBuf::from("C:/new/a.log"))],
        dock_layout: Some("(new)".to_string()),
    };
    session.apply_to_config(&mut cfg);
    assert_eq!(cfg.open_files, vec![PathBuf::from("C:/new/a.log")]);
    assert_eq!(cfg.dock_layout.as_deref(), Some("(new)"));
    assert!(cfg.wrap_for(Path::new("C:/new/a.log")));
    assert_eq!(
        cfg.stream_state_for(Path::new("C:/new/a.log"))
            .map(|s| s.include_filter.clone()),
        Some("ERROR".to_string())
    );
}

#[test]
fn session_suffix_and_cli_option() {
    assert_eq!(
        Session::with_suffix(Path::new("C:/s/incident")),
        PathBuf::from(format!("C:/s/incident{SESSION_SUFFIX}"))
    );
    let cwd = if cfg!(windows) {
        PathBuf::from(r"C:\work")
    } else {
        PathBuf::from("/work")
    };
    let args = CliArgs::parse(["--session", "dev.fasttail-session.ini"], &cwd).unwrap();
    assert_eq!(args.session, Some(cwd.join("dev.fasttail-session.ini")));
    assert!(fasttail::cli::USAGE.contains("--session <FILE>"));
}

#[test]
fn session_i18n_keys_exist_in_every_language() {
    for lang in [
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        for key in [
            "session_tip",
            "session_save_as",
            "session_load",
            "session_unsaved_body",
            "session_missing_title",
        ] {
            let text = t(lang, key);
            assert_ne!(text, key, "{key} missing for {lang:?}");
        }
        assert!(t(lang, "session_unsaved_body").contains("{name}"));
    }
}
