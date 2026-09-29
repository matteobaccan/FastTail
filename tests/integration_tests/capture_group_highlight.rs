// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::tail_engine::{HighlightRule, QuickLabel, SpanStyle, TailEngine, MAX_ROW_SPANS};
use std::io::Write;
use tempfile::NamedTempFile;

fn engine() -> TailEngine {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-18 req=1234 sess-8f3a payment ok").unwrap();
    tmp.flush().unwrap();
    TailEngine::open(tmp.path()).unwrap()
}

fn ranges(spans: &[fasttail::tail_engine::HighlightSpan]) -> Vec<(usize, usize)> {
    spans.iter().map(|s| (s.start, s.end)).collect()
}

#[test]
fn captures_only_paints_the_captured_group() {
    let mut e = engine();
    e.set_highlight_rules(vec![HighlightRule::captures(
        r"req=(\d+)",
        [0, 255, 255],
        [0, 0, 0],
    )]);
    assert!(e.has_span_rules());
    let line = "2026-09-18 req=1234 sess-8f3a req=99 payment ok";
    let hl = e.match_highlight_spans(line);
    assert_eq!(ranges(&hl.spans), vec![(15, 19), (34, 36)]);
    assert_eq!(&line[15..19], "1234");
    match hl.spans[0].style {
        SpanStyle::Rule(style) => assert_eq!(style.fg, egui::Color32::from_rgb(0, 255, 255)),
        other => panic!("unexpected style {other:?}"),
    }
    assert!(
        hl.rest.is_none(),
        "captures-only rules never colour the whole row"
    );
    // the whole-row path ignores captures-only rules
    assert!(e.match_highlight(line).is_none());
}

#[test]
fn captures_only_without_groups_paints_the_whole_match() {
    let mut e = engine();
    e.set_highlight_rules(vec![HighlightRule::captures(
        r"sess-[0-9a-f]+",
        [1, 2, 3],
        [0, 0, 0],
    )]);
    let hl = e.match_highlight_spans("a sess-8f3a b SESS-00 c");
    assert_eq!(ranges(&hl.spans), vec![(2, 11), (14, 21)]);
}

#[test]
fn whole_row_rules_keep_colouring_the_row_and_the_fast_path_is_used() {
    let mut e = engine();
    e.set_highlight_rules(vec![HighlightRule::new(
        "payment",
        [0, 255, 0],
        [0, 0, 0],
        false,
    )]);
    assert!(
        !e.has_span_rules(),
        "no span rule or label: the renderer keeps match_highlight"
    );
    let hl = e.match_highlight_spans("req=1 payment ok");
    assert!(hl.spans.is_empty());
    assert_eq!(
        hl.rest.map(|s| s.fg),
        Some(egui::Color32::from_rgb(0, 255, 0))
    );
}

#[test]
fn first_rule_wins_per_byte() {
    let mut e = engine();
    // 1: captures rule claims the digits; 2: whole-row rule colours the rest
    e.set_highlight_rules(vec![
        HighlightRule::captures(r"req=(\d+)", [9, 9, 9], [0, 0, 0]),
        HighlightRule::new("payment", [0, 255, 0], [0, 0, 0], false),
    ]);
    let hl = e.match_highlight_spans("req=1234 payment");
    assert_eq!(ranges(&hl.spans), vec![(4, 8)]);
    assert_eq!(
        hl.rest.map(|s| s.fg),
        Some(egui::Color32::from_rgb(0, 255, 0))
    );

    // whole-row rule first: it claims every byte, later captures paint nothing
    e.set_highlight_rules(vec![
        HighlightRule::new("payment", [0, 255, 0], [0, 0, 0], false),
        HighlightRule::captures(r"req=(\d+)", [9, 9, 9], [0, 0, 0]),
    ]);
    let hl = e.match_highlight_spans("req=1234 payment");
    assert!(hl.spans.is_empty());
    assert!(hl.rest.is_some());

    // two captures rules overlapping: the first keeps its bytes, the second gets the rest
    e.set_highlight_rules(vec![
        HighlightRule::captures(r"(1234)", [1, 1, 1], [0, 0, 0]),
        HighlightRule::captures(r"req=(\d+) pay", [2, 2, 2], [0, 0, 0]),
    ]);
    let hl = e.match_highlight_spans("req=01234 payment");
    assert_eq!(ranges(&hl.spans), vec![(4, 5), (5, 9)]);
    assert!(
        matches!(hl.spans[0].style, SpanStyle::Rule(s) if s.fg == egui::Color32::from_rgb(2, 2, 2))
    );
    assert!(
        matches!(hl.spans[1].style, SpanStyle::Rule(s) if s.fg == egui::Color32::from_rgb(1, 1, 1))
    );
}

#[test]
fn spans_are_capped_per_row() {
    let mut e = engine();
    e.set_highlight_rules(vec![HighlightRule::captures(r"(x)", [1, 1, 1], [0, 0, 0])]);
    let line = "x ".repeat(200);
    let hl = e.match_highlight_spans(&line);
    assert_eq!(hl.spans.len(), MAX_ROW_SPANS);
    let mut labels = Vec::new();
    QuickLabel::toggle(&mut labels, "x", 3);
    e.set_highlight_rules(Vec::new());
    e.set_quick_labels(&labels);
    assert_eq!(e.match_highlight_spans(&line).spans.len(), MAX_ROW_SPANS);
}

#[test]
fn quick_labels_toggle_and_rank_below_rules() {
    let mut labels = Vec::new();
    assert!(
        !QuickLabel::toggle(&mut labels, "   ", 2),
        "blank text is ignored"
    );
    assert!(QuickLabel::toggle(&mut labels, "sess-8f3a", 2));
    assert_eq!(labels.len(), 1);
    // same text, other colour: recoloured, not duplicated
    assert!(QuickLabel::toggle(&mut labels, "SESS-8F3A", 5));
    assert_eq!(
        labels,
        vec![QuickLabel {
            text: "sess-8f3a".into(),
            color: 5
        }]
    );
    // same text and colour: removed
    assert!(QuickLabel::toggle(&mut labels, "sess-8f3a", 5));
    assert!(labels.is_empty());

    QuickLabel::toggle(&mut labels, "Sess-8f3a", 2);
    let mut e = engine();
    e.set_quick_labels(&labels);
    assert!(e.has_span_rules());
    assert_eq!(e.quick_labels(), labels);
    let hl = e.match_highlight_spans("x SESS-8f3a y sess-8f3a");
    assert_eq!(ranges(&hl.spans), vec![(2, 11), (14, 23)]);
    assert!(matches!(hl.spans[0].style, SpanStyle::Label(2)));

    // a whole-row rule matching the row wins over the label
    e.set_highlight_rules(vec![HighlightRule::new(
        "sess",
        [7, 7, 7],
        [0, 0, 0],
        false,
    )]);
    let hl = e.match_highlight_spans("x SESS-8f3a y");
    assert!(hl.spans.is_empty());
    assert_eq!(
        hl.rest.map(|s| s.fg),
        Some(egui::Color32::from_rgb(7, 7, 7))
    );
    // a captures rule claims its bytes first, the label gets the rest
    e.set_highlight_rules(vec![HighlightRule::captures(
        r"(8f3a)",
        [7, 7, 7],
        [0, 0, 0],
    )]);
    let hl = e.match_highlight_spans("x SESS-8f3a y");
    assert_eq!(ranges(&hl.spans), vec![(2, 7), (7, 11)]);
    assert!(matches!(hl.spans[0].style, SpanStyle::Label(2)));
    assert!(matches!(hl.spans[1].style, SpanStyle::Rule(_)));

    // non-ASCII label text: byte offsets map back to the original characters
    let mut labels = Vec::new();
    QuickLabel::toggle(&mut labels, "ÜBER", 1);
    e.set_highlight_rules(Vec::new());
    e.set_quick_labels(&labels);
    let line = "über ÜBER x";
    let hl = e.match_highlight_spans(line);
    assert_eq!(ranges(&hl.spans), vec![(0, 5), (6, 11)]);
    assert_eq!(&line[6..11], "ÜBER");
}

#[test]
fn captures_only_round_trips_through_the_ini() {
    let mut config = fasttail::config::FastTailConfig::default();
    config.highlight_rules = vec![
        HighlightRule::captures(r"req=(\d+)", [0, 255, 255], [0, 0, 0]),
        HighlightRule::new("ERROR", [255, 0, 0], [0, 0, 0], false),
    ];
    let loaded = fasttail::config::FastTailConfig::from_ini(&config.to_ini());
    assert_eq!(loaded.highlight_rules.len(), 2);
    assert!(loaded.highlight_rules[0].captures_only);
    assert!(loaded.highlight_rules[0].is_regex);
    assert!(!loaded.highlight_rules[1].captures_only);
}
