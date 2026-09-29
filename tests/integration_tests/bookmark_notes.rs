// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::session::{Session, StreamEntry, SESSION_SUFFIX};
use fasttail::tail_engine::{HighlightRule, TailEngine};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Notes the ini parser would bend: quotes at the ends, edge spaces, backslashes and
/// text outside ASCII.
const NOTES: [&str; 5] = [
    "first OOM",
    "\"quoted\" it's",
    "  edge spaces  ",
    "città → 東京",
    "C:\\logs\\a.log",
];

fn notes_on(lines: &[usize]) -> BTreeMap<usize, String> {
    lines
        .iter()
        .zip(NOTES)
        .map(|(l, n)| (*l, n.to_string()))
        .collect()
}

fn ini_text(cfg: &FastTailConfig) -> String {
    let mut buf = Vec::new();
    cfg.to_ini().write_to(&mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

fn reload(cfg: &FastTailConfig) -> FastTailConfig {
    let text = ini_text(cfg);
    FastTailConfig::from_ini(&ini::Ini::load_from_str(&text).unwrap())
}

fn bookmarking(pattern: &str) -> HighlightRule {
    let mut rule = HighlightRule::new(pattern, [255, 255, 255], [0, 0, 0], false);
    rule.auto_bookmark = true;
    rule
}

#[test]
fn notes_round_trip_through_fasttail_ini() {
    let path = PathBuf::from(if cfg!(windows) {
        r"C:\logs\app.log"
    } else {
        "/var/log/app.log"
    });
    let lines = [10, 20, 30, 40, 200];
    let mut cfg = FastTailConfig::default();
    cfg.set_bookmarks_with_notes(&path, &lines, &notes_on(&lines));
    let text = ini_text(&cfg);
    assert!(text.contains("lines_0=10,20,30,40,200"), "{text}");
    assert!(text.contains("note_0_10=first OOM"), "{text}");

    let loaded = reload(&cfg);
    let (restored, notes) = loaded.bookmarks_with_notes_for(&path, 1000).unwrap();
    assert_eq!(restored, lines);
    assert_eq!(notes, notes_on(&lines));
    // A file that shrank below the largest saved line drops bookmarks and notes.
    assert!(loaded.bookmarks_with_notes_for(&path, 50).is_none());
}

#[test]
fn notes_of_unsaved_lines_are_ignored_and_old_files_load_as_before() {
    let text = "[bookmarks]\nfile_0=/logs/a.log\nlines_0=10,200\nnote_0_200=kept\nnote_0_7=stray\n\
                    file_1=/logs/b.log\nlines_1=3\n";
    let cfg = FastTailConfig::from_ini(&ini::Ini::load_from_str(text).unwrap());
    let (lines, notes) = cfg
        .bookmarks_with_notes_for(&PathBuf::from("/logs/a.log"), 500)
        .unwrap();
    assert_eq!(lines, vec![10, 200]);
    assert_eq!(
        notes.into_iter().collect::<Vec<_>>(),
        vec![(200, "kept".to_string())]
    );
    // Written before notes existed: bookmarks without notes.
    let (lines, notes) = cfg
        .bookmarks_with_notes_for(&PathBuf::from("/logs/b.log"), 500)
        .unwrap();
    assert_eq!(lines, vec![3]);
    assert!(notes.is_empty());
}

#[test]
fn the_rule_option_is_saved_as_bookmark_and_defaults_to_off() {
    let mut cfg = FastTailConfig::default();
    cfg.highlight_rules = vec![bookmarking("FATAL")];
    let text = ini_text(&cfg);
    assert!(text.contains("bookmark=true"), "{text}");
    assert!(reload(&cfg).highlight_rules[0].auto_bookmark);

    let old = "[highlight_0]\npattern=ERROR\nis_regex=false\nfg=255,0,0\nbg=0,0,0\n";
    let cfg = FastTailConfig::from_ini(&ini::Ini::load_from_str(old).unwrap());
    assert_eq!(cfg.highlight_rules.len(), 1);
    assert!(!cfg.highlight_rules[0].auto_bookmark);
}

#[test]
fn the_cap_setting_is_saved_and_clamped() {
    let mut cfg = FastTailConfig::default();
    assert_eq!(cfg.auto_bookmark_max, 10_000);
    cfg.auto_bookmark_max = 50_000;
    assert_eq!(reload(&cfg).auto_bookmark_max, 50_000);
    for (written, read) in [("5", 100), ("999999", 100_000)] {
        let text = format!("[general]\nauto_bookmark_max={written}\n");
        let cfg = FastTailConfig::from_ini(&ini::Ini::load_from_str(&text).unwrap());
        assert_eq!(cfg.auto_bookmark_max, read);
    }
}

#[test]
fn automatic_bookmarks_are_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    let text: String = (0..60)
        .map(|i| {
            if i % 5 == 0 {
                format!("{i} OutOfMemoryError\n")
            } else {
                format!("{i} fine\n")
            }
        })
        .collect();
    std::fs::write(&log, text).unwrap();
    let mut engine = TailEngine::open(&log).unwrap();
    engine.set_highlight_rules(vec![bookmarking("OutOfMemoryError")]);
    assert_eq!(engine.auto_bookmarks().len(), 12);
    engine.toggle_bookmark(1);
    engine.toggle_bookmark(2);
    engine.set_bookmark_note(3, "manual");
    // What the app saves when the bookmarks are dirty.
    let lines: Vec<usize> = engine.bookmarks.iter().copied().collect();
    let mut cfg = FastTailConfig::default();
    cfg.set_bookmarks_with_notes(&engine.path, &lines, &engine.bookmark_notes);

    let loaded = reload(&cfg);
    let (saved, notes) = loaded.bookmarks_with_notes_for(&log, 60).unwrap();
    assert_eq!(saved, vec![1, 2, 3]);
    assert_eq!(notes.get(&3).map(String::as_str), Some("manual"));

    // Reopened: the manual ones come back, the automatic ones are found again.
    let mut reopened = TailEngine::open(&log).unwrap();
    reopened.set_highlight_rules(vec![bookmarking("OutOfMemoryError")]);
    reopened.set_bookmarks_with_notes(saved, notes);
    assert_eq!(reopened.bookmark_note(3), Some("manual"));
    assert_eq!(reopened.auto_bookmarks().len(), 12);
    assert!(!reopened.bookmarks_dirty);
}

#[test]
fn notes_round_trip_through_a_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(&log, "a\nb\n").unwrap();
    let mut entry = StreamEntry::new(log.clone());
    entry.bookmarks = vec![7, 10, 20, 30, 40];
    entry.bookmark_notes = notes_on(&[7, 10, 20, 30, 40]);
    entry
        .bookmark_notes
        .insert(99, "not a bookmark".to_string());
    let session = Session {
        streams: vec![entry.clone()],
        dock_layout: None,
    };
    let file = dir.path().join(format!("incident{SESSION_SUFFIX}"));
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("bookmarks=7,10,20,30,40"), "{text}");
    assert!(text.contains("bookmark_note.7=first OOM"), "{text}");
    assert!(!text.contains("bookmark_note.99"), "{text}");

    let loaded = Session::load_from(&file).unwrap();
    entry.bookmark_notes.remove(&99);
    assert_eq!(loaded.session.streams, vec![entry.clone()]);

    // Made the default workspace, the notes land in the bookmarks of the config.
    let mut cfg = FastTailConfig::default();
    loaded.session.apply_to_config(&mut cfg);
    let (_, notes) = cfg.bookmarks_with_notes_for(&log, 100).unwrap();
    assert_eq!(notes, entry.bookmark_notes);
    let default = Session::from_config(&cfg);
    assert_eq!(default.streams[0].bookmark_notes, entry.bookmark_notes);
}
