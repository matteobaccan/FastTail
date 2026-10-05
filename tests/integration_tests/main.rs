// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Integration tests, one binary (`cargo test --test integration_tests`): one module per
//! feature in this folder, the helpers they share (`write_lines`, `wait_for_jobs`, …)
//! here. Keep it a single binary: every file directly under `tests/` is linked as a
//! program of its own, which costs far more time than it saves.
#![allow(
    clippy::field_reassign_with_default,
    clippy::bool_assert_comparison,
    clippy::write_with_newline
)]

use fasttail::config::FastTailConfig;
use fasttail::i18n::{t, Language};
use fasttail::screensaver::MatrixScreensaver;
use fasttail::tail_engine::{HighlightRule, TailEngine};
use fasttail::theme::CyberTheme;
use std::io::Write;
use std::time::Duration;
use tempfile::NamedTempFile;

mod basics;
mod bookmark_report;
mod char_selection;
mod compare_lines;
mod external_tools;
mod filter_tabs;
mod line_wrap;
mod log_levels;
mod pattern_streams;
mod relative_time_windows;
mod review_regressions;
mod scratchpad;
mod structured_fields;

/// Serialized `fasttail.ini` bytes of `cfg`.
fn ini_bytes(cfg: &FastTailConfig) -> Vec<u8> {
    let mut buf = Vec::new();
    cfg.to_ini().write_to(&mut buf).unwrap();
    buf
}

// ---------------------------------------------------------------------------
// Regression tests for the 2026-09-17 code review findings
// ---------------------------------------------------------------------------

/// Writes a 7z holding `files`, one LZMA2 block each.
fn write_7z(path: &std::path::Path, files: &[(&str, &[u8])]) {
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter};
    let mut writer = ArchiveWriter::new(std::fs::File::create(path).unwrap()).unwrap();
    for (name, data) in files {
        writer
            .push_archive_entry(ArchiveEntry::new_file(name), Some(*data))
            .unwrap();
    }
    writer.finish().unwrap();
}

fn write_lines(path: &std::path::Path, lines: &[&str]) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

/// Polls the engine until no background scan is running (or 30 s passed).
fn wait_for_jobs(engine: &mut TailEngine) {
    let start = std::time::Instant::now();
    loop {
        engine.poll_updates();
        if engine.scan_progress().is_none() && !engine.index_pending {
            return;
        }
        assert!(
            start.elapsed().as_secs() < 30,
            "background scan did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn write_scenario_log(path: &std::path::Path, lines: usize) {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 0..lines {
        let level = match i % 10 {
            0 => "ERROR",
            1 | 2 => "WARN",
            _ => "INFO",
        };
        writeln!(
            f,
            "2026-09-19 10:00:{:02} [{level}] svc-{} req={i} payload {}",
            i % 60,
            i % 7,
            "p".repeat(i % 50)
        )
        .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Log level detection, level cache, minimum-level filter and counters
// ---------------------------------------------------------------------------

fn write_level_log(path: &std::path::Path) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    writeln!(f, "2026-09-18 12:00:00 [INFO] service up").unwrap();
    writeln!(f, "2026-09-18 12:00:01 [DEBUG] cache warm").unwrap();
    writeln!(f, "2026-09-18 12:00:02 [WARN] slow query").unwrap();
    writeln!(f, "2026-09-18 12:00:03 [ERROR] payment failed").unwrap();
    writeln!(f, "    at com.example.Pay(Pay.java:42)").unwrap();
    writeln!(f, "    at com.example.Main(Main.java:7)").unwrap();
    writeln!(f, "plain line without a level").unwrap();
    writeln!(f, "<2>kernel: out of memory").unwrap();
    writeln!(f, "2026-09-18 12:00:05 [TRACE] tick").unwrap();
    writeln!(f, "INFO user typed \"error\" in the search box").unwrap();
}

fn expected_svc3_warn_or_error(lines: usize) -> usize {
    (0..lines).filter(|i| i % 10 < 3 && i % 7 == 3).count()
}

// ---------------------------------------------------------------------------
// Line wrap: per-stream toggle persisted in the workspace, i18n keys.
// ---------------------------------------------------------------------------

mod capture_group_highlight;

// ----- Pattern streams (directory wildcard tail) -----

fn write_file_with_mtime(path: &std::path::Path, content: &str, secs_ago: u64) {
    std::fs::write(path, content).unwrap();
    let when = std::time::SystemTime::now() - Duration::from_secs(secs_ago);
    let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(when).unwrap();
}

// ===== External tools =====

#[cfg(windows)]
fn quick_exit_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "cmd", "/c exit")
}
#[cfg(not(windows))]
fn quick_exit_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "true", "")
}

#[cfg(windows)]
fn slow_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "ping", "-n 3 127.0.0.1")
}
#[cfg(not(windows))]
fn slow_tool(name: &str) -> fasttail::external_tools::ExternalTool {
    fasttail::external_tools::ExternalTool::new(name, "sleep", "2")
}

// ---------------------------------------------------------------------------
// Named sessions
// ---------------------------------------------------------------------------

mod named_sessions;

mod timestamp_range;

// ---------------------------------------------------------------------------
// Background timestamp scan: the time range and go-to-time on large streams.
// ---------------------------------------------------------------------------

mod background_timestamps;

// ----- ANSI escape sequences (render / strip / raw) -----

mod ansi_escape_codes;

// ---------------------------------------------------------------------------
// Search results pane: the match cap with the true total, error block counts, the
// pane preferences and the pane keys.
// ---------------------------------------------------------------------------

mod search_results_pane;

// ----- Timeline histogram -----

mod timeline_histogram;

mod time_delta;

mod search_all_streams;

mod filter_terms_and_presets;

mod stdin_stream;

mod global_filter;

mod show_in_context;

mod dialogs;

mod bookmark_notes;

mod archive_formats;

mod collapse_repeated;

mod time_range_popup;

// ----- Line numbers and time delta per stream -----

mod per_stream_columns;

mod context_lines_around_matches;

mod automatic_highlighting;

mod rule_set_files;

mod selection_highlight;

mod rule_navigation;

mod time_display;

mod search_scope;

mod workspace;
mod minimised_window;
