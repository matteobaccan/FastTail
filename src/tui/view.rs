// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Pure layout of the log view: which rows are on screen, where the window moves to
//! reveal a row, and how a line is cut into plain, ANSI-coloured and search-hit segments.
//! No terminal
//! and no engine here, so all of it is testable.

use std::ops::Range;

/// Top row that shows the last `height` of `rows` rows (follow mode).
pub fn follow_top(rows: usize, height: usize) -> usize {
    rows.saturating_sub(height)
}

/// `top` kept inside the rows: never past the last full page.
pub fn clamp_top(top: usize, height: usize, rows: usize) -> usize {
    top.min(follow_top(rows, height))
}

/// Rows drawn for a window starting at `top`: only these are fetched from the engine.
pub fn visible_range(top: usize, height: usize, rows: usize) -> Range<usize> {
    let top = clamp_top(top, height, rows);
    top..(top + height).min(rows)
}

/// New top that brings `row` on screen: unchanged when it already is, else the row is
/// centred (a search jump lands in the middle of the view, with context around it).
pub fn reveal(top: usize, height: usize, rows: usize, row: usize) -> usize {
    if height == 0 {
        return top;
    }
    if row >= top && row < top + height {
        return clamp_top(top, height, rows);
    }
    clamp_top(row.saturating_sub(height / 2), height, rows)
}

/// A `w` x `h` box centred in an area of `area_w` x `area_h` (both clamped to the
/// area), as `(x, y, w, h)` offsets from the area's corner: where dialogs open.
pub fn centered(area_w: u16, area_h: u16, w: u16, h: u16) -> (u16, u16, u16, u16) {
    let w = w.min(area_w);
    let h = h.min(area_h);
    ((area_w - w) / 2, (area_h - h) / 2, w, h)
}

/// Columns of the line-number gutter for a file of `total_lines` lines.
pub fn gutter_width(total_lines: usize) -> usize {
    let mut n = total_lines.max(1);
    let mut digits = 0;
    while n > 0 {
        digits += 1;
        n /= 10;
    }
    digits.max(4)
}

/// Byte ranges of `query` in `line`, ASCII case-insensitive and non-overlapping. The
/// engine decides which lines are hits; this only paints the text inside them. Non-ASCII
/// bytes compare exactly, so the ranges always fall on character boundaries.
pub fn hit_ranges(line: &str, query: &str) -> Vec<Range<usize>> {
    let q = query.trim().as_bytes();
    let hay = line.as_bytes();
    let mut out = Vec::new();
    if q.is_empty() || q.len() > hay.len() {
        return out;
    }
    let mut i = 0;
    while i + q.len() <= hay.len() {
        if hay[i..i + q.len()].eq_ignore_ascii_case(q) {
            out.push(i..i + q.len());
            i += q.len();
        } else {
            i += 1;
        }
        // Cap the work on a pathological line.
        if out.len() >= 256 {
            break;
        }
    }
    out
}

/// What paints a segment of a line: nothing, ANSI style run `i`, or a search hit (which
/// wins over the run under it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Plain,
    Run(usize),
    Hit,
}

/// `line` cut into consecutive segments by the search `hits` and the ANSI style `runs`
/// (both sorted and non-overlapping byte ranges). Neighbours painted alike are merged.
pub fn segments<'a>(
    line: &'a str,
    hits: &[Range<usize>],
    runs: &[Range<usize>],
) -> Vec<(&'a str, Paint)> {
    let len = line.len();
    let mut cuts = Vec::with_capacity(2 * (hits.len() + runs.len()) + 2);
    cuts.extend([0, len]);
    for r in hits.iter().chain(runs) {
        cuts.extend([r.start.min(len), r.end.min(len)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut parts: Vec<(usize, usize, Paint)> = Vec::with_capacity(cuts.len());
    let (mut h, mut r) = (0, 0);
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        while hits.get(h).is_some_and(|x| x.end <= a) {
            h += 1;
        }
        while runs.get(r).is_some_and(|x| x.end <= a) {
            r += 1;
        }
        let paint = if hits.get(h).is_some_and(|x| x.start <= a) {
            Paint::Hit
        } else if runs.get(r).is_some_and(|x| x.start <= a) {
            Paint::Run(r)
        } else {
            Paint::Plain
        };
        match parts.last_mut() {
            Some(last) if last.2 == paint => last.1 = b,
            _ => parts.push((a, b, paint)),
        }
    }
    if parts.is_empty() {
        return vec![(line, Paint::Plain)];
    }
    parts
        .into_iter()
        // A range off a character boundary (never from the engine) is dropped rather
        // than cut through a character.
        .filter_map(|(a, b, p)| line.get(a..b).map(|t| (t, p)))
        .collect()
}

/// Text safe to hand to the terminal: tabs expanded to four spaces, `ESC` shown as `^[`
/// (raw ANSI mode) and other control characters (CR, BEL) as `·`, so no sequence from a
/// log line is ever interpreted by the terminal.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("    "),
            '\x1b' => out.push_str("^["),
            c if c.is_control() => out.push('\u{b7}'),
            c => out.push(c),
        }
    }
    out
}

/// Drops the first `skip` terminal cells of the segment list (horizontal scroll). Wide
/// characters (CJK, most emoji) take two cells; one cut in half becomes a space, so the
/// columns after it stay where they are.
pub fn skip_cells<P>(segments: Vec<(String, P)>, mut skip: usize) -> Vec<(String, P)> {
    use unicode_width::UnicodeWidthChar;
    if skip == 0 {
        return segments;
    }
    let mut out = Vec::with_capacity(segments.len());
    for (text, paint) in segments {
        if skip == 0 {
            out.push((text, paint));
            continue;
        }
        let mut cut = text.len();
        let mut pad = false;
        for (byte, c) in text.char_indices() {
            if skip == 0 {
                cut = byte;
                break;
            }
            let w = c.width().unwrap_or(0);
            if w > skip {
                // A wide character straddling the edge: its right half shows as a blank.
                cut = byte + c.len_utf8();
                pad = true;
                skip = 0;
                break;
            }
            skip -= w;
        }
        if cut < text.len() || pad {
            let rest = &text[cut..];
            out.push((
                if pad {
                    format!(" {rest}")
                } else {
                    rest.to_string()
                },
                paint,
            ));
            skip = 0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_follows_the_bottom() {
        assert_eq!(follow_top(1000, 40), 960);
        assert_eq!(follow_top(10, 40), 0);
        assert_eq!(visible_range(960, 40, 1000), 960..1000);
        // A top past the end is pulled back to the last page.
        assert_eq!(visible_range(5000, 40, 1000), 960..1000);
        // A file shorter than the screen shows every row.
        assert_eq!(visible_range(0, 40, 7), 0..7);
        assert_eq!(visible_range(3, 40, 0), 0..0);
    }

    #[test]
    fn reveal_centres_only_off_screen_rows() {
        // Already visible: the window does not move.
        assert_eq!(reveal(100, 40, 1000, 120), 100);
        // Below the window: centred.
        assert_eq!(reveal(100, 40, 1000, 500), 480);
        // Near the end: clamped to the last page.
        assert_eq!(reveal(0, 40, 1000, 995), 960);
        // Near the start: clamped to row 0.
        assert_eq!(reveal(500, 40, 1000, 3), 0);
    }

    #[test]
    fn dialogs_are_centred_and_clamped() {
        assert_eq!(centered(100, 30, 60, 3), (20, 13, 60, 3));
        // Wider than the screen: the dialog takes the whole width.
        assert_eq!(centered(40, 10, 60, 3), (0, 3, 40, 3));
        assert_eq!(centered(0, 0, 60, 3), (0, 0, 0, 0));
    }

    #[test]
    fn gutter_grows_with_the_line_count() {
        assert_eq!(gutter_width(0), 4);
        assert_eq!(gutter_width(9_999), 4);
        assert_eq!(gutter_width(10_000), 5);
        assert_eq!(gutter_width(12_345_678), 8);
    }

    #[test]
    fn hits_are_case_insensitive_and_segmented() {
        let line = "Error: disk error on ERROR path";
        let hits = hit_ranges(line, "error");
        assert_eq!(hits, vec![0..5, 12..17, 21..26]);
        let segs = segments(line, &hits, &[]);
        assert_eq!(segs[0], ("Error", Paint::Hit));
        assert_eq!(segs[1], (": disk ", Paint::Plain));
        assert_eq!(segs.last().unwrap(), &(" path", Paint::Plain));
        assert!(hit_ranges(line, "  ").is_empty());
        assert_eq!(segments("abc", &[], &[]), vec![("abc", Paint::Plain)]);
        assert_eq!(segments("", &[], &[]), vec![("", Paint::Plain)]);
    }

    #[test]
    fn hits_respect_utf8_boundaries() {
        let line = "città ERR città";
        let hits = hit_ranges(line, "err");
        assert_eq!(segments(line, &hits, &[])[1], ("ERR", Paint::Hit));
    }

    #[test]
    fn a_hit_wins_over_the_ansi_run_under_it() {
        // "red" is a run over 4..12, "err" a hit over 6..9 inside it.
        let (hit, run) = (6..9, 4..12);
        let segs = segments(
            "the red error",
            std::slice::from_ref(&hit),
            std::slice::from_ref(&run),
        );
        assert_eq!(
            segs,
            vec![
                ("the ", Paint::Plain),
                ("re", Paint::Run(0)),
                ("d e", Paint::Hit),
                ("rro", Paint::Run(0)),
                ("r", Paint::Plain),
            ]
        );
        // Two runs side by side stay apart: they carry different styles.
        let segs = segments("abcd", &[], &[0..2, 2..4]);
        assert_eq!(segs, vec![("ab", Paint::Run(0)), ("cd", Paint::Run(1))]);
    }

    #[test]
    fn sanitize_and_horizontal_scroll() {
        assert_eq!(sanitize("a\tb\x1b[0m\r"), "a    b^[[0m\u{b7}");
        let segs = vec![("hello ".to_string(), false), ("world".to_string(), true)];
        let cut = skip_cells(segs.clone(), 8);
        assert_eq!(cut, vec![("rld".to_string(), true)]);
        assert_eq!(skip_cells(segs.clone(), 0), segs);
        assert!(skip_cells(segs, 50).is_empty());
    }

    #[test]
    fn sideways_scroll_counts_terminal_cells() {
        let seg = |s: &str| vec![(s.to_string(), false)];
        let text = |v: Vec<(String, bool)>| v.into_iter().map(|(s, _)| s).collect::<String>();
        assert_eq!(text(skip_cells(seg("abcdef"), 2)), "cdef");
        // "日本" is four cells: skipping two drops 日, skipping one halves it.
        assert_eq!(text(skip_cells(seg("日本x"), 2)), "本x");
        assert_eq!(text(skip_cells(seg("日本x"), 1)), " 本x");
        assert_eq!(text(skip_cells(seg("ab"), 9)), "");
        let two = vec![("ab".to_string(), false), ("cd".to_string(), true)];
        assert_eq!(skip_cells(two, 3), vec![("d".to_string(), true)]);
    }
}
