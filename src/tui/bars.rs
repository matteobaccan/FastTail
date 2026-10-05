// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The GUI's two rows at the top of a stream, in a terminal window: the stream bar
//! (follow, monitor, view, line numbers, encoding, ANSI, collapse, context lines, the
//! time span and the line counts) and the filter row (include, exclude, `Aa`, `.*`,
//! level, presets, the global filter). Each chip is clickable and does what the GUI's
//! control does, or what its key does in the terminal.

use crate::ansi::AnsiMode;
use crate::collapse::CollapseMode;
use crate::i18n::Language;
use crate::i18n_tui::{tx, txf};
use crate::log_level::LogLevel;
use crate::tail_engine::FileEncoding;
use crate::tui::app::Tab;
use crate::tui::colors::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

/// What a chip does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarChip {
    Follow,
    Monitor,
    View,
    LineNumbers,
    Encoding,
    Ansi,
    Collapse,
    Context,
    Time,
    Include,
    Exclude,
    Case,
    Regex,
    Level,
    Presets,
    Global,
}

/// A piece of a bar: a clickable chip drawn `[text]`, or plain text.
pub struct Chip {
    pub text: String,
    pub style: Style,
    pub action: Option<BarChip>,
}

/// Inner rows a window gives to the bars: both from 8 rows (only the stream bar in
/// HEX, which has no filters), the stream bar alone from 5, none below.
pub fn rows_for(inner_height: u16, hex: bool) -> usize {
    let both = if hex { 1 } else { 2 };
    match inner_height {
        h if h >= 8 => both,
        h if h >= 5 => 1,
        _ => 0,
    }
}

/// The next encoding of the GUI's list.
pub fn next_encoding(e: FileEncoding) -> FileEncoding {
    match e {
        FileEncoding::Utf8 => FileEncoding::Ascii,
        FileEncoding::Ascii => FileEncoding::Ansi,
        FileEncoding::Ansi => FileEncoding::UnicodeLe,
        FileEncoding::UnicodeLe => FileEncoding::UnicodeBe,
        FileEncoding::UnicodeBe => FileEncoding::Utf8,
    }
}

/// The next number of context lines a click gives: 0, 1, 2, 3, 5, 10, back to 0.
pub fn next_context(n: u8) -> u8 {
    match n {
        0 => 1,
        1 => 2,
        2 => 3,
        3 | 4 => 5,
        5..=9 => 10,
        _ => 0,
    }
}

/// The two bars of `tab` (the second empty in HEX). `global`: the global filter applies.
pub fn lines(tab: &Tab, palette: &Palette, global: bool, lang: Language) -> Vec<Vec<Chip>> {
    let e = &tab.engine;
    let ascii = palette.ascii;
    let accent = Style::default().fg(palette.accent());
    let on = accent.add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(palette.dim());
    let warn = Style::default().fg(palette.level_color(LogLevel::Warn));
    let chip = |text: String, style: Style, action: BarChip| Chip {
        text,
        style,
        action: Some(action),
    };
    let text = |text: String, style: Style| Chip {
        text,
        style,
        action: None,
    };
    let (play, stop) = if ascii { (">", "#") } else { ("▶", "■") };
    let mark = |b: bool| if b { play } else { stop };
    let hex = tab.is_hex();

    let mut first = vec![
        chip(
            txf(lang, "{0} Follow", &[&mark(e.follow_tail)]),
            if e.follow_tail { on } else { dim },
            BarChip::Follow,
        ),
        chip(
            txf(lang, "{0} Monitor", &[&mark(e.is_watching)]),
            if e.is_watching { on } else { dim },
            BarChip::Monitor,
        ),
        chip(
            if tab.is_asm() {
                "ASM"
            } else if hex {
                "HEX"
            } else {
                "TXT"
            }
            .into(),
            on,
            BarChip::View,
        ),
    ];
    if !hex {
        first.push(chip(
            if e.show_line_numbers {
                "# 123"
            } else {
                "# ---"
            }
            .into(),
            if e.show_line_numbers { accent } else { dim },
            BarChip::LineNumbers,
        ));
        first.push(chip(
            e.encoding.name().into(),
            Style::default(),
            BarChip::Encoding,
        ));
        let ansi = match e.ansi_mode {
            AnsiMode::Auto => tx(lang, "ANSI auto").to_string(),
            mode => txf(lang, "ANSI {0}", &[&mode.name()]),
        };
        let ansi_style = if e.ansi_mode == AnsiMode::Auto {
            dim
        } else {
            accent
        };
        first.push(chip(ansi, ansi_style, BarChip::Ansi));
        let collapse = e.collapse_mode();
        let x = if ascii { "x" } else { "×" };
        first.push(chip(
            format!(
                "{x} {}",
                if collapse.is_on() {
                    collapse.name()
                } else {
                    "off"
                }
            ),
            if collapse == CollapseMode::Off {
                dim
            } else {
                accent
            },
            BarChip::Collapse,
        ));
        let n = e.context_lines();
        let pm = if ascii { "+-" } else { "±" };
        first.push(chip(
            format!("{pm} {n}"),
            if n > 0 { accent } else { dim },
            BarChip::Context,
        ));
        let clock = if ascii { tx(lang, "Time") } else { "🕘" };
        let window = e.time_window().is_some();
        let span = e.visible_time_span().map(|(a, b)| {
            format!(
                "{} -> {}",
                crate::timestamp::format_millis(a),
                crate::timestamp::format_millis(b)
            )
        });
        first.push(chip(
            format!("{clock} {}", span.unwrap_or_else(|| "-".into())),
            if window { accent } else { Style::default() },
            BarChip::Time,
        ));
    }
    let total = e.total_lines();
    let lines = if hex {
        txf(lang, "{0} bytes", &[&e.file_size])
    } else if e.visible_line_count() == total {
        txf(lang, "Lines: {0}", &[&total])
    } else {
        txf(lang, "Lines: {0} / {1}", &[&e.visible_line_count(), &total])
    };
    first.push(text(lines, dim));

    if hex {
        return vec![first];
    }
    let field = |value: &str| -> String {
        let shown: String = value.chars().take(18).collect();
        let pad = 18usize.saturating_sub(shown.chars().count());
        format!("{shown}{}", " ".repeat(pad))
    };
    let include = e.include_filter();
    let exclude = e.exclude_filter();
    let mut second = vec![
        text(tx(lang, "Include").into(), accent),
        chip(field(include), Style::default(), BarChip::Include),
        text(tx(lang, "Exclude").into(), warn),
        chip(field(exclude), Style::default(), BarChip::Exclude),
        chip(
            "Aa".into(),
            if e.filter_case_sensitive { on } else { dim },
            BarChip::Case,
        ),
        chip(
            ".*".into(),
            if e.filter_is_regex { on } else { dim },
            BarChip::Regex,
        ),
    ];
    let ge = if ascii { ">=" } else { "≥" };
    let level = match e.min_level {
        LogLevel::Unknown => tx(lang, "All levels").to_string(),
        l => format!("{ge} {}", l.name()),
    };
    let level_style = match e.min_level {
        LogLevel::Unknown => dim,
        l => Style::default().fg(palette.level_color(l)),
    };
    second.push(chip(level, level_style, BarChip::Level));
    second.push(chip(tx(lang, "Presets").into(), dim, BarChip::Presets));
    if global {
        let globe = if ascii {
            tx(lang, "Global")
        } else {
            tx(lang, "🌐 Global")
        };
        second.push(chip(globe.into(), on, BarChip::Global));
    }
    vec![first, second]
}

/// Draws `chips` on `row` from its left edge, a space between them, as many as fit.
/// Returns the clickable ones with where they were drawn.
pub fn draw(
    buf: &mut Buffer,
    row: Rect,
    chips: &[Chip],
    palette: &Palette,
) -> Vec<(Rect, BarChip)> {
    let width = unicode_width::UnicodeWidthStr::width;
    let bracket = Style::default().fg(palette.dim());
    let mut out = Vec::new();
    let mut x = row.x;
    for c in chips {
        let w = width(c.text.as_str()) as u16 + if c.action.is_some() { 2 } else { 0 };
        if x + w > row.right() {
            break;
        }
        match c.action {
            Some(action) => {
                buf.set_string(x, row.y, "[", bracket);
                buf.set_string(x + 1, row.y, &c.text, c.style);
                buf.set_string(x + w - 1, row.y, "]", bracket);
                out.push((Rect::new(x, row.y, w, 1), action));
            }
            None => {
                buf.set_string(x, row.y, &c.text, c.style);
            }
        }
        x += w + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bars_take_rows_only_when_the_window_has_room() {
        assert_eq!(rows_for(20, false), 2);
        assert_eq!(rows_for(20, true), 1);
        assert_eq!(rows_for(6, false), 1);
        assert_eq!(rows_for(4, false), 0);
        assert_eq!(next_context(0), 1);
        assert_eq!(next_context(10), 0);
        assert_eq!(next_encoding(FileEncoding::UnicodeBe), FileEncoding::Utf8);
    }
}
