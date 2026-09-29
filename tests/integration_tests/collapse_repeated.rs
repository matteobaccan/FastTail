use super::wait_for_jobs;
use fasttail::collapse::CollapseMode;
use fasttail::config::FastTailConfig;
use fasttail::i18n::Language;
use fasttail::session::{Session, StreamEntry};
use fasttail::tail_engine::{TailEngine, TimeDelta};
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn write(dir: &Path, name: &str, lines: &[String]) -> PathBuf {
    let path = dir.join(name);
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(&path, text).unwrap();
    path
}

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

fn rows(engine: &TailEngine) -> Vec<usize> {
    (0..engine.visible_line_count())
        .filter_map(|r| engine.get_actual_line_idx(r))
        .collect()
}

/// `(first line, count)` of every group row.
fn groups(engine: &TailEngine) -> Vec<(usize, u32)> {
    (0..engine.visible_line_count())
        .filter_map(|r| engine.collapsed_row(r))
        .map(|c| (c.first_line, c.count))
        .collect()
}

fn exported(engine: &TailEngine) -> String {
    let mut out = Vec::new();
    engine.export_visible(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn append(path: &Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

fn poll_until(engine: &mut TailEngine, what: &str, done: impl Fn(&TailEngine) -> bool) {
    let start = Instant::now();
    while !done(engine) {
        assert!(start.elapsed() < Duration::from_secs(10), "{what} not seen");
        engine.poll_updates();
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_for_jobs(engine);
}

/// Ten distinct lines, 500 retries with 500 different bare clocks, one last line.
fn retry_log() -> Vec<String> {
    let mut lines: Vec<String> = (0..10).map(|i| format!("INFO boot step {i}")).collect();
    for i in 0..500 {
        lines.push(format!(
            "12:00:{:02}.{:03} WARN connection refused, retrying",
            i / 10,
            (i * 7) % 1000
        ));
    }
    lines.push("INFO connected".into());
    lines
}

#[test]
fn a_retry_loop_is_one_row_that_expands_and_collapses() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", &retry_log())).unwrap();
    assert_eq!(engine.visible_line_count(), 511, "off by default");
    engine.set_collapse_mode(CollapseMode::Exact);
    let expected: Vec<usize> = (0..=10).chain([510]).collect();
    assert_eq!(rows(&engine), expected);
    let group = engine.collapsed_row(10).expect("group row");
    assert_eq!(
        (group.count, group.first_line, group.last_line, group.open),
        (500, 10, 509, false)
    );
    assert_eq!(group.hidden, Some((11, 509)));
    assert_eq!(
        engine.get_visible_row_of_line(300),
        Some(10),
        "hidden: group row"
    );
    assert_eq!(engine.get_visible_row_of_line(510), Some(11));
    assert_eq!(engine.row_span(10), Some((10, 509)));
    assert_eq!(engine.visible_lines(), 511, "lines are still lines");

    assert!(engine.toggle_collapsed_row(10));
    assert_eq!(engine.visible_line_count(), 511);
    assert!(engine.collapsed_row(10).unwrap().open, "the badge stays");
    assert_eq!(engine.get_visible_row_of_line(300), Some(300));
    assert!(engine.toggle_collapsed_row(10));
    assert_eq!(engine.visible_line_count(), 12);
    assert!(!engine.toggle_collapsed_row(11), "not a group row");

    engine.set_collapse_mode(CollapseMode::Off);
    assert_eq!(engine.visible_line_count(), 511);
    assert_eq!(engine.collapsed_row(10), None);
}

#[test]
fn numbers_mode_masks_ids_that_exact_mode_keeps() {
    let dir = tempfile::tempdir().unwrap();
    let lines = strings(&[
        "user 41 fetched order 0x1f3a",
        "user 42 fetched order 0x1f3b",
        "user 43 fetched order 0x2000",
        "done",
    ]);
    let mut engine = TailEngine::open(write(dir.path(), "ids.log", &lines)).unwrap();
    engine.set_collapse_mode(CollapseMode::Numbers);
    assert_eq!(rows(&engine), vec![0, 3]);
    assert_eq!(groups(&engine), vec![(0, 3)]);
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(rows(&engine), vec![0, 1, 2, 3]);
}

#[test]
fn identical_stack_traces_collapse_as_whole_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines = Vec::new();
    for _ in 0..40 {
        lines.push("ERROR payment failed".to_string());
        for frame in 0..12 {
            lines.push(format!("    at Pay.step{frame}(Pay.java:{frame})"));
        }
    }
    lines.push("INFO next".to_string());
    let mut engine = TailEngine::open(write(dir.path(), "trace.log", &lines)).unwrap();
    engine.set_collapse_mode(CollapseMode::Exact);
    let expected: Vec<usize> = (0..13).chain([520]).collect();
    assert_eq!(rows(&engine), expected);
    assert_eq!(groups(&engine), vec![(0, 40)]);
    // The last shown frame stands for the hidden entries (time delta), the head row
    // selects them all.
    assert_eq!(engine.row_span(12), Some((12, 519)));
    engine.select_row(0);
    assert_eq!(engine.selected_lines().len(), 520);
}

#[test]
fn runs_are_formed_over_the_filtered_lines() {
    let dir = tempfile::tempdir().unwrap();
    let lines = strings(&["A", "B", "A"]);
    let mut engine = TailEngine::open(write(dir.path(), "abc.log", &lines)).unwrap();
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(rows(&engine), vec![0, 1, 2]);
    engine.set_exclude_filter("B");
    assert_eq!(rows(&engine), vec![0]);
    assert_eq!(groups(&engine), vec![(0, 2)]);
    engine.set_exclude_filter("");
    assert_eq!(rows(&engine), vec![0, 1, 2]);
}

#[test]
fn copy_writes_every_line_and_copy_as_shown_the_badge() {
    let dir = tempfile::tempdir().unwrap();
    let lines = strings(&["start", "retry 1", "retry 2", "retry 3", "end"]);
    let mut engine = TailEngine::open(write(dir.path(), "copy.log", &lines)).unwrap();
    engine.set_collapse_mode(CollapseMode::Numbers);
    assert_eq!(rows(&engine), vec![0, 1, 4]);

    engine.select_row(1);
    assert_eq!(engine.selected_lines(), vec![1, 2, 3]);
    assert_eq!(
        engine.copy_selection_text().as_deref(),
        Some("retry 1\nretry 2\nretry 3")
    );
    assert_eq!(
        engine.copy_selection_as_shown().as_deref(),
        Some("retry 1 ×3")
    );

    engine.select_all_visible();
    assert_eq!(engine.selected_lines(), vec![0, 1, 2, 3, 4]);
    assert!(engine.is_selected(2), "CTRL + A takes the hidden lines too");
    assert_eq!(
        engine.copy_selection_as_shown().as_deref(),
        Some("start\nretry 1 ×3\nend")
    );

    engine.select_row(0);
    engine.extend_selection_to(4);
    assert_eq!(engine.selected_lines(), vec![0, 1, 2, 3, 4]);
    engine.toggle_row(1);
    assert_eq!(engine.selected_lines(), vec![0, 4]);
    engine.toggle_row(1);
    assert_eq!(engine.selected_lines(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn export_writes_every_visible_line_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines = Vec::new();
    for block in 0..1000 {
        lines.push(format!("INFO step {block}"));
        lines.extend(std::iter::repeat_n("WARN retry".to_string(), 9));
        lines.push("DEBUG noise".to_string());
    }
    let mut engine = TailEngine::open(write(dir.path(), "export.log", &lines)).unwrap();
    engine.set_exclude_filter("noise");
    let plain = exported(&engine);
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(engine.visible_lines(), 10_000);
    assert_eq!(engine.visible_line_count(), 2_000);
    assert_eq!(exported(&engine), plain);
    assert_eq!(plain.lines().count(), 10_000);
}

/// A log with runs, traces, orphan frames and noise, for comparing the two paths.
fn mixed_log() -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..600 {
        match i % 6 {
            0 => lines.push(format!("2026-09-18T14:00:{:02}.000Z INFO tick {i}", i % 60)),
            1 | 2 => lines.push(format!("2026-09-18T14:00:{:02}.500Z WARN retry", i % 60)),
            3 => {
                lines.push("ERROR boom".to_string());
                lines.push("    at A.b(A.java:1)".to_string());
            }
            4 => lines.push(format!("DEBUG noise {i}")),
            _ => lines.push(format!("user {i} fetched order 0x{i:x}")),
        }
    }
    lines
}

#[test]
fn background_detection_matches_the_synchronous_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "mixed.log", &mixed_log());
    let mut sync = TailEngine::open(&path).unwrap();
    // Thresholds at 0: the detection runs as a background `Collapse` scan.
    let mut job = TailEngine::open_with_thresholds(&path, 0, 0).unwrap();
    wait_for_jobs(&mut job);
    for mode in [CollapseMode::Numbers, CollapseMode::Exact] {
        for exclude in ["", "noise"] {
            for engine in [&mut sync, &mut job] {
                engine.set_exclude_filter(exclude);
                engine.set_collapse_mode(mode);
                wait_for_jobs(engine);
                assert!(!engine.collapse_pending());
            }
            assert!(!groups(&sync).is_empty());
            assert_eq!(rows(&job), rows(&sync), "{mode:?} exclude {exclude:?}");
            assert_eq!(groups(&job), groups(&sync), "{mode:?} exclude {exclude:?}");
        }
    }
}

#[test]
fn follow_mode_grows_the_last_count_without_new_rows() {
    for (threshold, label) in [(u64::MAX, "synchronous"), (0, "background")] {
        let dir = tempfile::tempdir().unwrap();
        let mut lines = vec!["INFO start".to_string()];
        lines.extend(std::iter::repeat_n("heartbeat ok".to_string(), 12));
        let path = write(dir.path(), "follow.log", &lines);
        let mut engine = TailEngine::open_with_thresholds(&path, threshold, u64::MAX).unwrap();
        engine.size_check_interval = Duration::ZERO;
        engine.set_collapse_mode(CollapseMode::Exact);
        wait_for_jobs(&mut engine);
        assert_eq!(groups(&engine), vec![(1, 12)], "{label}");
        assert!(engine.follow_tail);

        append(&path, "heartbeat ok\nheartbeat ok\nheartbeat ok\n");
        poll_until(&mut engine, "append", |e| e.total_lines() == 16);
        assert_eq!(engine.visible_line_count(), 2, "{label}: no new row");
        assert_eq!(groups(&engine), vec![(1, 15)], "{label}");
        assert!(engine.follow_tail);

        // A different line ends the run.
        append(&path, "INFO after\nheartbeat ok\n");
        poll_until(&mut engine, "second append", |e| e.total_lines() == 18);
        assert_eq!(rows(&engine), vec![0, 1, 16, 17], "{label}");
        assert_eq!(groups(&engine), vec![(1, 15)], "{label}");
    }
}

#[test]
fn appends_far_past_the_last_match_read_only_the_new_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines = vec!["ERROR boom".to_string(); 3];
    lines.extend((0..2_000).map(|i| format!("INFO tick {i}")));
    let path = write(dir.path(), "far.log", &lines);
    // The lines after the last match weigh more than the job threshold: a rewind to
    // that match would take a background scan on every append.
    let mut engine = TailEngine::open_with_thresholds(&path, 8 * 1024, u64::MAX).unwrap();
    engine.size_check_interval = Duration::ZERO;
    engine.set_include_filter("ERROR");
    engine.set_collapse_mode(CollapseMode::Exact);
    wait_for_jobs(&mut engine);
    assert_eq!(groups(&engine), vec![(0, 3)]);
    let mut total = engine.total_lines();
    for round in 0..5 {
        let fed = engine.collapse_lines_fed().expect("detected");
        append(&path, "INFO tick more\nINFO tick more\n");
        let start = Instant::now();
        while engine.total_lines() < total + 2 {
            assert!(start.elapsed() < Duration::from_secs(10), "append not seen");
            engine.poll_updates();
            assert!(engine.scan_progress().is_none(), "round {round}: no scan");
            std::thread::sleep(Duration::from_millis(5));
        }
        total = engine.total_lines();
        let read = engine.collapse_lines_fed().unwrap() - fed;
        // The two new lines, and the last old one read again (it may have been
        // partial).
        assert!(read <= 3, "round {round}: {read} lines read");
        assert_eq!(groups(&engine), vec![(0, 3)], "round {round}");
    }
    // A match appended far below joins the run of the visible lines.
    append(&path, "ERROR boom\n");
    poll_until(&mut engine, "match", |e| e.total_lines() == total + 1);
    assert_eq!(groups(&engine), vec![(0, 4)]);
    assert_eq!(rows(&engine), vec![0]);
}

#[test]
fn a_rewrite_detects_again_and_expands_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", &retry_log());
    let mut engine = TailEngine::open(&path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    engine.set_collapse_mode(CollapseMode::Exact);
    assert!(engine.toggle_collapsed_row(10));
    let generation = engine.reload_generation;
    std::fs::write(&path, "x\nx\ny\n").unwrap();
    poll_until(&mut engine, "rewrite", |e| {
        e.reload_generation != generation
    });
    assert_eq!(rows(&engine), vec![0, 2]);
    let group = engine.collapsed_row(0).unwrap();
    assert_eq!((group.count, group.open), (2, false));
}

#[test]
fn time_delta_after_a_group_starts_from_its_last_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines: Vec<String> = (0..100)
        .map(|i| {
            format!(
                "2026-09-18T14:00:{:02}.{:03}Z WARN poll",
                i / 10,
                (i % 10) * 100
            )
        })
        .collect();
    lines.push("2026-09-18T14:00:10.400Z INFO next".to_string());
    let mut engine = TailEngine::open(write(dir.path(), "delta.log", &lines)).unwrap();
    engine.ensure_timestamps();
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(rows(&engine), vec![0, 100]);
    assert_eq!(engine.row_time_delta(0), TimeDelta::Blank);
    assert_eq!(engine.row_time_delta(1), TimeDelta::Millis(500));
}

#[test]
fn f3_lands_on_the_group_row_then_moves_past_its_hits() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines = vec!["INFO start".to_string()];
    lines.extend(std::iter::repeat_n(
        "WARN timeout talking to db".to_string(),
        200,
    ));
    lines.push("INFO between".to_string());
    lines.push("ERROR timeout again".to_string());
    let mut engine = TailEngine::open(write(dir.path(), "search.log", &lines)).unwrap();
    engine.set_collapse_mode(CollapseMode::Exact);
    engine.update_search("timeout");
    assert_eq!(engine.search_total(), 201, "every hit is counted");
    // From the top: the group row, then the later line.
    engine.current_match_idx = None;
    let first = engine.search_next(false).unwrap();
    assert_eq!(engine.get_visible_row_of_line(first), Some(1));
    assert_eq!(engine.search_next(false), Some(202));
    // Back: onto the group row again, at its first hit.
    assert_eq!(engine.search_prev(false), Some(1));
    assert_eq!(engine.row_marks(1, 1, true), (true, true, false));
    assert_eq!(engine.collapsed_row(1).map(|c| c.open), Some(false));

    // A result picked in the results pane is the exact line: its group opens.
    engine.select_match(150);
    assert_eq!(engine.collapsed_row(1).map(|c| c.open), Some(true));
    assert_eq!(engine.get_visible_row_of_line(151), Some(151));
}

fn group_log() -> Vec<String> {
    let mut lines: Vec<String> = (0..1000).map(|i| format!("INFO line {i}")).collect();
    lines.extend(std::iter::repeat_n("WARN same".to_string(), 500));
    lines.push("INFO end".to_string());
    lines
}

#[test]
fn jumps_to_a_hidden_line_expand_its_group() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "goto.log", &group_log())).unwrap();
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(engine.visible_line_count(), 1002);
    let target = engine.request_jump(1249, Language::En);
    assert_eq!(target.line, 1249);
    assert_eq!(engine.collapsed_row(1000).map(|c| c.open), Some(true));
    assert_eq!(engine.get_visible_row_of_line(1249), Some(1249));
    assert_eq!(engine.selected_lines(), vec![1249]);

    // Go to line: the viewer reveals the target the same way.
    assert!(engine.toggle_collapsed_row(1000));
    engine.reveal_line(1300);
    assert_eq!(engine.get_visible_row_of_line(1300), Some(1300));
}

#[test]
fn a_bookmark_inside_a_group_marks_its_row() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "marks.log", &group_log())).unwrap();
    engine.set_collapse_mode(CollapseMode::Exact);
    engine.toggle_bookmark(1300);
    assert_eq!(engine.row_marks(1000, 1000, false), (false, false, true));
    assert_eq!(engine.row_marks(999, 999, false), (false, false, false));
    // Bookmark navigation shows the exact line.
    assert_eq!(engine.bookmark_next(0), Some(1300));
    assert_eq!(engine.get_visible_row_of_line(1300), Some(1300));
}

#[test]
fn a_hidden_automatic_bookmark_marks_its_group_row() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines = vec!["INFO start".to_string()];
    lines.extend((1..=500).map(|i| format!("WARN retry {i}")));
    lines.push("INFO end".to_string());
    let mut engine = TailEngine::open(write(dir.path(), "auto.log", &lines)).unwrap();
    let mut rule =
        fasttail::tail_engine::HighlightRule::new("retry 250", [255, 255, 255], [0, 0, 0], false);
    rule.auto_bookmark = true;
    engine.set_highlight_rules(vec![rule]);
    wait_for_jobs(&mut engine);
    assert!(engine.is_auto_bookmark(250));
    engine.set_collapse_mode(CollapseMode::Numbers);
    assert_eq!(rows(&engine), vec![0, 1, 501]);
    assert_eq!(engine.hidden_bookmark(1), Some((250, false)));
    assert_eq!(engine.row_marks(1, 1, false), (false, false, true));
    // A manual bookmark wins over the automatic one for the row's mark.
    engine.toggle_bookmark(400);
    assert_eq!(engine.hidden_bookmark(1), Some((400, true)));
}

#[test]
fn bookmarks_the_filter_hides_leave_the_group_row_unmarked() {
    let dir = tempfile::tempdir().unwrap();
    // Every other line is a DEBUG line a rule bookmarks and the filter hides: the
    // group's file span holds far more bookmarks than visible lines.
    let mut lines = Vec::new();
    for _ in 0..2_000 {
        lines.push("WARN retry".to_string());
        lines.push("DEBUG probe".to_string());
    }
    let mut engine = TailEngine::open(write(dir.path(), "debug.log", &lines)).unwrap();
    let mut rule =
        fasttail::tail_engine::HighlightRule::new("DEBUG", [255, 255, 255], [0, 0, 0], false);
    rule.auto_bookmark = true;
    engine.set_highlight_rules(vec![rule]);
    wait_for_jobs(&mut engine);
    engine.set_exclude_filter("DEBUG");
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(groups(&engine), vec![(0, 2_000)]);
    assert_eq!(engine.hidden_bookmark(0), None);
    assert_eq!(engine.row_marks(0, 0, false), (false, false, false));
    // A visible line bookmarked by hand is found, and the cached answer follows.
    engine.toggle_bookmark(3_000);
    assert_eq!(engine.hidden_bookmark(0), Some((3_000, true)));
}

#[test]
fn show_in_context_shows_every_line_uncollapsed() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "ctx.log", &group_log())).unwrap();
    engine.set_include_filter("WARN");
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(rows(&engine), vec![1000]);
    assert!(engine.enter_context(1200));
    assert_eq!(
        engine.visible_line_count(),
        1501,
        "the full log, line by line"
    );
    assert_eq!(engine.collapsed_row(1000), None);
    assert_eq!(engine.get_visible_row_of_line(1200), Some(1200));
    assert_eq!(engine.selected_lines(), vec![1200]);
    engine.leave_context();
    assert_eq!(rows(&engine), vec![1000]);
    assert_eq!(groups(&engine), vec![(1000, 500)]);
}

#[test]
fn session_files_keep_the_mode_and_old_files_read_as_off() {
    let dir = tempfile::tempdir().unwrap();
    let log = write(dir.path(), "a.log", &strings(&["x"]));
    let file = dir.path().join("s.fasttail-session.ini");
    let mut entry = StreamEntry::new(log.clone());
    entry.collapse = Some("exact".to_string());
    let session = Session {
        streams: vec![entry.clone(), StreamEntry::new(log.clone())],
        dock_layout: None,
    };
    session.save_to(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("collapse=").count(), 1, "written only when on");
    let loaded = Session::load_from(&file).unwrap().session;
    assert_eq!(loaded.streams[0].collapse.as_deref(), Some("exact"));
    assert_eq!(loaded.streams[1].collapse, None);

    // An older file has no key; `off` and unknown values read as off too.
    for (value, expected) in [
        ("", None),
        ("collapse=off\n", None),
        ("collapse=signature\n", None),
        ("collapse=Numbers\n", Some("numbers")),
    ] {
        // Forward slashes: a backslash is an escape character in INI values.
        let text = format!(
            "[session]\nstreams=1\n[stream_0]\npath={}\n{value}",
            log.display().to_string().replace('\\', "/")
        );
        std::fs::write(&file, text).unwrap();
        let loaded = Session::load_from(&file).unwrap().session;
        assert_eq!(loaded.streams[0].collapse.as_deref(), expected, "{value:?}");
    }
}

fn frame(app: &mut FastTailApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

/// CTRL + SHIFT + D as the Windows integration reports it: CTRL is also the command
/// modifier there.
fn ctrl_shift_d() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::D,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    }
}

#[test]
fn ctrl_shift_d_cycles_the_mode_which_the_workspace_restores() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "a.log", &strings(&["x", "x", "y"]));
    let other = write(dir.path(), "b.log", &strings(&["z", "z"]));
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        open_files: vec![path.clone()],
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![ctrl_shift_d()]);
    assert_eq!(app.engines[0].collapse_mode(), CollapseMode::Exact);
    assert_eq!(app.engines[0].visible_line_count(), 2);
    frame(&mut app, &ctx, vec![ctrl_shift_d()]);
    assert_eq!(app.engines[0].collapse_mode(), CollapseMode::Numbers);
    let saved = app.config.stream_state_for(&path).cloned().unwrap();
    assert_eq!(saved.collapse.as_deref(), Some("numbers"));
    // The badge row is drawn in the wrapped layout too.
    app.engines[0].set_wrap_lines(true, 0);
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
    assert_eq!(app.engines[0].visible_line_count(), 2);
    app.engines[0].set_wrap_lines(false, 0);
    frame(&mut app, &ctx, vec![]);

    // Ignored while a text field has the keyboard.
    let search = egui::Id::new("log_search_input").with(&app.engines[0].path);
    ctx.memory_mut(|m| m.request_focus(search));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![ctrl_shift_d()]);
    assert_eq!(app.engines[0].collapse_mode(), CollapseMode::Numbers);

    // Restored with the workspace, per stream.
    let mut config = app.config.clone();
    config.open_files = vec![path.clone(), other.clone()];
    config.dock_layout = None;
    let restored = FastTailApp::from_config(config);
    let mode_of = |app: &FastTailApp, p: &PathBuf| {
        app.engines
            .iter()
            .find(|e| &e.path == p)
            .map(|e| e.collapse_mode())
    };
    assert_eq!(mode_of(&restored, &path), Some(CollapseMode::Numbers));
    assert_eq!(mode_of(&restored, &other), Some(CollapseMode::Off));
}
