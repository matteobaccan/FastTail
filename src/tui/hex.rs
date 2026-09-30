// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The HEX view of a stream: the row layout (offset, bytes, ASCII), the bytes per row
//! the window width allows, and the byte hits of the search painted over a row.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Bytes per row the view picks from: the largest whose row fits the window.
pub const ROW_BYTES: [usize; 6] = [8, 16, 24, 32, 48, 64];

/// Hex digits of the offset column: 8, or more for a file past 4 GiB.
pub fn offset_digits(file_size: u64) -> usize {
    let last = file_size.saturating_sub(1);
    let digits = (64 - last.leading_zeros() as usize).div_ceil(4);
    digits.max(8)
}

/// Cells of a row of `n` bytes: offset, two spaces, the bytes as `XX` separated by one
/// space and one more every 8 bytes, two spaces, then the ASCII between `|`.
pub fn row_width(n: usize, digits: usize) -> usize {
    digits + 2 + (3 * n - 1) + (n / 8 - 1) + 2 + n + 2
}

/// Bytes per row for a window `width` cells wide (8 when not even that fits).
pub fn bytes_per_row(width: usize, digits: usize) -> usize {
    ROW_BYTES
        .iter()
        .rev()
        .copied()
        .find(|&n| row_width(n, digits) <= width)
        .unwrap_or(ROW_BYTES[0])
}

/// Parses a byte offset typed in the go-to dialog: decimal, or hexadecimal after `0x`.
pub fn parse_offset(text: &str) -> Option<usize> {
    let s = text.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => usize::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// How a byte is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Hit,
    Current,
}

/// The styles of a row: plain bytes, the offset, a hit, the current hit.
pub struct Styles {
    pub text: Style,
    pub offset: Style,
    pub hit: Style,
    pub current: Style,
}

/// One HEX row: `bytes` start at file offset `offset`, `n` is the row width in bytes
/// (a short last row is padded). `hits` are the sorted byte hits `(offset, len)`,
/// `current` the current one.
pub fn render_row(
    offset: usize,
    bytes: &[u8],
    n: usize,
    digits: usize,
    hits: &[(usize, usize)],
    current: Option<(usize, usize)>,
    styles: &Styles,
) -> Line<'static> {
    // The first hit that ends past the row start; the ones after it are walked in step.
    let mut next = hits.partition_point(|&(off, len)| off + len <= offset);
    let marks: Vec<Mark> = (0..bytes.len())
        .map(|i| {
            let at = offset + i;
            if current.is_some_and(|(off, len)| at >= off && at < off + len) {
                return Mark::Current;
            }
            while hits.get(next).is_some_and(|&(off, len)| off + len <= at) {
                next += 1;
            }
            match hits.get(next) {
                Some(&(off, _)) if off <= at => Mark::Hit,
                _ => Mark::None,
            }
        })
        .collect();
    let style_of = |m: Mark| match m {
        Mark::None => styles.text,
        Mark::Hit => styles.hit,
        Mark::Current => styles.current,
    };
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(8);
    spans.push(Span::styled(format!("{offset:0digits$X}  "), styles.offset));
    for i in 0..n {
        if i > 0 {
            // A space between two bytes of the same hit takes the hit's colour.
            let joined = i < bytes.len() && marks[i] != Mark::None && marks[i] == marks[i - 1];
            let gap = if i % 8 == 0 { "  " } else { " " };
            push(
                &mut spans,
                gap,
                if joined {
                    style_of(marks[i])
                } else {
                    styles.text
                },
            );
        }
        match bytes.get(i) {
            Some(b) => push(&mut spans, &format!("{b:02X}"), style_of(marks[i])),
            None => push(&mut spans, "  ", styles.text),
        }
    }
    push(&mut spans, "  |", styles.text);
    for (&b, &mark) in bytes.iter().zip(&marks) {
        let c = if (0x20..=0x7E).contains(&b) {
            b as char
        } else {
            '.'
        };
        push(&mut spans, c.encode_utf8(&mut [0; 4]), style_of(mark));
    }
    // A short last row keeps the closing `|` in its column.
    let pad = " ".repeat(n.saturating_sub(bytes.len()));
    push(&mut spans, &format!("{pad}|"), styles.text);
    Line::from(spans)
}

/// Appends `text`, growing the last span when it has the same style.
fn push(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if let Some(last) = spans.last_mut().filter(|s| s.style == style) {
        last.content.to_mut().push_str(text);
    } else {
        spans.push(Span::styled(text.to_string(), style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    fn styles() -> Styles {
        Styles {
            text: Style::default(),
            offset: Style::default().fg(Color::DarkGray),
            hit: Style::default().bg(Color::Yellow),
            current: Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn the_width_picks_the_bytes_per_row() {
        assert_eq!(row_width(16, 8), 78, "16 bytes fill an 80-column window");
        assert_eq!(bytes_per_row(78, 8), 16);
        assert_eq!(bytes_per_row(77, 8), 8);
        assert_eq!(bytes_per_row(10, 8), 8, "never fewer than 8");
        assert_eq!(bytes_per_row(row_width(64, 8), 8), 64);
        assert_eq!(bytes_per_row(row_width(48, 8) + 5, 8), 48);
        assert_eq!(offset_digits(0), 8);
        assert_eq!(offset_digits(1 << 32), 8);
        assert_eq!(offset_digits((1 << 32) + 1), 9);
    }

    #[test]
    fn offsets_are_decimal_or_0x() {
        assert_eq!(parse_offset("256"), Some(256));
        assert_eq!(parse_offset(" 0x100 "), Some(256));
        assert_eq!(parse_offset("0XfF"), Some(255));
        assert_eq!(parse_offset("zz"), None);
    }

    #[test]
    fn a_row_shows_offset_bytes_and_ascii_padded() {
        let bytes = b"Hello,\x00world\n";
        let line = render_row(0x20, bytes, 16, 8, &[], None, &styles());
        assert_eq!(
            text(&line),
            "00000020  48 65 6C 6C 6F 2C 00 77  6F 72 6C 64 0A           |Hello,.world.   |"
        );
        assert_eq!(text(&line).chars().count(), row_width(16, 8));
    }

    #[test]
    fn hits_are_painted_in_both_columns_and_the_current_one_differs() {
        let s = styles();
        // "ll" at 2..4 is a hit, "wo" at 7..9 the current one.
        let line = render_row(
            0,
            b"Hello,\x00world",
            16,
            8,
            &[(2, 2), (7, 2)],
            Some((7, 2)),
            &s,
        );
        let styled = |style: Style| -> String {
            line.spans
                .iter()
                .filter(|sp| sp.style == style)
                .map(|sp| sp.content.as_ref())
                .collect::<Vec<_>>()
                .join("|")
        };
        assert_eq!(styled(s.hit), "6C 6C|ll");
        assert_eq!(styled(s.current), "77  6F|wo");
    }

    #[test]
    fn a_hit_started_on_the_previous_row_carries_on() {
        let s = styles();
        let line = render_row(8, b"abcdefgh", 8, 8, &[(6, 4)], None, &s);
        let hit: Vec<&str> = line
            .spans
            .iter()
            .filter(|sp| sp.style == s.hit)
            .map(|sp| sp.content.as_ref())
            .collect();
        assert_eq!(hit, ["61 62", "ab"]);
    }
}
