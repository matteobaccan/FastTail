// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::source_links::{self, Target};
use fasttail::ui::app::FastTailApp;

/// A source tree with `com/acme/Orders.java` under `src/main/java`, and a log naming it.
fn fixture(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let java = dir.join("shop/src/main/java/com/acme");
    std::fs::create_dir_all(&java).unwrap();
    let lines: Vec<String> = (1..=60).map(|i| format!("    line {i};")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&java.join("Orders.java"), &refs);
    let log = dir.join("app.log");
    write_lines(
        &log,
        &[
            "2026-10-28 10:00:00 ERROR save failed",
            "java.lang.IllegalStateException: no stock",
            "\tat com.acme.Orders.save(Orders.java:42)",
            "\tat com.acme.Api.post(Api.java:7)",
        ],
    );
    (log, java.join("Orders.java"))
}

#[test]
fn a_frame_opens_its_file_at_the_line_in_fasttail() {
    let dir = tempfile::tempdir().unwrap();
    let (log, source) = fixture(dir.path());
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        source_roots: vec![
            dir.path().join("nowhere"),
            dir.path().join("shop/src/main/java"),
        ],
        ..Default::default()
    });
    app.open_log_file(log.clone());
    let text = app.engines[0].get_line(2).unwrap().to_string();
    let loc = source_links::find(&text).remove(0);
    assert_eq!((loc.path.as_str(), loc.line), ("Orders.java", 42));
    app.engines[0].source_request = Some((loc, Target::Stream));
    app.apply_merged_views();
    assert_eq!(app.engines.len(), 2, "the source file opened as a stream");
    let opened = app
        .engines
        .iter()
        .find(|e| fasttail::paths::paths_equal(&e.path, &source))
        .expect("Orders.java is open");
    assert_eq!(opened.selection_anchor, Some(41), "line 42 selected");
}

#[test]
fn a_location_not_under_the_roots_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (log, _) = fixture(dir.path());
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    });
    app.open_log_file(log);
    let text = app.engines[0].get_line(3).unwrap().to_string();
    let loc = source_links::find(&text).remove(0);
    app.engines[0].source_request = Some((loc, Target::Editor));
    app.apply_merged_views();
    assert_eq!(app.engines.len(), 1, "nothing opened");
    let notice = app.engines[0].view_notice.clone().unwrap_or_default();
    assert!(
        notice.contains("Api.java") && notice.contains("not found"),
        "{notice}"
    );
}
