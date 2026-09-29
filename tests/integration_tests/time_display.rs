// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::tail_engine::TailEngine;
use fasttail::timestamp::{SourceZone, TimeDisplay};
use std::io::Write;

fn engine(lines: &[&str]) -> (tempfile::NamedTempFile, TailEngine) {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    for l in lines {
        writeln!(tmp, "{l}").unwrap();
    }
    tmp.flush().unwrap();
    let e = TailEngine::open(tmp.path()).unwrap();
    (tmp, e)
}

fn visible(e: &TailEngine) -> Vec<usize> {
    (0..e.visible_line_count())
        .filter_map(|r| e.get_actual_line_idx(r))
        .collect()
}

const LOG: &[&str] = &[
    "2026-09-28T14:01:00.000Z INFO a",
    "2026-09-28T14:02:05.123Z ERROR b",
    "2026-09-28T14:03:30.000Z INFO c",
    "2026-09-28T14:05:00.000Z INFO d",
];

#[test]
fn as_written_by_default() {
    let (_tmp, e) = engine(LOG);
    assert_eq!(e.time_display(), TimeDisplay::Written);
    assert_eq!(e.time_source_zone(), SourceZone::Local);
    assert_eq!(e.display_time(LOG[1]), None);
}

#[test]
fn rows_show_the_display_zone_and_the_text_stays_as_written() {
    let (_tmp, mut e) = engine(LOG);
    e.set_time_display(TimeDisplay::Offset(120));
    assert!(e.view_columns_dirty);
    let (start, end, text) = e.display_time(LOG[1]).unwrap();
    assert_eq!(&LOG[1][start..end], "2026-09-28T14:02:05.123Z");
    assert_eq!(text, "2026-09-28 16:02:05.123+02:00");
    // Search, copy and export see the text as written.
    e.update_search("14:02:05");
    assert_eq!(e.search_matches, vec![1]);
    assert_eq!(e.get_line(1).as_deref(), Some(LOG[1]));
    e.select_row(1);
    assert_eq!(e.copy_selection_text().as_deref(), Some(LOG[1]));
}

#[test]
fn time_range_and_go_to_time_in_the_display_zone() {
    let (_tmp, mut e) = engine(LOG);
    e.set_time_display(TimeDisplay::Offset(120));
    e.ensure_timestamps();
    let (from_ok, to_ok) = e.apply_time_range_text("16:02", "16:03");
    assert!(from_ok && to_ok);
    // Through 16:03:59.999 on the display clock: 14:02:05Z and 14:03:30Z.
    assert_eq!(visible(&e), vec![1, 2]);
    e.apply_time_range_text("16:02", "16:02");
    assert_eq!(visible(&e), vec![1]);
    // The span of what is shown, on the display clock.
    let (a, b) = e.visible_time_span().unwrap();
    assert_eq!(
        fasttail::timestamp::format_millis(e.to_display_clock(a)),
        "2026-09-28 16:02:05"
    );
    assert_eq!(e.from_display_clock(e.to_display_clock(b)), b);
    e.clear_time_range();
    let target = e.resolve_goto("16:03", 0).unwrap();
    assert_eq!(target.line, 2);
    // A window set from typed text is read again on a new clock: 16:02 in UTC is
    // not in this log.
    e.apply_time_range_text("16:02", "16:02");
    assert_eq!(visible(&e), vec![1]);
    e.set_time_display(TimeDisplay::Utc);
    assert!(visible(&e).is_empty());
    e.set_time_display(TimeDisplay::Offset(120));
    assert_eq!(visible(&e), vec![1]);
    e.set_time_source_zone(SourceZone::Offset(60));
    assert_eq!(visible(&e), vec![1], "the log states its zone: Z wins");
    e.clear_time_range();
    // Changing the display does not re-time the stream: the cache keeps the clock
    // the log printed.
    let before = e.line_timestamp(2);
    e.set_time_display(TimeDisplay::Utc);
    assert_eq!(e.line_timestamp(2), before);
    assert_eq!(e.resolve_goto("14:03", 0).unwrap().line, 2);
}

#[test]
fn source_zone_for_lines_without_one() {
    let (_tmp, mut e) = engine(&[
        "2026-09-28 14:02:05 INFO plain",
        "2026-09-28 14:04:00 INFO x",
    ]);
    e.set_time_source_zone(SourceZone::Offset(-300));
    e.set_time_display(TimeDisplay::Utc);
    let (_, _, text) = e.display_time("2026-09-28 14:02:05 INFO plain").unwrap();
    assert_eq!(text, "2026-09-28 19:02:05Z");
    e.ensure_timestamps();
    e.apply_time_range_text("19:02", "19:02");
    assert_eq!(visible(&e), vec![0]);
}

#[test]
fn a_leading_count_in_an_iso_log_is_not_a_date() {
    let (_tmp, mut e) = engine(&[LOG[0], LOG[1], "1234567890 rows copied"]);
    e.set_time_display(TimeDisplay::Utc);
    assert_eq!(e.display_time("1234567890 rows copied"), None);
    assert!(e.display_time(LOG[1]).is_some());
}

#[test]
fn epoch_milliseconds_as_a_date() {
    let (_tmp, mut e) = engine(&["1790604125123 GET /x"]);
    e.set_time_display(TimeDisplay::Utc);
    let (start, end, text) = e.display_time("1790604125123 GET /x").unwrap();
    assert_eq!((start, end), (0, 13));
    assert_eq!(text, "2026-09-28 14:02:05.123Z");
}

#[test]
fn spans_follow_the_substitution() {
    use fasttail::tail_engine::{replace_with_spans, HighlightSpan, SpanHighlight, SpanStyle};
    let text = "2026-09-28T14:02:05Z ERROR payment";
    let style = SpanStyle::Label(1);
    let spans = SpanHighlight {
        spans: vec![
            HighlightSpan {
                start: 0,
                end: 4,
                style,
            },
            HighlightSpan {
                start: 15,
                end: 27,
                style,
            },
            HighlightSpan {
                start: 27,
                end: 34,
                style,
            },
        ],
        rest: None,
    };
    let with = "2026-09-28 16:02:05";
    let (out, moved) = replace_with_spans(text, Some(spans), 0, 20, with);
    assert_eq!(out, "2026-09-28 16:02:05 ERROR payment");
    let moved: Vec<(usize, usize)> = moved
        .unwrap()
        .spans
        .iter()
        .map(|s| (s.start, s.end))
        .collect();
    // Inside the timestamp: dropped; across its end: the part after it; after it:
    // shifted by the change of length (-1).
    assert_eq!(moved, vec![(19, 26), (26, 33)]);
    assert_eq!(&out[19..26], " ERROR ");
}
