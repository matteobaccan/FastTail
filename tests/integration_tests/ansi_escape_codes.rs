use super::wait_for_jobs;
use fasttail::ansi::{AnsiColor, AnsiMode};
use fasttail::config::FastTailConfig;
use fasttail::log_level::LogLevel;
use fasttail::session::{Session, StreamEntry};
use fasttail::tail_engine::{
    HighlightRule, QuickLabel, SpanStyle, TailEngine, ViewMode, MAX_ROW_SPANS,
};
use std::io::Write;
use std::path::{Path, PathBuf};

const COLOURED: &str = "\x1b[32mINFO \x1b[0mstarted\n\
                            \x1b[31mERROR\x1b[0m payment failed\n\
                            plain line\n";

fn log_with(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker.log");
    std::fs::write(&path, text).unwrap();
    (dir, path)
}

fn append(path: &Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

/// A coloured container log: every 10th line an error, colour codes around the
/// level and the timestamp, like `docker logs` of a colourised logger.
fn write_coloured_log(path: &Path, lines: usize) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 0..lines {
        let (colour, level) = match i % 10 {
            0 => (31, "ERROR"),
            1 | 2 => (33, "WARN"),
            _ => (32, "INFO"),
        };
        writeln!(
                f,
                "\x1b[2m2026-09-19T10:{:02}:{:02}.000Z\x1b[0m \x1b[{colour}m{level}\x1b[0m payment svc-{} req={i}",
                (i / 60) % 60,
                i % 60,
                i % 7
            )
            .unwrap();
    }
}

#[test]
fn coloured_log_resolves_to_render_and_hides_the_codes() {
    let (_dir, path) = log_with(COLOURED);
    let engine = TailEngine::open(&path).unwrap();
    assert_eq!(engine.ansi_mode, AnsiMode::Auto);
    assert_eq!(engine.ansi_effective(), AnsiMode::Render);
    assert_eq!(engine.get_line(0).as_deref(), Some("INFO started"));
    assert_eq!(engine.get_line(1).as_deref(), Some("ERROR payment failed"));
    let row = engine.get_row(1).unwrap();
    assert_eq!(row.line, "ERROR payment failed");
    assert_eq!(row.ansi.len(), 1);
    assert_eq!((row.ansi[0].start, row.ansi[0].end), (0, 5));
    assert_eq!(row.ansi[0].style.fg, Some(AnsiColor::Indexed(1)));
    assert!(!row.visible_escapes);
    let spans = engine.match_row_spans(&row);
    assert_eq!(spans.spans.len(), 1);
    assert!(matches!(spans.spans[0].style, SpanStyle::Ansi(_)));
    let (shown, _) = row.display(Some(spans));
    assert_eq!(shown, "ERROR payment failed");
}

#[test]
fn plain_log_stays_auto_and_raw() {
    let (_dir, path) = log_with("2026-09-19 INFO one\n2026-09-19 ERROR two\n");
    let engine = TailEngine::open(&path).unwrap();
    assert_eq!(engine.ansi_mode, AnsiMode::Auto);
    assert_eq!(engine.ansi_effective(), AnsiMode::Raw);
    let row = engine.get_row(1).unwrap();
    assert!(row.ansi.is_empty() && !row.visible_escapes);
    assert_eq!(row.line, "2026-09-19 ERROR two");
    assert_eq!(engine.level_of(1), LogLevel::Error);
    // A non-SGR sequence (a window title) does not switch auto to render.
    let (_dir, path) = log_with("\x1b]0;title\x07 INFO x\n");
    assert_eq!(
        TailEngine::open(&path).unwrap().ansi_effective(),
        AnsiMode::Raw
    );
}

#[test]
fn level_and_filters_see_the_stripped_line() {
    let (_dir, path) = log_with(COLOURED);
    let mut engine = TailEngine::open(&path).unwrap();
    assert_eq!(engine.level_of(0), LogLevel::Info);
    assert_eq!(engine.level_of(1), LogLevel::Error);
    assert_eq!(engine.level_count(LogLevel::Error), 1);
    engine.set_min_level(LogLevel::Warn);
    assert_eq!(engine.filtered_lines, vec![1]);
    engine.set_min_level(LogLevel::Unknown);
    engine.filter_is_regex = true;
    engine.set_include_filter(r"\bERROR\b");
    assert_eq!(engine.filtered_lines, vec![1]);
    // Search across the position of a removed sequence.
    engine.set_include_filter("");
    engine.update_search("error payment");
    assert_eq!(engine.search_matches, vec![1]);
}

#[test]
fn raw_mode_shows_and_matches_the_escape_bytes() {
    let (_dir, path) = log_with(COLOURED);
    let mut engine = TailEngine::open(&path).unwrap();
    let generation = engine.buffer_generation;
    engine.set_ansi_mode(AnsiMode::Raw);
    assert!(engine.ansi_dirty);
    assert!(
        engine.buffer_generation != generation,
        "caches keyed by it rebuild"
    );
    assert_eq!(
        engine.get_line(1).as_deref(),
        Some("\x1b[31mERROR\x1b[0m payment failed")
    );
    // `[31mERROR` glues the token to `m`: raw text has no level any more.
    assert_eq!(engine.level_of(1), LogLevel::Unknown);
    let row = engine.get_row(1).unwrap();
    assert!(row.visible_escapes && row.ansi.is_empty());
    engine.set_quick_labels(&[QuickLabel {
        text: "payment".into(),
        color: 2,
    }]);
    let spans = engine.match_row_spans(&row);
    let (shown, spans) = row.display(Some(spans));
    assert_eq!(shown, "␛[31mERROR␛[0m payment failed");
    let sp = spans.unwrap().spans[0];
    assert_eq!(
        &shown[sp.start..sp.end],
        "payment",
        "spans follow the glyphs"
    );
    engine.filter_is_regex = true;
    engine.set_include_filter(r"\x1b\[31m");
    assert_eq!(engine.filtered_lines, vec![1]);
    // Strip: codes hidden, nothing painted.
    engine.set_include_filter("");
    engine.set_ansi_mode(AnsiMode::Strip);
    let row = engine.get_row(1).unwrap();
    assert_eq!(row.line, "ERROR payment failed");
    assert!(row.ansi.is_empty() && !row.visible_escapes);
    assert_eq!(engine.level_of(1), LogLevel::Error);
}

#[test]
fn copy_and_export_follow_the_mode() {
    let (_dir, path) = log_with(COLOURED);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.select_row(0);
    engine.extend_selection_to(1);
    let copied = engine.copy_selection_text().unwrap();
    assert_eq!(copied, "INFO started\nERROR payment failed");
    let mut out = Vec::new();
    engine.export_visible(&mut out).unwrap();
    assert!(!out.contains(&0x1b));
    let ctx = fasttail::ui::dock::tool_context_for_row(&engine, 1).unwrap();
    assert_eq!(ctx.line, "ERROR payment failed");

    engine.set_ansi_mode(AnsiMode::Strip);
    engine.select_row(1);
    assert_eq!(
        engine.copy_selection_text().unwrap(),
        "ERROR payment failed"
    );
    engine.set_ansi_mode(AnsiMode::Raw);
    engine.select_row(1);
    assert_eq!(
        engine.copy_selection_text().unwrap(),
        "\x1b[31mERROR\x1b[0m payment failed",
        "raw copies exactly the file's text"
    );
}

#[test]
fn colours_deep_in_a_large_append_are_detected() {
    let (_dir, path) = log_with("started\n");
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    // One append far larger than the detection sample, the colour at its end.
    let mut burst: String = (0..20_000).map(|i| format!("plain line {i}\n")).collect();
    assert!(burst.len() > 4 * fasttail::ansi::DETECT_SAMPLE_BYTES);
    burst.push_str("\x1b[31mERROR\x1b[0m payment failed\n");
    append(&path, &burst);
    engine.poll_updates();
    wait_for_jobs(&mut engine);
    assert_eq!(engine.ansi_effective(), AnsiMode::Render);
    assert_eq!(
        engine.get_line(20_001).as_deref(),
        Some("ERROR payment failed")
    );

    // Opened empty, like a decompression spool: the first append is all new bytes.
    let (_dir, path) = log_with("");
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    append(&path, &burst);
    engine.poll_updates();
    wait_for_jobs(&mut engine);
    assert_eq!(engine.ansi_effective(), AnsiMode::Render);

    // At open only the head is sampled: a file is not read end to end to decide.
    let (_dir, path) = log_with(&burst);
    let engine = TailEngine::open(&path).unwrap();
    assert_eq!(engine.ansi_effective(), AnsiMode::Raw);
}

#[test]
fn colours_after_the_banner_switch_auto_once_and_rescan() {
    let banner: String = (0..10_000).map(|i| format!("banner {i}\n")).collect();
    let (_dir, path) = log_with(&banner);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = std::time::Duration::ZERO;
    engine.set_include_filter("ERROR payment");
    engine.update_search("error payment");
    assert!(engine.filtered_lines.is_empty());
    assert_eq!(engine.ansi_effective(), AnsiMode::Raw);

    append(&path, "\x1b[31mERROR\x1b[0m payment failed\n");
    engine.poll_updates();
    assert_eq!(engine.ansi_effective(), AnsiMode::Render);
    assert!(engine.ansi_switch_notice());
    assert_eq!(engine.filtered_lines, vec![10_000]);
    assert_eq!(engine.search_matches, vec![10_000]);
    assert_eq!(engine.level_of(10_000), LogLevel::Error);
    assert!(engine.levels_complete());
    assert_eq!(engine.level_count(LogLevel::Error), 1);
    assert_eq!(engine.ansi_mode, AnsiMode::Auto, "resolved, not chosen");
    assert!(!engine.ansi_dirty);
}

#[test]
fn background_scans_match_the_synchronous_path() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("coloured.log");
    write_coloured_log(&log, 30_000);

    let mut sync = TailEngine::open(&log).unwrap();
    assert_eq!(sync.ansi_effective(), AnsiMode::Render);
    sync.set_include_filter("ERROR payment");
    assert!(sync.scan_progress().is_none());
    let expected = sync.filtered_lines.clone();
    assert_eq!(expected.len(), 3_000);
    sync.set_include_filter("");
    sync.update_search("warn payment");
    let expected_search = sync.search_matches.clone();
    assert_eq!(expected_search.len(), 6_000);
    sync.ensure_timestamps();

    let mut bg = TailEngine::open_with_thresholds(&log, 0, 0).unwrap();
    wait_for_jobs(&mut bg);
    assert_eq!(bg.ansi_effective(), AnsiMode::Render);
    assert_eq!(
        bg.level_count(LogLevel::Error),
        3_000,
        "background level scan"
    );
    bg.set_include_filter("ERROR payment");
    assert!(bg.scan_progress().is_some(), "large-file path spawns a job");
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, expected);
    bg.set_include_filter("");
    wait_for_jobs(&mut bg);
    bg.update_search("warn payment");
    wait_for_jobs(&mut bg);
    assert_eq!(bg.search_matches, expected_search);
    // Timestamps behind a colour code, timed on a worker thread.
    bg.resolve_goto("10:05:00", 0);
    wait_for_jobs(&mut bg);
    assert_eq!(bg.timestamp_cache(), sync.timestamp_cache());
    assert!(bg.timestamps_usable());

    // A mode change restarts the scans on the new text.
    bg.update_search("");
    bg.set_include_filter("ERROR payment");
    wait_for_jobs(&mut bg);
    bg.set_ansi_mode(AnsiMode::Raw);
    wait_for_jobs(&mut bg);
    assert!(
        bg.filtered_lines.is_empty(),
        "raw text has codes in between"
    );
    assert_eq!(bg.level_count(LogLevel::Error), 0);
    bg.set_ansi_mode(AnsiMode::Render);
    wait_for_jobs(&mut bg);
    assert_eq!(bg.filtered_lines, expected);
    assert_eq!(bg.level_count(LogLevel::Error), 3_000);
}

#[test]
fn user_rules_and_labels_rank_above_ansi_colours() {
    let (_dir, path) =
        log_with("\x1b[31mERROR\x1b[0m payment sess-8f3a \x1b[33mreq=42 slow\x1b[0m\n");
    let mut engine = TailEngine::open(&path).unwrap();
    let row = engine.get_row(0).unwrap();
    assert_eq!(row.line, "ERROR payment sess-8f3a req=42 slow");

    // A whole-row rule colours the whole row.
    engine.set_highlight_rules(vec![HighlightRule::new(
        "payment",
        [0, 255, 0],
        [0, 0, 0],
        false,
    )]);
    let hl = engine.match_row_spans(&row);
    assert!(hl.spans.is_empty());
    assert_eq!(
        hl.rest.map(|s| s.fg),
        Some(egui::Color32::from_rgb(0, 255, 0))
    );

    // A capture inside a yellow ANSI span, and a quick label: both win on their bytes.
    engine.set_highlight_rules(vec![HighlightRule::captures(
        r"req=(\d+)",
        [0, 255, 255],
        [0, 0, 0],
    )]);
    engine.set_quick_labels(&[QuickLabel {
        text: "sess-8f3a".into(),
        color: 5,
    }]);
    let hl = engine.match_row_spans(&row);
    assert!(hl.rest.is_none());
    let text = &row.line;
    let pieces: Vec<(&str, &str)> = hl
        .spans
        .iter()
        .map(|s| {
            let kind = match s.style {
                SpanStyle::Rule(_) => "rule",
                SpanStyle::Label(_) => "label",
                SpanStyle::Ansi(_) => "ansi",
                SpanStyle::Token(_) => "token",
            };
            (&text[s.start..s.end], kind)
        })
        .collect();
    assert_eq!(
        pieces,
        vec![
            ("ERROR", "ansi"),
            ("sess-8f3a", "label"),
            ("req=", "ansi"),
            ("42", "rule"),
            (" slow", "ansi"),
        ]
    );
}

#[test]
fn ansi_spans_share_the_row_budget() {
    let line: String = (0..200)
        .map(|i| format!("\x1b[3{}mx{i} ", i % 8))
        .collect::<String>()
        + "\n";
    let (_dir, path) = log_with(&line);
    let mut engine = TailEngine::open(&path).unwrap();
    let row = engine.get_row(0).unwrap();
    assert!(row.ansi.len() > MAX_ROW_SPANS);
    assert_eq!(engine.match_row_spans(&row).spans.len(), MAX_ROW_SPANS);
    // A label claims first; the ANSI runs fill what is left of the budget.
    engine.set_quick_labels(&[QuickLabel {
        text: "x199".into(),
        color: 1,
    }]);
    let hl = engine.match_row_spans(&row);
    assert_eq!(hl.spans.len(), MAX_ROW_SPANS);
    assert!(hl.spans.iter().any(|s| s.style == SpanStyle::Label(1)));
}

#[test]
fn search_cursor_is_carried_to_the_file_bytes_in_hex() {
    let text = "payment payment ok\n\x1b[1;31mERROR\x1b[0m \x1b[4mpayment\x1b[0m failed\n";
    let (_dir, path) = log_with(text);
    let mut engine = TailEngine::open(&path).unwrap();
    engine.update_search("payment");
    assert_eq!(engine.search_matches, vec![0, 1]);
    engine.search_next(false);
    assert_eq!(engine.current_search_line(), Some(1));
    engine.set_view_mode(ViewMode::Hex);
    let expected = text.rfind("payment").unwrap();
    assert_eq!(engine.current_search_byte(), Some((expected, 7)));
    assert_eq!(engine.scroll_to_byte, Some(expected));
    // HEX shows the file bytes, escapes included.
    let bytes = engine.get_bytes(19, 4).unwrap();
    assert_eq!(bytes, b"\x1b[1;");
}

#[test]
fn utf16_logs_are_detected_and_stripped() {
    let text = "\x1b[31mERROR\x1b[0m boom\r\nplain\r\n";
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("u16.log");
    std::fs::write(&path, bytes).unwrap();
    let engine = TailEngine::open(&path).unwrap();
    assert_eq!(engine.ansi_effective(), AnsiMode::Render);
    assert_eq!(engine.get_line(0).as_deref(), Some("ERROR boom"));
    assert_eq!(engine.level_of(0), LogLevel::Error);
}

#[test]
fn line_cut_by_the_length_cap_keeps_its_marker() {
    let mut line = "\x1b[31m".to_string();
    line.push_str(&"a".repeat(fasttail::tail_engine::MAX_LINE_BYTES));
    line.push_str("\x1b]8;;cut after the cap\n");
    let (_dir, path) = log_with(&line);
    let engine = TailEngine::open(&path).unwrap();
    let text = engine.get_line(0).unwrap();
    assert!(text.ends_with(fasttail::tail_engine::TRUNCATED_LINE_MARKER));
    assert!(!text.contains('\x1b'));
}

#[test]
fn mode_is_persisted_in_sessions_and_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    std::fs::write(&log, "a\n").unwrap();
    let mut entry = StreamEntry::new(log.clone());
    entry.ansi = Some("raw".to_string());
    let session = Session {
        streams: vec![entry.clone()],
        dock_layout: None,
    };
    let file = dir.path().join("s.fasttail-session.ini");
    session.save_to(&file).unwrap();
    let loaded = Session::load_from(&file).unwrap();
    assert_eq!(loaded.session.streams[0].ansi.as_deref(), Some("raw"));

    // The workspace keeps it in the stream entry of the default session.
    let mut cfg = FastTailConfig::default();
    cfg.open_files = vec![log.clone()];
    cfg.set_stream_state(entry);
    let restored = FastTailConfig::from_ini(&cfg.to_ini());
    assert_eq!(
        restored.stream_state_for(&log).unwrap().ansi.as_deref(),
        Some("raw")
    );

    // Old files without the key, and an explicit auto, read as auto.
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("ansi=raw"));
    std::fs::write(&file, text.replace("ansi=raw", "")).unwrap();
    assert_eq!(
        Session::load_from(&file).unwrap().session.streams[0].ansi,
        None
    );
    std::fs::write(&file, text.replace("ansi=raw", "ansi=auto")).unwrap();
    assert_eq!(
        Session::load_from(&file).unwrap().session.streams[0].ansi,
        None
    );
    // Auto is not written at all.
    let auto = Session {
        streams: vec![StreamEntry::new(log)],
        dock_layout: None,
    };
    assert!(!auto.serialized(None).contains("ansi"));
}

#[test]
fn ansi_i18n_keys() {
    for lang in fasttail::i18n::Language::ALL {
        for key in [
            "ansi_auto",
            "ansi_render",
            "ansi_strip",
            "ansi_raw",
            "tip_ansi",
            "ansi_switched",
        ] {
            assert_ne!(
                fasttail::i18n::t(*lang, key),
                "Unknown",
                "{key} for {lang:?}"
            );
        }
    }
}
