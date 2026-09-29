// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::compressed::{entry_path, EntryRefusal, JobState};
use fasttail::config::FastTailConfig;
use fasttail::session::{Session, StreamEntry, SESSION_SUFFIX};
use fasttail::tail_engine::TailEngine;
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn app(dir: &Path) -> FastTailApp {
    FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.join("spool")),
        ..FastTailConfig::default()
    })
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn xz(data: &[u8]) -> Vec<u8> {
    let options = lzma_rust2::XzOptions::with_preset(1);
    let mut enc = lzma_rust2::XzWriter::new(Vec::new(), options).unwrap();
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn zstd(data: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
}

fn lines(prefix: &str, count: usize) -> Vec<u8> {
    (0..count)
        .map(|i| format!("{prefix} line {i}\n"))
        .collect::<String>()
        .into_bytes()
}

/// A tar of `(name, data)` files; a name ending with `/` is a directory.
fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let mut header = tar::Header::new_gnu();
        if name.ends_with('/') {
            header.set_entry_type(tar::EntryType::Directory);
        }
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        builder.append_data(&mut header, name, *data).unwrap();
    }
    builder.into_inner().unwrap()
}

/// A tar header written as it is, for the names and types the builder refuses.
fn raw_entry(out: &mut Vec<u8>, name: &str, kind: tar::EntryType, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_entry_type(kind);
    header.set_mode(0o644);
    header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
    header.set_cksum();
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
}

/// Polls the engine until its decompression is over and indexed.
fn settle(engine: &mut TailEngine) {
    let start = Instant::now();
    while !engine.compressed.as_ref().unwrap().is_finalized() {
        assert!(start.elapsed() < Duration::from_secs(20), "never settled");
        engine.poll_updates();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Lets the picker pull its scan until the scan is over (or the picker closed).
fn finish_scan(app: &mut FastTailApp) {
    let start = Instant::now();
    loop {
        app.poll_archive_picker();
        let Some(picker) = app.archive_picker.as_ref() else {
            return;
        };
        if picker.scan_state().is_over() {
            // One more pull: the rows listed just before the end, and the end itself.
            app.poll_archive_picker();
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "scan never ended"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_tarball_with_one_log_opens_it_once_scanned() {
    let dir = tempfile::tempdir().unwrap();
    let tgz = dir.path().join("app.tgz");
    std::fs::write(
        &tgz,
        gzip(&tar(&[("logs/", b""), ("logs/app.log", &lines("app", 50))])),
    )
    .unwrap();
    let mut app = app(dir.path());
    app.open_log_file(tgz.clone());
    assert!(app.archive_picker.is_some(), "the picker opens at once");
    assert!(app.engines.is_empty());
    finish_scan(&mut app);
    assert!(app.archive_picker.is_none(), "the picker closed");
    assert_eq!(app.engines.len(), 1);
    let engine = &mut app.engines[0];
    assert_eq!(engine.path, entry_path(&tgz, "logs/app.log"));
    assert_eq!(
        engine.compressed.as_ref().unwrap().title(),
        "app.tgz › logs/app.log"
    );
    settle(engine);
    assert_eq!(engine.total_lines(), 50);
}

#[test]
fn a_tar_bundle_lists_its_entries_and_refuses_the_unsafe_ones() {
    let dir = tempfile::tempdir().unwrap();
    let mut bundle = Vec::new();
    raw_entry(
        &mut bundle,
        "../../etc/passwd",
        tar::EntryType::Regular,
        b"root\n",
    );
    raw_entry(&mut bundle, "/abs.log", tar::EntryType::Regular, b"abs\n");
    raw_entry(&mut bundle, "current.log", tar::EntryType::Symlink, b"");
    raw_entry(&mut bundle, "app.log", tar::EntryType::Regular, b"app\n");
    raw_entry(
        &mut bundle,
        "var/log/messages",
        tar::EntryType::Regular,
        &lines("messages", 20),
    );
    bundle.extend_from_slice(&[0u8; 1024]);
    let sos = dir.path().join("sosreport.tar.xz");
    std::fs::write(&sos, xz(&bundle)).unwrap();

    let mut app = app(dir.path());
    app.open_log_file(sos.clone());
    finish_scan(&mut app);
    let picker = app
        .archive_picker
        .as_ref()
        .expect("several entries: picker");
    let rows: Vec<(&str, bool)> = picker
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.refusal.is_none()))
        .collect();
    assert_eq!(
        rows,
        [
            ("../../etc/passwd", false),
            ("/abs.log", false),
            ("current.log", false),
            ("app.log", true),
            ("var/log/messages", true),
        ]
    );
    assert_eq!(picker.entries[2].refusal, Some(EntryRefusal::LinkOrSpecial));
    assert!(app.engines.is_empty(), "nothing opens by itself");

    // Each chosen entry is its own stream, titled archive › entry.
    app.open_log_file(entry_path(&sos, "var/log/messages"));
    assert_eq!(app.engines.len(), 1);
    let engine = &mut app.engines[0];
    assert_eq!(
        engine.compressed.as_ref().unwrap().title(),
        "sosreport.tar.xz › var/log/messages"
    );
    settle(engine);
    assert_eq!(engine.total_lines(), 20);
    // Only the spool was written: nothing next to the archive.
    let beside: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        beside
            .iter()
            .all(|n| n == "sosreport.tar.xz" || n == "spool"),
        "{beside:?}"
    );
}

#[test]
fn xz_and_zstd_rotated_logs_open_decompressed() {
    let dir = tempfile::tempdir().unwrap();
    let syslog = dir.path().join("syslog.2.xz");
    std::fs::write(&syslog, xz(&lines("syslog", 300))).unwrap();
    let journal = dir.path().join("journal.zst");
    let mut frames = zstd(&lines("first", 10));
    frames.extend(zstd(&lines("second", 10)));
    frames.extend(zstd(&lines("third", 10)));
    std::fs::write(&journal, frames).unwrap();

    let mut app = app(dir.path());
    app.open_log_file(syslog.clone());
    app.open_log_file(journal.clone());
    assert_eq!(app.engines.len(), 2);
    for engine in app.engines.iter_mut() {
        assert!(engine.is_compressed());
        assert!(!engine.follow_tail);
        settle(engine);
        assert_eq!(engine.compressed.as_ref().unwrap().state(), JobState::Done);
    }
    assert_eq!(app.engines[0].total_lines(), 300);
    let journal = &app.engines[1];
    assert_eq!(journal.total_lines(), 30);
    assert_eq!(journal.get_line(0).as_deref(), Some("first line 0"));
    assert_eq!(journal.get_line(10).as_deref(), Some("second line 0"));
    assert_eq!(journal.get_line(29).as_deref(), Some("third line 9"));
}

#[test]
fn a_tar_entry_session_round_trip_restores_its_bookmarks() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle.tar.gz");
    std::fs::write(
        &bundle,
        gzip(&tar(&[
            ("var/log/", b""),
            ("var/log/server.log", &lines("server", 100)),
            ("var/log/worker.log", &lines("worker", 5)),
        ])),
    )
    .unwrap();
    let server = entry_path(&bundle, "var/log/server.log");
    let session = Session {
        streams: vec![StreamEntry {
            path: server.clone(),
            bookmarks: vec![4, 90],
            bookmark_notes: [(90, "restart here".to_string())].into_iter().collect(),
            archive_entry: Some("var/log/server.log".to_string()),
            ..StreamEntry::default()
        }],
        dock_layout: None,
    };
    let file = dir.path().join(format!("bundle{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("entry=var/log/server.log"), "{text}");
    assert!(text.contains("rel=bundle.tar.gz"), "{text}");
    let loaded = Session::load_from(&file).unwrap();
    assert!(loaded.missing.is_empty(), "{:?}", loaded.missing);
    assert_eq!(loaded.session, session);

    // Loading it extracts the entry again in the background, without the picker.
    let mut app = app(dir.path());
    app.load_session_file(file, true);
    assert!(app.archive_picker.is_none());
    assert_eq!(app.engines.len(), 1);
    let engine = &mut app.engines[0];
    assert_eq!(engine.path, server);
    settle(engine);
    assert_eq!(
        engine.bookmarks.iter().copied().collect::<Vec<_>>(),
        [4, 90]
    );
    // The note travels with its bookmark through the pending merge.
    assert_eq!(engine.bookmark_note(90), Some("restart here"));
    assert_eq!(engine.bookmark_note(4), None);
}
