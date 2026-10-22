// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::config::FastTailConfig;
use fasttail::tail_engine::TailEngine;
use fasttail::ui::FastTailApp;

const LOG: [&str; 7] = [
    "2026-09-18 14:00:00 INFO payment start",
    "2026-09-18 14:02:10 ERROR payment timeout",
    "    at Gateway.call(Gateway.java:12)",
    "2026-09-18 14:03:00 WARN refund timeout retry worker",
    "2026-09-18 14:05:30 ERROR refund refused",
    "2026-09-18 14:06:00 INFO circuit open",
    "2026-09-18 14:07:00 INFO done",
];

fn visible(e: &TailEngine) -> Vec<usize> {
    (0..e.visible_line_count())
        .filter_map(|r| e.get_actual_line_idx(r))
        .collect()
}

fn open(path: &std::path::Path, thresholds: Option<u64>) -> TailEngine {
    let mut e = match thresholds {
        None => TailEngine::open(path).unwrap(),
        Some(t) => TailEngine::open_with_thresholds(path, t, t).unwrap(),
    };
    wait_for_jobs(&mut e);
    e
}

#[test]
fn an_expression_filters_alike_on_both_paths_with_traces_and_exclude_terms() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    for thresholds in [None, Some(0)] {
        let mut e = open(&log, thresholds);
        e.set_filter_expr("((payment OR refund) AND timeout) OR \"circuit open\"");
        assert!(!e.is_filter_active(), "term mode: the expression waits");
        e.set_filter_expr_mode(true);
        wait_for_jobs(&mut e);
        assert!(e.is_filter_active());
        assert_eq!(visible(&e), vec![1, 2, 3, 5], "{thresholds:?}");
        // An exclude term still hides lines, and the trace follows its parent.
        e.set_exclude_filter("retry");
        wait_for_jobs(&mut e);
        assert_eq!(visible(&e), vec![1, 2, 5], "{thresholds:?}");
        e.set_exclude_filter("Gateway");
        wait_for_jobs(&mut e);
        assert_eq!(visible(&e), vec![1, 3, 5], "an excluded trace line goes");
        e.set_exclude_filter("");

        // A broken expression keeps the last valid one filtering.
        e.set_filter_expr("(payment OR");
        wait_for_jobs(&mut e);
        assert_eq!(
            e.expr_error().map(|x| x.key),
            Some("expr_err_missing_operand")
        );
        assert_eq!(visible(&e), vec![1, 2, 3, 5], "{thresholds:?}");
        e.set_filter_expr("NOT INFO");
        wait_for_jobs(&mut e);
        assert_eq!(e.expr_error(), None);
        assert_eq!(visible(&e), vec![1, 2, 3, 4], "{thresholds:?}");
    }
}

#[test]
fn switching_modes_keeps_both_sides() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    let mut e = open(&log, None);
    e.set_filter_terms(vec!["payment".into(), "time out".into()], Vec::new());
    e.set_filter_expr_mode(true);
    assert_eq!(
        e.filter_expr(),
        "payment AND \"time out\"",
        "built from the terms"
    );
    e.set_filter_expr("payment OR refund");
    e.set_filter_expr_mode(false);
    assert_eq!(e.include_terms()[0], "payment", "the terms come back");
    assert_eq!(visible(&e), Vec::<usize>::new(), "payment AND \"time out\"");
    e.set_filter_expr_mode(true);
    assert_eq!(e.filter_expr(), "payment OR refund", "kept");
    assert_eq!(visible(&e), vec![0, 1, 2, 3, 4]);
}

#[test]
fn the_mode_and_the_expression_are_saved_and_frozen_into_filter_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    let config = || FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config());
    app.open_log_file(log.clone());
    app.engines[0].set_filter_expr("refund AND NOT refused");
    app.engines[0].set_filter_expr_mode(true);
    fasttail::workspace::save_changes(&mut app.engines[0], &mut app.config, false);
    app.save_dock_layout();
    let entry = app.config.stream_state_for(&log).unwrap();
    assert!(entry.filter_expr_mode);
    assert_eq!(entry.filter_expr, "refund AND NOT refused");

    let again = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..FastTailConfig::from_ini(&app.config.to_ini())
    });
    let e = again.engines.iter().find(|e| e.path == log).unwrap();
    assert!(e.filter_expr_mode());
    assert_eq!(visible(e), vec![3]);

    // "Open filter as new tab" freezes the expression.
    let frozen = fasttail::filter_tab::FrozenFilter::of(&app.engines[0]);
    assert_eq!(frozen.expr.as_deref(), Some("refund AND NOT refused"));
    assert!(frozen.include.is_empty());
    let pairs = frozen.to_ini_pairs();
    let back = fasttail::filter_tab::FrozenFilter::from_ini(|k| {
        pairs
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    })
    .unwrap();
    assert_eq!(back.expr, frozen.expr);
    assert_eq!(back.label(), "refund AND NOT refused");
}
