//! `grep -C N` in the viewer: every filter match with the `N` file lines before and
//! after it, the context taken from the file whatever the filters say.

use super::wait_for_jobs;
use fasttail::collapse::CollapseMode;
use fasttail::config::FastTailConfig;
use fasttail::log_level::LogLevel;
use fasttail::scan_job::{FilterSpec, ScanKind};
use fasttail::tail_engine::{TailEngine, TimeDelta};
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn write(dir: &Path, name: &str, lines: &[String]) -> PathBuf {
    let path = dir.join(name);
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(&path, text).unwrap();
    path
}

/// `total` lines: a match on each line of `matches`, DEBUG lines elsewhere.
fn log(total: usize, matches: &[usize]) -> Vec<String> {
    (0..total)
        .map(|i| {
            if matches.contains(&i) {
                format!("ERROR payment failed {i}")
            } else {
                format!("DEBUG step {i}")
            }
        })
        .collect()
}

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

fn rows(engine: &TailEngine) -> Vec<usize> {
    (0..engine.visible_line_count())
        .filter_map(|r| engine.get_actual_line_idx(r))
        .collect()
}

/// What `grep -C n` prints: every line within `n` of a match, once, in order.
fn grep_c(n: usize, matches: &[usize], total: usize) -> Vec<usize> {
    (0..total)
        .filter(|&l| matches.iter().any(|&m| l + n >= m && l <= m + n))
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

/// The same file on the synchronous paths and on the background jobs (thresholds 0).
fn both(path: &Path) -> [TailEngine; 2] {
    let sync = TailEngine::open(path).unwrap();
    let mut background = TailEngine::open_with_thresholds(path, 0, 0).unwrap();
    wait_for_jobs(&mut background);
    [sync, background]
}

#[test]
fn rows_are_what_grep_c_prints() {
    let dir = tempfile::tempdir().unwrap();
    let total = 200;
    let matches = [0, 3, 50, 51, 60, 120, 199];
    let mut engine = TailEngine::open(write(dir.path(), "app.log", &log(total, &matches))).unwrap();
    engine.set_include_filter("payment failed");
    assert_eq!(rows(&engine), matches);
    for n in [1u8, 2, 3, 10, 100] {
        engine.set_context_lines(n);
        let expected = grep_c(n as usize, &matches, total);
        assert_eq!(rows(&engine), expected, "N = {n}");
        for (row, &line) in expected.iter().enumerate() {
            assert_eq!(engine.is_context_row(line), !matches.contains(&line));
            let gap = row
                .checked_sub(1)
                .map(|p| line - expected[p] - 1)
                .filter(|&g| g > 0);
            assert_eq!(engine.context_gap_above_row(row), gap, "N = {n}, row {row}");
        }
    }
    // Off again: the plain filtered view, no separator, no dimming.
    engine.set_context_lines(0);
    assert_eq!(rows(&engine), matches);
    assert!(!engine.is_context_row(1));
    assert_eq!(engine.context_gap_above_row(3), None);
    // Capped at 100.
    engine.set_context_lines(250);
    assert_eq!(engine.context_lines(), 100);
}

#[test]
fn three_lines_of_context_and_the_hidden_count() {
    // The spec's example: matches on lines 1,000 and 5,000 (1-based).
    let dir = tempfile::tempdir().unwrap();
    let mut engine =
        TailEngine::open(write(dir.path(), "app.log", &log(6000, &[999, 4999]))).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(3);
    let expected: Vec<usize> = (996..=1002).chain(4996..=5002).collect();
    assert_eq!(rows(&engine), expected);
    assert!(!engine.is_context_row(999) && !engine.is_context_row(4999));
    assert!(engine.is_context_row(996) && engine.is_context_row(5002));
    assert_eq!(engine.context_gap_above_row(7), Some(3993));
    assert_eq!(engine.context_gap_above_row(6), None);
    assert!(engine.shows_context_lines());
}

#[test]
fn overlapping_context_is_shown_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine =
        TailEngine::open(write(dir.path(), "app.log", &log(300, &[100, 104]))).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(3);
    assert_eq!(rows(&engine), (97..=107).collect::<Vec<_>>());
    assert!((1..11).all(|r| engine.context_gap_above_row(r).is_none()));
    assert!(!engine.is_context_row(104));
}

#[test]
fn context_ignores_the_level_filter() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "app.log",
        &strings(&[
            "INFO start",
            "DEBUG connecting",
            "DEBUG retrying",
            "ERROR connection lost",
            "INFO recovered",
            "INFO idle",
            "INFO idle again",
        ]),
    );
    let mut engine = TailEngine::open(path).unwrap();
    engine.set_min_level(LogLevel::Error);
    assert_eq!(rows(&engine), vec![3]);
    engine.set_context_lines(2);
    assert_eq!(rows(&engine), vec![1, 2, 3, 4, 5]);
    assert!(engine.is_context_row(1) && engine.is_context_row(2));
    // Without any filter the setting has no effect.
    engine.set_min_level(LogLevel::Unknown);
    assert_eq!(rows(&engine), (0..7).collect::<Vec<_>>());
    assert!(!engine.shows_context_lines());
    assert!(!engine.is_context_row(1));
}

#[test]
fn global_filter_matches_get_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "app.log",
        &strings(&["a", "b req-7f3a", "c", "d", "e", "f req-7f3a", "g"]),
    );
    let mut engine = TailEngine::open(path).unwrap();
    engine.set_context_lines(1);
    engine.set_global_filter(Some(Arc::new(FilterSpec::build(
        &["req-7f3a"],
        &[""],
        false,
        false,
    ))));
    assert_eq!(rows(&engine), vec![0, 1, 2, 4, 5, 6]);
    assert_eq!(engine.context_gap_above_row(3), Some(1));
}

#[test]
fn background_jobs_show_what_the_synchronous_paths_show() {
    let dir = tempfile::tempdir().unwrap();
    let total = 3000;
    let matches: Vec<usize> = (0..total).filter(|i| i % 97 == 5 || i % 331 == 0).collect();
    let path = write(dir.path(), "app.log", &log(total, &matches));
    let [mut sync, mut background] = both(&path);
    for engine in [&mut sync, &mut background] {
        // N set before the filter: the ranges grow with the filter batches.
        engine.set_context_lines(2);
        engine.set_include_filter("payment failed");
        engine.search_query = "step 4".into();
        engine.update_search("step 4");
        wait_for_jobs(engine);
    }
    let expected = grep_c(2, &matches, total);
    assert_eq!(rows(&sync), expected);
    assert_eq!(rows(&background), expected);
    // The search covers the rows shown, context rows included.
    let hits: Vec<usize> = expected
        .iter()
        .copied()
        .filter(|&l| !matches.contains(&l) && format!("DEBUG step {l}").contains("step 4"))
        .collect();
    assert!(!hits.is_empty());
    assert_eq!(sync.search_matches, hits);
    assert_eq!(background.search_matches, hits);
    assert_eq!(sync.search_total(), background.search_total());

    // Changing N refreshes the search, never the filter.
    for engine in [&mut sync, &mut background] {
        let generation = engine.filter_generation;
        engine.set_context_lines(5);
        assert_ne!(
            engine.scan_progress().map(|p| p.0),
            Some(ScanKind::Filter),
            "no filter scan for a change of N"
        );
        assert_ne!(engine.filter_generation, generation, "rows rebuilt");
        wait_for_jobs(engine);
    }
    let expected = grep_c(5, &matches, total);
    assert_eq!(rows(&sync), expected);
    assert_eq!(rows(&background), expected);
    assert_eq!(sync.search_matches, background.search_matches);
    assert!(sync.search_matches.len() > hits.len());

    // Collapse forms its runs over the rows shown, the same on both paths.
    for engine in [&mut sync, &mut background] {
        engine.set_collapse_mode(CollapseMode::Numbers);
        wait_for_jobs(engine);
    }
    assert_eq!(rows(&sync), rows(&background));
    assert!(sync.visible_line_count() < expected.len());
}

#[test]
fn a_large_rebuild_runs_on_a_worker_and_keeps_the_old_rows_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let matches = [10, 200, 480];
    let path = write(dir.path(), "app.log", &log(500, &matches));
    let mut engine = TailEngine::open(&path).unwrap();
    engine.context_rebuild_threshold = 0;
    engine.set_include_filter("payment failed");
    engine.set_context_lines(2);
    // The frame never waits: the previous rows stay until the worker delivers.
    assert_eq!(rows(&engine), matches);
    assert_eq!(engine.context_lines(), 2);
    poll_until(&mut engine, "the rebuilt ranges", |e| {
        rows(e) == grep_c(2, &matches, 500)
    });
    // A match found while the worker runs is added on arrival.
    engine.set_context_lines(4);
    append(&path, "DEBUG x\nERROR payment failed tail\n");
    poll_until(&mut engine, "the appended match", |e| {
        rows(e) == grep_c(4, &[10, 200, 480, 501], 502)
    });
}

#[test]
fn lines_appended_after_the_last_match_show_as_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", &log(20, &[5, 19]));
    let mut engine = TailEngine::open(&path).unwrap();
    engine.follow_tail = true;
    engine.set_include_filter("payment failed");
    engine.set_context_lines(3);
    assert_eq!(rows(&engine), vec![2, 3, 4, 5, 6, 7, 8, 16, 17, 18, 19]);
    let generation = engine.filter_generation;
    append(&path, "DEBUG after one\nDEBUG after two\n");
    poll_until(&mut engine, "the appended lines", |e| e.total_lines() == 22);
    assert!(engine.filter_generation != generation);
    assert_eq!(rows(&engine)[11..], [20, 21]);
    assert!(engine.is_context_row(20) && engine.is_context_row(21));
    assert!(engine.follow_tail);

    // A new match brings the lines before it; lines past N after it stay hidden.
    append(
        &path,
        "DEBUG a\nDEBUG b\nDEBUG c\nDEBUG d\nDEBUG e\nERROR payment failed late\nDEBUG z\n",
    );
    poll_until(&mut engine, "the new match", |e| e.total_lines() == 29);
    let expected = grep_c(3, &[5, 19, 27], 29);
    assert_eq!(rows(&engine), expected);
    assert_eq!(
        engine.context_gap_above_row(expected.iter().position(|&l| l == 24).unwrap()),
        Some(1)
    );
}

#[test]
fn show_in_context_bypasses_the_ranges_and_returns_to_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", &log(100, &[20, 70]))).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(3);
    let with_context = rows(&engine);
    assert!(engine.enter_context(70));
    assert_eq!(engine.visible_line_count(), 100);
    assert!(!engine.shows_context_lines());
    assert!(!engine.is_context_row(10));
    assert_eq!(engine.context_gap_above_row(50), None);
    engine.leave_context();
    assert_eq!(rows(&engine), with_context);
    assert!(engine.shows_context_lines());
}

#[test]
fn collapse_runs_cover_context_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "app.log",
        &strings(&[
            "INFO boot",
            "WARN retrying",
            "WARN retrying",
            "WARN retrying",
            "ERROR payment failed",
            "INFO done",
            "INFO idle",
        ]),
    );
    let mut engine = TailEngine::open(path).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(3);
    assert_eq!(rows(&engine), vec![1, 2, 3, 4, 5, 6]);
    engine.set_collapse_mode(CollapseMode::Exact);
    assert_eq!(rows(&engine), vec![1, 4, 5, 6]);
    let group = engine.collapsed_row(0).expect("group row");
    assert_eq!((group.count, group.first_line, group.last_line), (3, 1, 3));
    // Export writes every line shown, those the group hides included.
    assert_eq!(
        exported(&engine),
        "WARN retrying\nWARN retrying\nWARN retrying\nERROR payment failed\nINFO done\nINFO idle\n"
    );
}

#[test]
fn time_delta_measures_context_rows_like_any_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "app.log",
        &strings(&[
            "2024-05-01 10:00:00.000 INFO start",
            "2024-05-01 10:00:01.000 DEBUG sending",
            "2024-05-01 10:00:01.250 ERROR payment failed",
            "2024-05-01 10:00:03.250 INFO retry",
            "2024-05-01 10:00:09.000 INFO idle",
        ]),
    );
    let mut engine = TailEngine::open(path).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(1);
    engine.want_timestamps();
    wait_for_jobs(&mut engine);
    assert_eq!(rows(&engine), vec![1, 2, 3]);
    assert_eq!(engine.row_time_delta(1), TimeDelta::Millis(250));
    assert_eq!(engine.row_time_delta(2), TimeDelta::Millis(2000));
    engine.select_all_visible();
    assert_eq!(engine.selection_elapsed(), Some((2250, 3)));
}

#[test]
fn export_and_copy_write_the_lines_shown() {
    let dir = tempfile::tempdir().unwrap();
    // The filter keeps lines 10 and 50 (1-based).
    let lines: Vec<String> = (1..=60).map(|i| format!("line {i}")).collect();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", &lines)).unwrap();
    // Keep exactly "line 10" and "line 50": exclude the other multiples of ten.
    engine.set_filter_terms(
        vec!["0".into()],
        vec![
            "line 20".into(),
            "line 30".into(),
            "line 40".into(),
            "line 60".into(),
        ],
    );
    assert_eq!(rows(&engine), vec![9, 49]);
    engine.set_context_lines(1);
    assert_eq!(
        exported(&engine),
        "line 9\nline 10\nline 11\nline 49\nline 50\nline 51\n"
    );
    // Across the separator: two lines, one newline.
    engine.select_row(10);
    engine.extend_selection_to(48);
    assert_eq!(
        engine.copy_selection_text().as_deref(),
        Some("line 11\nline 49")
    );
    engine.select_all_visible();
    assert_eq!(engine.selected_lines(), vec![8, 9, 10, 48, 49, 50]);
    assert!(engine.is_selected(48) && !engine.is_selected(30));
}

#[test]
fn search_hits_on_context_rows_and_go_to_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut lines: Vec<String> = (0..600).map(|i| format!("INFO tick {i}")).collect();
    lines[99] = "WARN retry the call".into();
    lines[100] = "ERROR payment failed".into();
    lines[400] = "ERROR payment failed again".into();
    let mut engine = TailEngine::open(write(dir.path(), "app.log", &lines)).unwrap();
    engine.set_include_filter("ERROR");
    engine.search_query = "retry".into();
    engine.update_search("retry");
    assert!(engine.search_matches.is_empty(), "hidden without context");
    engine.set_context_lines(2);
    assert_eq!(engine.search_matches, vec![99]);
    engine.current_match_idx = None;
    assert_eq!(engine.search_next(false), Some(99));
    assert_eq!(engine.get_visible_row_of_line(99), Some(1));
    let (hit, _, _) = engine.row_marks(1, 99, true);
    assert!(hit);

    // A line far from any match goes to the next row shown.
    let target = engine.goto_target_for(250, engine.total_lines());
    assert_eq!((target.line, target.hidden), (398, true));
    let target = engine.goto_target_for(101, engine.total_lines());
    assert_eq!(
        (target.line, target.hidden),
        (101, false),
        "a context row is visible"
    );
    assert_eq!(engine.row_of_line_or_next(250), 5);
}

#[test]
fn truncation_rebuilds_the_context_from_the_new_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "app.log", &log(50, &[10, 40]));
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(3);
    assert_eq!(rows(&engine).len(), 14);
    std::fs::write(&path, "fresh a\nERROR payment failed new\nfresh b\n").unwrap();
    poll_until(&mut engine, "the rewrite", |e| e.total_lines() == 3);
    assert_eq!(rows(&engine), vec![0, 1, 2]);
    assert_eq!(engine.get_line(0).as_deref(), Some("fresh a"));
    assert_eq!(engine.context_lines(), 3);
}

fn frame(app: &mut FastTailApp, ctx: &egui::Context) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 900.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

#[test]
fn restored_with_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "a.log", &log(40, &[10, 30]));
    let other = write(dir.path(), "b.log", &log(10, &[3]));
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        open_files: vec![path.clone(), other.clone()],
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx);
    let engine = app.engines.iter_mut().find(|e| e.path == path).unwrap();
    engine.set_include_filter("payment failed");
    engine.set_context_lines(4);
    // Drawn in both layouts, separators and dimmed rows included.
    frame(&mut app, &ctx);
    let engine = app.engines.iter_mut().find(|e| e.path == path).unwrap();
    engine.set_wrap_lines(true, 0);
    frame(&mut app, &ctx);
    frame(&mut app, &ctx);
    let saved = app.config.stream_state_for(&path).cloned().unwrap();
    assert_eq!(saved.context_lines, 4);
    let mut buf = Vec::new();
    app.config.to_ini().write_to(&mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("context_lines=4"), "{text}");
    assert_eq!(text.matches("context_lines=").count(), 1, "{text}");

    let mut config = FastTailConfig::from_ini(&app.config.to_ini());
    config.spool_dir = Some(dir.path().to_path_buf());
    config.dock_layout = None;
    let restored = FastTailApp::from_config(config);
    let n_of = |p: &PathBuf| {
        restored
            .engines
            .iter()
            .find(|e| &e.path == p)
            .map(|e| e.context_lines())
    };
    assert_eq!(n_of(&path), Some(4));
    assert_eq!(n_of(&other), Some(0));
    let engine = restored.engines.iter().find(|e| e.path == path).unwrap();
    assert_eq!(rows(engine), grep_c(4, &[10, 30], 40));
}
