//! ANSI escape sequences in log lines: detection, removal and SGR styling.
//!
//! Only the framing of the sequences is parsed (CSI `ESC [ … final`, the string
//! sequences OSC `ESC ] …`, DCS, SOS, PM and APC terminated by BEL or `ESC \`, and a
//! lone `ESC` followed by one character) and only SGR (`ESC [ … m`) is interpreted; every
//! other sequence is removed without being interpreted. There is no terminal emulation:
//! cursor movement, clearing and `\r` overwrites stay what they are in the file.
//!
//! A line without an `ESC` byte (`0x1B`) is found with one `memchr` and returned
//! untouched, so plain logs pay nothing for any of this.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The escape byte every sequence starts with.
pub const ESC: u8 = 0x1B;
/// Glyph drawn in place of `ESC` in raw mode (U+241B SYMBOL FOR ESCAPE).
pub const ESC_GLYPH: char = '\u{241B}';
/// Bytes of the file (at open) and of each append examined by the auto-detection.
pub const DETECT_SAMPLE_BYTES: usize = 64 * 1024;

/// How a stream treats escape sequences (per stream, persisted as `ansi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AnsiMode {
    /// Render once an SGR sequence has been seen, raw until then.
    #[default]
    Auto,
    /// Sequences hidden, SGR attributes painted.
    Render,
    /// Sequences hidden, nothing painted.
    Strip,
    /// Line shown as stored, `ESC` drawn as `␛`.
    Raw,
}

impl AnsiMode {
    pub const ALL: [AnsiMode; 4] = [
        AnsiMode::Auto,
        AnsiMode::Render,
        AnsiMode::Strip,
        AnsiMode::Raw,
    ];

    /// Name used in the workspace and session files.
    pub fn name(self) -> &'static str {
        match self {
            AnsiMode::Auto => "auto",
            AnsiMode::Render => "render",
            AnsiMode::Strip => "strip",
            AnsiMode::Raw => "raw",
        }
    }

    /// The mode written as `name()` (case-insensitive).
    pub fn from_name(name: &str) -> Option<AnsiMode> {
        Self::ALL
            .iter()
            .copied()
            .find(|m| m.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Whether the text features see the line with its sequences removed. `Auto` is
    /// meant to be resolved first (see `TailEngine::ansi_effective`).
    pub fn strips(self) -> bool {
        matches!(self, AnsiMode::Render | AnsiMode::Strip)
    }
}

/// A colour of an SGR attribute: an index of the 256-colour table (0..16 being the
/// theme's palette) or a 24-bit colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnsiColor {
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// SGR state of a run of text. `Default` is the plain style (no run is emitted for it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AnsiStyle {
    pub fg: Option<AnsiColor>,
    pub bg: Option<AnsiColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

/// Bytes `[start, end)` of the stripped text painted with `style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleRun {
    pub start: usize,
    pub end: usize,
    pub style: AnsiStyle,
}

/// True when `bytes` holds an escape byte at all (the fast path of every function here).
#[inline]
pub fn has_escape(bytes: &[u8]) -> bool {
    memchr::memchr(ESC, bytes).is_some()
}

/// One piece of a line: kept text `[start, end)`, the parameter bytes `[start, end)` of
/// an SGR sequence, or any other sequence (removed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Text(usize, usize),
    Sgr(usize, usize),
    Other,
}

/// Splits a line into text and escape sequences. Sequences are ASCII, so every text
/// range lies on UTF-8 character boundaries; the only non-ASCII byte a sequence can
/// swallow is the character after a lone `ESC`, which is removed whole.
struct Tokens<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Tokens<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            b: text.as_bytes(),
            pos: 0,
        }
    }

    /// The sequence starting at `self.pos` (an `ESC`).
    fn escape(&mut self) -> Token {
        let b = self.b;
        let len = b.len();
        let j = self.pos + 1;
        if j >= len {
            self.pos = len;
            return Token::Other;
        }
        match b[j] {
            b'[' => {
                // CSI: parameter bytes 0x30-0x3F, intermediate bytes 0x20-0x2F, one final
                // byte 0x40-0x7E. Anything else ends a malformed sequence and is kept.
                let mut k = j + 1;
                let mut intermediates = false;
                while k < len {
                    let c = b[k];
                    match c {
                        0x40..=0x7E => {
                            self.pos = k + 1;
                            let params = &b[j + 1..k];
                            let is_sgr = c == b'm'
                                && !intermediates
                                && params
                                    .iter()
                                    .all(|&p| p.is_ascii_digit() || p == b';' || p == b':');
                            return if is_sgr {
                                Token::Sgr(j + 1, k)
                            } else {
                                Token::Other
                            };
                        }
                        0x30..=0x3F if !intermediates => k += 1,
                        0x20..=0x2F => {
                            intermediates = true;
                            k += 1;
                        }
                        _ => {
                            self.pos = k;
                            return Token::Other;
                        }
                    }
                }
                // Unterminated (cut at the end of the line): dropped to the end.
                self.pos = len;
                Token::Other
            }
            // OSC, DCS, SOS, PM, APC: a string ended by BEL or ST (`ESC \`).
            b']' | b'P' | b'X' | b'^' | b'_' => {
                match memchr::memchr2(0x07, ESC, &b[j + 1..]) {
                    Some(rel) => {
                        let at = j + 1 + rel;
                        // BEL and ST end the string; any other ESC aborts it and starts
                        // the next sequence, as in a terminal.
                        self.pos = if b[at] == 0x07 {
                            at + 1
                        } else if b.get(at + 1) == Some(&b'\\') {
                            at + 2
                        } else {
                            at
                        };
                    }
                    // Unterminated: dropped to the end of the line.
                    None => self.pos = len,
                }
                Token::Other
            }
            // ESC ESC: the second one starts the next sequence.
            ESC => {
                self.pos = j;
                Token::Other
            }
            // A lone ESC and the character after it (a two-byte escape such as `ESC 7`).
            lead => {
                let width = match lead {
                    0xF0..=0xFF => 4,
                    0xE0..=0xEF => 3,
                    0xC0..=0xDF => 2,
                    _ => 1,
                };
                self.pos = (j + width).min(len);
                Token::Other
            }
        }
    }
}

impl Iterator for Tokens<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        let len = self.b.len();
        if self.pos >= len {
            return None;
        }
        match memchr::memchr(ESC, &self.b[self.pos..]) {
            None => {
                let start = self.pos;
                self.pos = len;
                Some(Token::Text(start, len))
            }
            Some(0) => Some(self.escape()),
            Some(rel) => {
                let start = self.pos;
                self.pos += rel;
                Some(Token::Text(start, start + rel))
            }
        }
    }
}

/// `line` without its escape sequences; borrowed when it holds none.
pub fn strip(line: &str) -> Cow<'_, str> {
    if !has_escape(line.as_bytes()) {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    for token in Tokens::new(line) {
        if let Token::Text(s, e) = token {
            out.push_str(&line[s..e]);
        }
    }
    Cow::Owned(out)
}

/// Like `strip`, for an owned line: returned as it is when it holds no sequence.
pub fn strip_owned(line: String) -> String {
    match strip(&line) {
        Cow::Borrowed(_) => line,
        Cow::Owned(s) => s,
    }
}

/// A stripped line and where its text came from: one `(stripped_offset, raw_offset)`
/// pair per kept segment (a run of bytes between two sequences), in increasing order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stripped {
    pub text: String,
    pub map: Vec<(usize, usize)>,
}

impl Stripped {
    /// Offset in the unstripped line of the byte at `offset` in the stripped text. An
    /// offset at the end of a segment maps to the end of that segment (not past the
    /// sequence that follows it), so a range end maps to the end of its last byte.
    pub fn to_raw(&self, offset: usize) -> usize {
        let at = self.map.partition_point(|&(s, _)| s <= offset);
        match at.checked_sub(1) {
            Some(i) => {
                let (s, r) = self.map[i];
                r + (offset - s)
            }
            None => offset,
        }
    }
}

/// `strip` plus the segment map, for callers that convert offsets back to the file.
pub fn strip_with_map(line: &str) -> Stripped {
    if !has_escape(line.as_bytes()) {
        return Stripped {
            text: line.to_owned(),
            map: vec![(0, 0)],
        };
    }
    let mut out = Stripped {
        text: String::with_capacity(line.len()),
        map: Vec::new(),
    };
    for token in Tokens::new(line) {
        if let Token::Text(s, e) = token {
            out.map.push((out.text.len(), s));
            out.text.push_str(&line[s..e]);
        }
    }
    out
}

/// Style runs of `line`, in stripped-text offsets (see `strip_and_style`).
pub fn style_runs(line: &str) -> Vec<StyleRun> {
    strip_and_style(line).1
}

/// The stripped line and its SGR style runs in one pass: sorted, non-overlapping,
/// adjacent runs of the same style merged, the plain style left out.
pub fn strip_and_style(line: &str) -> (String, Vec<StyleRun>) {
    if !has_escape(line.as_bytes()) {
        return (line.to_owned(), Vec::new());
    }
    let bytes = line.as_bytes();
    let mut text = String::with_capacity(line.len());
    let mut runs: Vec<StyleRun> = Vec::new();
    let mut style = AnsiStyle::default();
    for token in Tokens::new(line) {
        match token {
            Token::Text(s, e) => {
                let start = text.len();
                text.push_str(&line[s..e]);
                if style == AnsiStyle::default() {
                    continue;
                }
                match runs.last_mut() {
                    Some(last) if last.end == start && last.style == style => {
                        last.end = text.len();
                    }
                    _ => runs.push(StyleRun {
                        start,
                        end: text.len(),
                        style,
                    }),
                }
            }
            Token::Sgr(s, e) => apply_sgr(&mut style, &bytes[s..e]),
            Token::Other => {}
        }
    }
    (text, runs)
}

/// True when `bytes` holds an SGR sequence (`ESC [ <digits ; :> m`), the auto-detection
/// test. Other sequences (cursor movement, titles) do not turn a stream to render mode.
pub fn contains_sgr(bytes: &[u8]) -> bool {
    let mut from = 0;
    while let Some(rel) = memchr::memchr(ESC, &bytes[from..]) {
        let at = from + rel;
        if bytes.get(at + 1) == Some(&b'[') {
            let params = &bytes[at + 2..];
            let n = params
                .iter()
                .take_while(|&&p| p.is_ascii_digit() || p == b';' || p == b':')
                .count();
            if params.get(n) == Some(&b'm') {
                return true;
            }
        }
        from = at + 1;
    }
    false
}

/// Applies the parameters of one SGR sequence (`1;31`, `38;5;208`, `38:2::10:20:30`).
fn apply_sgr(style: &mut AnsiStyle, params: &[u8]) {
    if params.is_empty() {
        *style = AnsiStyle::default();
        return;
    }
    // `;` separates parameters; `:` separates the sub-parameters of one of them.
    // Stack-allocate parameter groups for zero heap allocations in hot paths.
    let mut stack_groups = [&[] as &[u8]; 16];
    let heap_groups;
    let groups: &[&[u8]] = if params.iter().filter(|&&b| b == b';').count() < 16 {
        let mut len = 0;
        for group in params.split(|&b| b == b';') {
            stack_groups[len] = group;
            len += 1;
        }
        &stack_groups[..len]
    } else {
        heap_groups = params.split(|&b| b == b';').collect::<Vec<_>>();
        &heap_groups
    };

    let mut i = 0;
    while i < groups.len() {
        let group = groups[i];
        i += 1;
        if memchr::memchr(b':', group).is_some() {
            let mut stack_sub = [0u32; 16];
            let heap_sub;
            let sub: &[u32] = if group.iter().filter(|&&b| b == b':').count() < 16 {
                let mut len = 0;
                for part in group.split(|&b| b == b':') {
                    stack_sub[len] = number(part);
                    len += 1;
                }
                &stack_sub[..len]
            } else {
                heap_sub = group.split(|&b| b == b':').map(number).collect::<Vec<_>>();
                &heap_sub
            };

            if let Some(&sub0) = sub.first() {
                match sub0 {
                    38 | 48 | 58 => {
                        let color = match sub.get(1) {
                            Some(5) => sub.get(2).map(|&n| AnsiColor::Indexed(n.min(255) as u8)),
                            // `38:2:r:g:b` or, with the colour-space id, `38:2::r:g:b`.
                            Some(2) if sub.len() >= 5 => {
                                let c = &sub[sub.len() - 3..];
                                Some(AnsiColor::Rgb(byte(c[0]), byte(c[1]), byte(c[2])))
                            }
                            _ => None,
                        };
                        set_color(style, sub0, color);
                    }
                    4 => style.underline = sub.get(1).copied().unwrap_or(1) != 0,
                    code => apply_code(style, code),
                }
            }
            continue;
        }
        let code = number(group);
        match code {
            38 | 48 | 58 => {
                let color = match groups.get(i).map(|g| number(g)) {
                    Some(5) => {
                        let n = groups.get(i + 1).map(|g| number(g));
                        i += 2;
                        n.map(|n| AnsiColor::Indexed(n.min(255) as u8))
                    }
                    Some(2) => {
                        let mut c = [0u32; 3];
                        let mut c_len = 0;
                        while c_len < 3 && i + 1 + c_len < groups.len() {
                            c[c_len] = number(groups[i + 1 + c_len]);
                            c_len += 1;
                        }
                        i += 1 + c_len;
                        (c_len == 3).then(|| AnsiColor::Rgb(byte(c[0]), byte(c[1]), byte(c[2])))
                    }
                    _ => None,
                };
                set_color(style, code, color);
            }
            code => apply_code(style, code),
        }
    }
}

/// Sets the foreground (38) or background (48) colour; 58 (underline colour) is ignored.
fn set_color(style: &mut AnsiStyle, code: u32, color: Option<AnsiColor>) {
    let Some(color) = color else {
        return;
    };
    match code {
        38 => style.fg = Some(color),
        48 => style.bg = Some(color),
        _ => {}
    }
}

/// A single SGR code without arguments.
fn apply_code(style: &mut AnsiStyle, code: u32) {
    match code {
        0 => *style = AnsiStyle::default(),
        1 => style.bold = true,
        2 => style.dim = true,
        3 => style.italic = true,
        4 | 21 => style.underline = true,
        7 => style.inverse = true,
        22 => {
            style.bold = false;
            style.dim = false;
        }
        23 => style.italic = false,
        24 => style.underline = false,
        27 => style.inverse = false,
        30..=37 => style.fg = Some(AnsiColor::Indexed((code - 30) as u8)),
        39 => style.fg = None,
        40..=47 => style.bg = Some(AnsiColor::Indexed((code - 40) as u8)),
        49 => style.bg = None,
        90..=97 => style.fg = Some(AnsiColor::Indexed((code - 90 + 8) as u8)),
        100..=107 => style.bg = Some(AnsiColor::Indexed((code - 100 + 8) as u8)),
        // Blink, conceal, strikethrough, fonts, frames...: not rendered.
        _ => {}
    }
}

/// Decimal value of a parameter (empty = 0, saturating, digits only).
fn number(digits: &[u8]) -> u32 {
    digits
        .iter()
        .filter(|b| b.is_ascii_digit())
        .fold(0u32, |n, &d| {
            n.saturating_mul(10).saturating_add((d - b'0') as u32)
        })
}

fn byte(n: u32) -> u8 {
    n.min(255) as u8
}

/// `text` with every `ESC` drawn as `␛`, for raw mode; borrowed when it holds none.
pub fn visible_escapes(text: &str) -> Cow<'_, str> {
    if !has_escape(text.as_bytes()) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace(ESC as char, &ESC_GLYPH.to_string()))
}

/// Maps byte offsets of `text` to offsets in `visible_escapes(text)`: each `ESC` before
/// an offset grows the text by two bytes (`␛` takes three). `offsets` are updated in place.
pub fn shift_for_visible_escapes(text: &str, offsets: &mut [&mut usize]) {
    let escapes: Vec<usize> = memchr::memchr_iter(ESC, text.as_bytes()).collect();
    if escapes.is_empty() {
        return;
    }
    let extra = ESC_GLYPH.len_utf8() - 1;
    for off in offsets.iter_mut() {
        let before = escapes.partition_point(|&e| e < **off);
        **off += before * extra;
    }
}

/// The xterm 256-colour table from index 16 on: the 6×6×6 cube, then 24 greys.
/// Indices 0..16 come from the theme (see `CyberTheme::ansi_palette`).
pub fn xterm_color(index: u8) -> [u8; 3] {
    match index {
        16..=231 => {
            let i = index - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            [level(i / 36), level((i / 6) % 6), level(i % 6)]
        }
        232..=255 => {
            let g = 8 + (index - 232) * 10;
            [g, g, g]
        }
        // The standard xterm base colours, used only without a theme.
        _ => {
            const BASE: [[u8; 3]; 16] = [
                [0, 0, 0],
                [205, 0, 0],
                [0, 205, 0],
                [205, 205, 0],
                [0, 0, 238],
                [205, 0, 205],
                [0, 205, 205],
                [229, 229, 229],
                [127, 127, 127],
                [255, 0, 0],
                [0, 255, 0],
                [255, 255, 0],
                [92, 92, 255],
                [255, 0, 255],
                [0, 255, 255],
                [255, 255, 255],
            ];
            BASE[index as usize]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fg(i: u8) -> Option<AnsiColor> {
        Some(AnsiColor::Indexed(i))
    }

    #[test]
    fn plain_line_is_borrowed_untouched() {
        let line = "2026-09-18 INFO plain line [31m without escape";
        assert!(!has_escape(line.as_bytes()));
        assert!(matches!(strip(line), Cow::Borrowed(s) if s == line));
        assert!(matches!(visible_escapes(line), Cow::Borrowed(_)));
        let owned = line.to_string();
        let ptr = owned.as_ptr();
        let kept = strip_owned(owned);
        assert_eq!(kept.as_ptr(), ptr, "no reallocation");
        let (text, runs) = strip_and_style(line);
        assert_eq!(text, line);
        assert!(runs.is_empty());
        assert!(!contains_sgr(line.as_bytes()));
    }

    #[test]
    fn strips_sgr_and_styles_the_runs() {
        let line = "\x1b[32mINFO \x1b[0mstarted \x1b[1;31mERROR\x1b[0m done";
        assert_eq!(strip(line), "INFO started ERROR done");
        let (text, runs) = strip_and_style(line);
        assert_eq!(text, "INFO started ERROR done");
        assert_eq!(runs.len(), 2);
        assert_eq!(&text[runs[0].start..runs[0].end], "INFO ");
        assert_eq!(runs[0].style.fg, fg(2));
        assert_eq!(&text[runs[1].start..runs[1].end], "ERROR");
        assert_eq!(runs[1].style.fg, fg(1));
        assert!(runs[1].style.bold);
        assert_eq!(style_runs(line), runs);
        assert!(contains_sgr(line.as_bytes()));
    }

    #[test]
    fn combined_attributes_and_their_resets() {
        let line = "\x1b[1;2;3;4;7;33;44ma\x1b[22mb\x1b[23mc\x1b[24md\x1b[27me\x1b[39mf\x1b[49mg";
        let (text, runs) = strip_and_style(line);
        assert_eq!(text, "abcdefg");
        let at = |c: char| {
            let off = text.find(c).unwrap();
            runs.iter()
                .find(|r| r.start <= off && off < r.end)
                .map(|r| r.style)
                .unwrap_or_default()
        };
        let a = at('a');
        assert!(a.bold && a.dim && a.italic && a.underline && a.inverse);
        assert_eq!((a.fg, a.bg), (fg(3), fg(4)));
        let b = at('b');
        assert!(!b.bold && !b.dim && b.italic);
        assert!(!at('c').italic);
        assert!(!at('d').underline);
        assert!(!at('e').inverse);
        assert_eq!(at('f').fg, None);
        assert_eq!(at('f').bg, fg(4));
        assert_eq!(at('g'), AnsiStyle::default(), "nothing left: no run");
        // ESC[m is a full reset, like ESC[0m.
        let (_, runs) = strip_and_style("\x1b[31mred\x1b[mplain");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].end, 3);
    }

    #[test]
    fn nested_colours_split_into_runs_and_same_styles_merge() {
        let (text, runs) = strip_and_style("\x1b[31ma\x1b[1mb\x1b[31mc\x1b[0md");
        assert_eq!(text, "abcd");
        assert_eq!(
            runs.iter().map(|r| (r.start, r.end)).collect::<Vec<_>>(),
            vec![(0, 1), (1, 3)],
            "`b` and `c` share bold red: one run"
        );
    }

    #[test]
    fn bright_256_and_truecolor() {
        let (_, runs) = strip_and_style("\x1b[91ma\x1b[102mb");
        assert_eq!(runs[0].style.fg, fg(9));
        assert_eq!(runs[1].style.bg, fg(10));
        let (_, runs) = strip_and_style("\x1b[38;5;208;48;5;17mx");
        assert_eq!(runs[0].style.fg, fg(208));
        assert_eq!(runs[0].style.bg, fg(17));
        let (_, runs) = strip_and_style("\x1b[38;2;10;20;300;1mx");
        assert_eq!(runs[0].style.fg, Some(AnsiColor::Rgb(10, 20, 255)));
        assert!(
            runs[0].style.bold,
            "parameters after the colour still apply"
        );
        let (_, runs) = strip_and_style("\x1b[38:2::1:2:3mx\x1b[48:5:99my");
        assert_eq!(runs[0].style.fg, Some(AnsiColor::Rgb(1, 2, 3)));
        assert_eq!(runs[1].style.bg, fg(99));
        // Truncated colour arguments are ignored, not misread.
        let (_, runs) = strip_and_style("\x1b[38;5mx");
        assert!(runs.is_empty());
        assert_eq!(xterm_color(16), [0, 0, 0]);
        assert_eq!(xterm_color(231), [255, 255, 255]);
        assert_eq!(xterm_color(196), [255, 0, 0]);
        assert_eq!(xterm_color(232), [8, 8, 8]);
        assert_eq!(xterm_color(255), [238, 238, 238]);
    }

    #[test]
    fn osc_and_other_sequences_are_removed() {
        // OSC 8 hyperlink, both terminators.
        let line = "see \x1b]8;;https://example.com\x1b\\the docs\x1b]8;;\x07 now";
        assert_eq!(strip(line), "see the docs now");
        // Window title, cursor movement, erase line, private mode: removed, no style.
        let line = "\x1b]0;title\x07\x1b[2K\x1b[1A\x1b[?25lprogress\x1b[?25h 50%";
        let (text, runs) = strip_and_style(line);
        assert_eq!(text, "progress 50%");
        assert!(runs.is_empty());
        assert!(
            !contains_sgr(line.as_bytes()),
            "non-SGR CSI does not trigger auto"
        );
        // Two-byte escapes (save cursor, charset) and one before a multi-byte char.
        assert_eq!(strip("a\x1b7b\x1b(Bc\x1bàd"), "abBcd");
    }

    #[test]
    fn malformed_and_truncated_sequences() {
        // Cut at the end of the line: dropped to the end.
        assert_eq!(strip("ok \x1b[1;3"), "ok ");
        assert_eq!(strip("ok \x1b]8;;http://cut"), "ok ");
        assert_eq!(strip("ok \x1b"), "ok ");
        // A control or non-ASCII byte ends a malformed CSI and is kept.
        assert_eq!(strip("a\x1b[31\tb"), "a\tb");
        assert_eq!(strip("a\x1b[3é"), "aé");
        // A parameter after an intermediate byte is malformed too.
        assert_eq!(strip("a\x1b[ 1mb"), "a1mb");
        // Never panics on arbitrary input, always valid UTF-8 at char boundaries.
        let tricky = "\x1b[\x1b]\x1b\x1b[38;2;\x1b[48;5;\x1b[;;;m€\x1b[";
        let (text, runs) = strip_and_style(tricky);
        assert_eq!(text, "€");
        for r in runs {
            assert!(text.is_char_boundary(r.start) && text.is_char_boundary(r.end));
        }
    }

    #[test]
    fn offset_map_round_trip() {
        let line = "\x1b[31mERROR\x1b[0m payment \x1b]8;;u\x07failed";
        let s = strip_with_map(line);
        assert_eq!(s.text, "ERROR payment failed");
        // Every stripped byte maps back to the same byte of the line.
        for (i, b) in s.text.bytes().enumerate() {
            assert_eq!(line.as_bytes()[s.to_raw(i)], b, "byte {i}");
        }
        let pay = s.text.find("payment").unwrap();
        assert_eq!(s.to_raw(pay), line.find("payment").unwrap());
        assert_eq!(s.to_raw(0), 5, "the text starts after ESC[31m");
        // Without escapes the map is the identity.
        let plain = strip_with_map("plain");
        assert_eq!(plain.to_raw(3), 3);
        assert_eq!(strip_with_map("").to_raw(0), 0);
    }

    #[test]
    fn style_runs_index_the_stripped_text() {
        let line = "pre \x1b[33mé€ \x1b[4;36mμ\x1b[0m post";
        let (text, runs) = strip_and_style(line);
        assert_eq!(text, strip(line));
        assert_eq!(&text[runs[0].start..runs[0].end], "é€ ");
        assert_eq!(&text[runs[1].start..runs[1].end], "μ");
        assert!(runs[1].style.underline);
    }

    #[test]
    fn visible_escapes_and_shifted_offsets() {
        let text = "\x1b[31mERR\x1b[0m x";
        let shown = visible_escapes(text);
        assert_eq!(shown, "␛[31mERR␛[0m x");
        let (mut a, mut b) = (text.find("ERR").unwrap(), text.find(" x").unwrap());
        shift_for_visible_escapes(text, &mut [&mut a, &mut b]);
        assert_eq!(&shown[a..a + 3], "ERR");
        assert_eq!(&shown[b..], " x");
    }

    #[test]
    fn mode_names_round_trip() {
        for mode in AnsiMode::ALL {
            assert_eq!(AnsiMode::from_name(mode.name()), Some(mode));
        }
        assert_eq!(AnsiMode::from_name(" RENDER "), Some(AnsiMode::Render));
        assert_eq!(AnsiMode::from_name("colour"), None);
        assert!(AnsiMode::Render.strips() && AnsiMode::Strip.strips());
        assert!(!AnsiMode::Raw.strips() && !AnsiMode::Auto.strips());
    }
}
