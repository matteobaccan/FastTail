// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Pure layout of the log view: which rows are on screen, where the window moves to
//! reveal a row, and how a line is cut into plain and search-hit segments. No terminal
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

/// `line` cut into consecutive segments, each flagged when it is a search hit.
pub fn segments<'a>(line: &'a str, hits: &[Range<usize>]) -> Vec<(&'a str, bool)> {
    let mut out = Vec::with_capacity(hits.len() * 2 + 1);
    let mut pos = 0;
    for h in hits {
        if h.start > pos {
            out.push((&line[pos..h.start], false));
        }
        out.push((&line[h.start..h.end], true));
        pos = h.end;
    }
    if pos < line.len() || out.is_empty() {
        out.push((&line[pos..], false));
    }
    out
}

/// Text safe to hand to the terminal: tabs expanded to four spaces and other control
/// characters (a stray ESC, CR, BEL) shown as `·` instead of being interpreted.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("    "),
            c if c.is_control() => out.push('\u{b7}'),
            c => out.push(c),
        }
    }
    out
}

/// Drops the first `skip` characters of the segment list (horizontal scroll).
pub fn skip_chars(segments: Vec<(String, bool)>, mut skip: usize) -> Vec<(String, bool)> {
    if skip == 0 {
        return segments;
    }
    let mut out = Vec::with_capacity(segments.len());
    for (text, hit) in segments {
        if skip == 0 {
            out.push((text, hit));
            continue;
        }
        let n = text.chars().count();
        if n <= skip {
            skip -= n;
            continue;
        }
        let byte = text.char_indices().nth(skip).map_or(text.len(), |(b, _)| b);
        out.push((text[byte..].to_string(), hit));
        skip = 0;
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
        let segs = segments(line, &hits);
        assert_eq!(segs[0], ("Error", true));
        assert_eq!(segs[1], (": disk ", false));
        assert_eq!(segs.last().unwrap(), &(" path", false));
        assert!(hit_ranges(line, "  ").is_empty());
        assert_eq!(segments("abc", &[]), vec![("abc", false)]);
    }

    #[test]
    fn hits_respect_utf8_boundaries() {
        let line = "città ERR città";
        let hits = hit_ranges(line, "err");
        assert_eq!(segments(line, &hits)[1], ("ERR", true));
    }

    #[test]
    fn sanitize_and_horizontal_scroll() {
        assert_eq!(sanitize("a\tb\x1b[0m\r"), "a    b\u{b7}[0m\u{b7}");
        let segs = vec![("hello ".to_string(), false), ("world".to_string(), true)];
        let cut = skip_chars(segs.clone(), 8);
        assert_eq!(cut, vec![("rld".to_string(), true)]);
        assert_eq!(skip_chars(segs.clone(), 0), segs);
        assert!(skip_chars(segs, 50).is_empty());
    }
}
