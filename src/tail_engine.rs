use crate::ansi::{AnsiMode, AnsiStyle, StyleRun};
use crate::audio::SoundAlertPreset;
use crate::file_source::FileSource;
use crate::log_level::{detect_level, LogLevel};
use crate::scan_job::{
    FilterSpec, JobSpec, ScanBatch, ScanJob, ScanKind, ScanRange, MAX_FILTER_TERMS,
};
use crate::time_histogram::TimeHistogram;
use crate::wildcard::{resolve_newest, split_pattern};
use crate::wrap_layout::{WrapAnchor, WrapScroll};
use egui::Color32;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FileEncoding {
    #[default]
    Utf8,
    Ascii,
    Ansi,
    UnicodeLe, // Unicode (UTF-16 Little Endian)
    UnicodeBe, // Unicode Big Endian (UTF-16 BE)
}

impl FileEncoding {
    pub fn name(&self) -> &'static str {
        match self {
            FileEncoding::Utf8 => "UTF-8",
            FileEncoding::Ascii => "ASCII",
            FileEncoding::Ansi => "ANSI",
            FileEncoding::UnicodeLe => "Unicode",
            FileEncoding::UnicodeBe => "Unicode BE",
        }
    }

    pub fn all() -> &'static [FileEncoding] {
        &[
            FileEncoding::Utf8,
            FileEncoding::Ascii,
            FileEncoding::Ansi,
            FileEncoding::UnicodeLe,
            FileEncoding::UnicodeBe,
        ]
    }

    /// The encoding whose `name()` is `name` (case-insensitive), used by session files.
    pub fn from_name(name: &str) -> Option<FileEncoding> {
        Self::all()
            .iter()
            .copied()
            .find(|e| e.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ViewMode {
    #[default]
    Text,
    Hex,
    Markdown,
    Filtered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SizeUnit {
    #[default]
    Bytes,
    MB,
    GB,
    Hex,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightStyle {
    pub fg: Color32,
    pub bg: Color32,
    pub bold: bool,
    pub italic: bool,
}

/// Upper bound on painted spans per row: bounds the layout work on pathological lines.
pub const MAX_ROW_SPANS: usize = 64;

/// Timestamp of a line that has none and inherits none (before the first timed line).
pub const NO_TIMESTAMP: i64 = i64::MIN;
/// Lines timed per call of `fill_timestamps`, the unit `ensure_timestamps` loops over.
pub const TIMESTAMP_FILL_BUDGET: usize = 200_000;
/// Below this share of timed lines that can be placed in time (a timestamp of their own
/// or inherited) the file is not one we can read times from, and the time controls say so.
pub const MIN_TIMESTAMP_RATE: f32 = 0.5;
/// Lines to look at before trusting the rate above (a header of untimed banner lines
/// must not disable the controls for the whole file).
pub const TIMESTAMP_RATE_SAMPLE: usize = 200;
/// Visible lines scanned for the time span of an out-of-order log.
pub const MAX_SPAN_SCAN: usize = 100_000;
/// Lines looked at from each end of the view for the time span before the cache is
/// built: enough to step over a stack trace, few enough to stay free per frame.
pub const SPAN_PROBE_LINES: usize = 256;

/// What the time delta column shows on a row (see `TailEngine::row_time_delta`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeDelta {
    /// Nothing: a continuation line, the first visible row, or no known time.
    Blank,
    /// The row, or the anchor it is measured from, has not been timed yet.
    Pending,
    /// The row is the time anchor itself.
    Anchor,
    /// Signed milliseconds from the previous visible row, or from the anchor when one is
    /// set.
    Millis(i64),
}

/// Style of a painted span: a captures-only rule's style, preset colour `1..=9` of a
/// quick label, or the SGR attributes of an ANSI-coloured run (both resolved by the theme
/// in the renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpanStyle {
    Rule(HighlightStyle),
    Label(u8),
    Ansi(AnsiStyle),
}

/// A byte range `[start, end)` of a row painted with its own style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub style: SpanStyle,
}

/// Result of the span evaluation of a row: the painted spans (sorted, non-overlapping,
/// at most `MAX_ROW_SPANS`) and the style of the remaining bytes when a whole-row rule
/// matched (`None` leaves them in the default style, or the level palette).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpanHighlight {
    pub spans: Vec<HighlightSpan>,
    pub rest: Option<HighlightStyle>,
}

/// A row as the text view draws it (see `TailEngine::get_row`): the text of the line
/// (without its escape sequences in render and strip modes), its ANSI style runs in
/// render mode, and whether raw mode draws its `ESC` bytes as `␛`.
#[derive(Debug, Clone, PartialEq)]
pub struct RowText {
    pub line: String,
    pub ansi: Vec<StyleRun>,
    pub visible_escapes: bool,
}

impl RowText {
    /// The text to lay out and `spans` in its offsets: the line itself, or in raw mode the
    /// line with `␛` glyphs and the spans shifted past them.
    pub fn display(&self, spans: Option<SpanHighlight>) -> (Cow<'_, str>, Option<SpanHighlight>) {
        if !self.visible_escapes {
            return (Cow::Borrowed(&self.line), spans);
        }
        let spans = spans.map(|mut highlight| {
            let mut offsets: Vec<&mut usize> = highlight
                .spans
                .iter_mut()
                .flat_map(|sp| {
                    let HighlightSpan { start, end, .. } = sp;
                    [start, end]
                })
                .collect();
            crate::ansi::shift_for_visible_escapes(&self.line, &mut offsets);
            highlight
        });
        (crate::ansi::visible_escapes(&self.line), spans)
    }
}

/// An ad-hoc colour label (Ctrl+Shift+1..9): case-insensitive plain text painted with
/// preset colour `color` (1..=9) in every stream. Lives in memory only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickLabel {
    pub text: String,
    pub color: u8,
}

impl QuickLabel {
    /// Creates, recolours or removes the label for `text`: a label with the same text and
    /// colour is removed, one with another colour takes `color`, otherwise it is added.
    /// Returns `false` when `text` is blank and nothing changed.
    pub fn toggle(labels: &mut Vec<QuickLabel>, text: &str, color: u8) -> bool {
        let text = text.trim();
        if text.is_empty() || !(1..=9).contains(&color) {
            return false;
        }
        if let Some(pos) = labels.iter().position(|l| {
            l.text.eq_ignore_ascii_case(text) || l.text.to_lowercase() == text.to_lowercase()
        }) {
            if labels[pos].color == color {
                labels.remove(pos);
            } else {
                labels[pos].color = color;
            }
        } else {
            labels.push(QuickLabel {
                text: text.to_string(),
                color,
            });
        }
        true
    }
}

/// Cap on queued external-tool hits per stream (see `TailEngine::collect_tool_hits`).
pub const MAX_PENDING_TOOL_HITS: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HighlightRule {
    pub pattern: String,
    pub is_regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    pub fg_color: [u8; 3],
    pub bg_color: [u8; 3],
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub sound_alert: SoundAlertPreset,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Regex rules only: paint the captured groups (the whole match without groups)
    /// instead of the whole row.
    #[serde(default)]
    pub captures_only: bool,
}

impl HighlightRule {
    pub fn new(pattern: &str, fg: [u8; 3], bg: [u8; 3], is_regex: bool) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex,
            case_sensitive: false,
            fg_color: fg,
            bg_color: bg,
            bold: false,
            italic: false,
            sound_alert: SoundAlertPreset::None,
            enabled: true,
            captures_only: false,
        }
    }

    /// A regex rule painting only its captured groups.
    pub fn captures(pattern: &str, fg: [u8; 3], bg: [u8; 3]) -> Self {
        let mut rule = Self::new(pattern, fg, bg, true);
        rule.captures_only = true;
        rule
    }

    fn style(&self) -> HighlightStyle {
        HighlightStyle {
            fg: Color32::from_rgb(self.fg_color[0], self.fg_color[1], self.fg_color[2]),
            bg: Color32::from_rgb(self.bg_color[0], self.bg_color[1], self.bg_color[2]),
            bold: self.bold,
            italic: self.italic,
        }
    }

    pub fn with_style(
        pattern: &str,
        fg: [u8; 3],
        bg: [u8; 3],
        is_regex: bool,
        bold: bool,
        italic: bool,
    ) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex,
            case_sensitive: false,
            fg_color: fg,
            bg_color: bg,
            bold,
            italic,
            sound_alert: SoundAlertPreset::None,
            enabled: true,
            captures_only: false,
        }
    }

    pub fn with_alert(
        pattern: &str,
        fg: [u8; 3],
        bg: [u8; 3],
        is_regex: bool,
        sound_alert: SoundAlertPreset,
    ) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex,
            case_sensitive: false,
            fg_color: fg,
            bg_color: bg,
            bold: false,
            italic: false,
            sound_alert,
            enabled: true,
            captures_only: false,
        }
    }
}

/// Upper bound on the stored line hits of a search: 8 bytes each, so at most 8 MB per
/// stream. Past it the search keeps counting (`search_total`) without storing.
pub const MAX_SEARCH_MATCHES: usize = 1_000_000;
/// Upper bound on the byte-level hits of the HEX view.
pub const MAX_BYTE_MATCHES: usize = 20_000;
/// Lines per bucket of the ERROR / FATAL counts kept beside the level cache.
pub const ERROR_BLOCK_LINES: usize = 4096;

/// Outcome of a synchronous line search: the hits listed (at most the limit), every hit
/// counted, and the last line counted.
#[derive(Debug, Default)]
struct LineHits {
    lines: Vec<usize>,
    total: usize,
    last: Option<usize>,
}

/// How many of the `total` counted hits lie at or after line `start`, which a rescan
/// from `start` replaces. `keep` of the `stored` listed hits are before `start`; the hits
/// counted past the cap all follow the listed ones, and `last_counted` bounds the last of
/// them (`exact` when it is that line). `None` when the answer is not known: the list is
/// capped, every listed hit is before `start`, and counted-only hits may sit on both
/// sides of it.
fn counted_hits_from(
    start: usize,
    keep: usize,
    stored: usize,
    total: usize,
    last_counted: usize,
    exact: bool,
) -> Option<usize> {
    if total <= stored {
        return Some(stored - keep);
    }
    if keep < stored {
        // A listed hit is at or after `start`, so every counted-only hit is too.
        return Some(total - keep);
    }
    if last_counted < start {
        Some(0)
    } else if exact && last_counted == start {
        // The counted hits are distinct lines, and the last one is `start` itself.
        Some(1)
    } else {
        None
    }
}

/// Whether a cached level byte is ERROR or FATAL (the levels the overview strip marks).
pub fn is_error_level(v: u8) -> bool {
    v == LogLevel::Error as u8 || v == LogLevel::Fatal as u8
}

/// Sets the first row of a term list, adding it when the list is empty.
fn set_first_term(terms: &mut Vec<String>, text: &str) {
    match terms.first_mut() {
        Some(first) => *first = text.to_string(),
        None => terms.push(text.to_string()),
    }
}

/// Non-empty terms after the first row.
fn non_empty_after_first(terms: &[String]) -> usize {
    terms.iter().skip(1).filter(|t| !t.is_empty()).count()
}

/// Bytes read per step by sequential scans (indexing, byte search).
const SCAN_CHUNK: usize = 1024 * 1024;
/// A single line longer than this is shown truncated, with a marker.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Rendered Markdown needs the whole text: larger files stay in text mode.
pub const MARKDOWN_MAX_BYTES: u64 = 1024 * 1024;
/// Files larger than this run filters and search on a worker thread.
pub const JOB_THRESHOLD_BYTES: u64 = 16 * 1024 * 1024;
/// Files larger than this build their line index on a worker thread.
pub const INDEX_JOB_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;
/// Bytes remembered at the head and at the indexed end to recognise rewrites.
const FINGERPRINT_LEN: usize = 64;
/// Bytes the encoding and binary detection look at.
pub const ENCODING_SAMPLE_BYTES: u64 = 512;
/// Marker appended to a line cut at `MAX_LINE_BYTES`.
pub const TRUNCATED_LINE_MARKER: &str = " …[line truncated]";

/// Decodes one raw line (bytes between two offsets, newline included) in `encoding`,
/// dropping the trailing newline / CR LF unless the line was cut by the length cap.
pub(crate) fn decode_line(bytes: &[u8], encoding: FileEncoding, truncated: bool) -> String {
    let mut s = decode_content(bytes, encoding, truncated);
    if truncated {
        s.push_str(TRUNCATED_LINE_MARKER);
    }
    s
}

/// `decode_line` for a stream whose text features see the line without its escape
/// sequences (`strip`): they are removed before the truncation marker is appended, so a
/// sequence cut by the length cap cannot swallow the marker.
pub(crate) fn decode_line_ansi(
    bytes: &[u8],
    encoding: FileEncoding,
    truncated: bool,
    strip: bool,
) -> String {
    if !strip {
        return decode_line(bytes, encoding, truncated);
    }
    let mut s = crate::ansi::strip_owned(decode_content(bytes, encoding, truncated));
    if truncated {
        s.push_str(TRUNCATED_LINE_MARKER);
    }
    s
}

/// The decoded text of a raw line, without the truncation marker.
fn decode_content(bytes: &[u8], encoding: FileEncoding, truncated: bool) -> String {
    match encoding {
        FileEncoding::Utf8 => {
            let end = if truncated {
                bytes.len()
            } else {
                trim_newline_1(bytes)
            };
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        }
        FileEncoding::Ascii => {
            let end = if truncated {
                bytes.len()
            } else {
                trim_newline_1(bytes)
            };
            bytes[..end]
                .iter()
                .map(|&b| if b <= 127 { b as char } else { '?' })
                .collect()
        }
        FileEncoding::Ansi => {
            let end = if truncated {
                bytes.len()
            } else {
                trim_newline_1(bytes)
            };
            bytes[..end].iter().map(|&b| b as char).collect()
        }
        FileEncoding::UnicodeLe | FileEncoding::UnicodeBe => {
            let le = encoding == FileEncoding::UnicodeLe;
            let end = if truncated {
                bytes.len() & !1
            } else {
                trim_newline_2(bytes, le)
            };
            let units = bytes[..end].as_chunks::<2>().0.iter().map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            });
            char::decode_utf16(units)
                .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect()
        }
    }
}

/// Bytes `text` (decoded from the file) takes in the file in `encoding`, used to turn an
/// offset inside a decoded line back into a file offset.
fn encoded_len(text: &str, encoding: FileEncoding) -> usize {
    match encoding {
        FileEncoding::Utf8 => text.len(),
        FileEncoding::Ascii | FileEncoding::Ansi => text.chars().count(),
        FileEncoding::UnicodeLe | FileEncoding::UnicodeBe => {
            text.chars().map(char::len_utf16).sum::<usize>() * 2
        }
    }
}

/// End of the line content for single-byte encodings: strips `\n` and a preceding `\r`.
pub(crate) fn trim_newline_1(bytes: &[u8]) -> usize {
    let mut end = bytes.len();
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && bytes[end - 1] == b'\r' {
            end -= 1;
        }
    }
    end
}

/// End of the line content for UTF-16: strips the newline and a preceding CR (2 bytes each).
fn trim_newline_2(bytes: &[u8], le: bool) -> usize {
    let (nl, cr): ([u8; 2], [u8; 2]) = if le {
        ([0x0A, 0x00], [0x0D, 0x00])
    } else {
        ([0x00, 0x0A], [0x00, 0x0D])
    };
    let mut end = bytes.len() & !1;
    if end >= 2 && bytes[end - 2..end] == nl {
        end -= 2;
        if end >= 2 && bytes[end - 2..end] == cr {
            end -= 2;
        }
    }
    end
}

/// Result of a go-to-line request (see `TailEngine::resolve_goto`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GotoTarget {
    /// The requested line after clamping (0-based).
    pub requested: usize,
    /// The line the view will actually show (0-based).
    pub line: usize,
    /// True when `requested` is hidden by the filters and `line` was substituted.
    pub hidden: bool,
    /// True when the input is a time and the stream is still being timed in the
    /// background: nothing to scroll to yet, the jump happens when timing finishes (see
    /// `TailEngine::take_goto_time_result`). `requested` and `line` mean nothing then.
    pub waiting: bool,
}

/// Whether a line timed at `millis` falls inside the window `[from, to]` (either side
/// open when `None`). A line with no timestamp cannot be placed in time and is outside.
pub fn time_window_contains(millis: Option<i64>, from: Option<i64>, to: Option<i64>) -> bool {
    let Some(millis) = millis else {
        return false;
    };
    from.is_none_or(|from| millis >= from) && to.is_none_or(|to| millis <= to)
}

/// A time window entered before the stream was fully timed, applied once it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingWindow {
    /// Re-read the two time fields: a bare `14:02` belongs to the day of the first
    /// timestamp, known only once the head of the file has been timed.
    Texts,
    /// A window given in milliseconds (`set_time_range`).
    Range(Option<i64>, Option<i64>),
}

/// Efficient case-insensitive substring search via callback.
/// Invokes `on_match(start, end)` for every non-overlapping match.
/// Returning `false` from `on_match` halts the search early.
pub(crate) fn find_case_insensitive_cb(
    haystack: &str,
    needle_lower: &str,
    mut on_match: impl FnMut(usize, usize) -> bool,
) {
    if needle_lower.is_empty() {
        return;
    }
    if needle_lower.is_ascii() {
        let h = haystack.as_bytes();
        let n = needle_lower.as_bytes();
        let n_len = n.len();
        if n_len == 0 || n_len > h.len() {
            return;
        }

        // SIMD-accelerated ASCII case-insensitive search:
        // Scans rapidly using memchr / memchr2 on the first character's byte variants.
        // In UTF-8, ASCII bytes (0..127) never overlap with multi-byte sequence bytes (128..255),
        // so checking haystack.is_ascii() upfront is unnecessary overhead.
        let first_lower = n[0].to_ascii_lowercase();
        let first_upper = first_lower.to_ascii_uppercase();
        let max_pos = h.len() - n_len;
        let mut i = 0;

        if first_lower == first_upper {
            while i <= max_pos {
                match memchr::memchr(first_lower, &h[i..=max_pos]) {
                    Some(rel) => {
                        i += rel;
                        if h[i..i + n_len].eq_ignore_ascii_case(n) {
                            if !on_match(i, i + n_len) {
                                return;
                            }
                            i += n_len;
                        } else {
                            i += 1;
                        }
                    }
                    None => break,
                }
            }
        } else {
            while i <= max_pos {
                match memchr::memchr2(first_lower, first_upper, &h[i..=max_pos]) {
                    Some(rel) => {
                        i += rel;
                        if h[i..i + n_len].eq_ignore_ascii_case(n) {
                            if !on_match(i, i + n_len) {
                                return;
                            }
                            i += n_len;
                        } else {
                            i += 1;
                        }
                    }
                    None => break,
                }
            }
        }
        return;
    }
    // Lower-casing can change byte lengths: keep a map from each byte of the lowered
    // text back to the start and end offsets of the original character.
    let mut lowered = String::with_capacity(haystack.len());
    let mut starts = Vec::with_capacity(haystack.len());
    let mut ends = Vec::with_capacity(haystack.len());
    for (idx, ch) in haystack.char_indices() {
        let before = lowered.len();
        for lc in ch.to_lowercase() {
            lowered.push(lc);
        }
        for _ in before..lowered.len() {
            starts.push(idx);
            ends.push(idx + ch.len_utf8());
        }
    }
    let mut from = 0;
    while let Some(pos) = lowered[from..].find(needle_lower) {
        let s = from + pos;
        let e = s + needle_lower.len();
        if !on_match(starts[s], ends[e - 1]) {
            break;
        }
        from = e;
    }
}

/// Efficient case-insensitive substring search.
/// Byte ranges of every non-overlapping, case-insensitive occurrence of `needle_lower`
/// (already lower-cased) in `haystack`.
pub(crate) fn find_case_insensitive(haystack: &str, needle_lower: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    find_case_insensitive_cb(haystack, needle_lower, |s, e| {
        out.push((s, e));
        true
    });
    out
}

pub(crate) fn contains_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if needle_lower.is_ascii() {
        let h_bytes = haystack.as_bytes();
        let n_bytes = needle_lower.as_bytes();
        let n_len = n_bytes.len();
        if n_len > h_bytes.len() {
            return false;
        }

        let first_lower = n_bytes[0].to_ascii_lowercase();
        let first_upper = first_lower.to_ascii_uppercase();
        let max_pos = h_bytes.len() - n_len;
        let mut curr = 0;

        if first_lower == first_upper {
            while curr <= max_pos {
                let match_rel = match memchr::memchr(first_lower, &h_bytes[curr..=max_pos]) {
                    Some(rel) => rel,
                    None => return false,
                };

                curr += match_rel;
                if h_bytes[curr..curr + n_len].eq_ignore_ascii_case(n_bytes) {
                    return true;
                }
                curr += 1;
            }
        } else {
            while curr <= max_pos {
                let match_rel =
                    match memchr::memchr2(first_lower, first_upper, &h_bytes[curr..=max_pos]) {
                        Some(rel) => rel,
                        None => return false,
                    };

                curr += match_rel;
                if h_bytes[curr..curr + n_len].eq_ignore_ascii_case(n_bytes) {
                    return true;
                }
                curr += 1;
            }
        }
        false
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

/// Precompiled highlight rule for fast evaluation per row.
#[derive(Debug, Clone)]
pub struct CompiledHighlight {
    pub regex: Option<Regex>,
    pub pattern_lower: String,
    pub style: HighlightStyle,
    pub enabled: bool,
    pub captures_only: bool,
    pub case_sensitive: bool,
    pub pattern: String,
    pub sound_alert: SoundAlertPreset,
}

/// Adds `[start, end)` minus the bytes already claimed by `spans`; returns `true` once the
/// cap of `MAX_ROW_SPANS` is reached.
fn claim_span(spans: &mut Vec<HighlightSpan>, start: usize, end: usize, style: SpanStyle) -> bool {
    // Fast path: if no spans exist yet, push directly without any piece vector allocation.
    if spans.is_empty() {
        spans.push(HighlightSpan { start, end, style });
        return spans.len() >= MAX_ROW_SPANS;
    }
    // Double-buffer piece vectors and reuse them via drain and swap to eliminate heap
    // allocation churn during interval subtraction.
    let mut pieces = vec![(start, end)];
    let mut next_pieces = Vec::with_capacity(4);
    for sp in spans.iter() {
        for (s, e) in pieces.drain(..) {
            if e <= sp.start || s >= sp.end {
                next_pieces.push((s, e));
            } else {
                if s < sp.start {
                    next_pieces.push((s, sp.start));
                }
                if e > sp.end {
                    next_pieces.push((sp.end, e));
                }
            }
        }
        std::mem::swap(&mut pieces, &mut next_pieces);
        if pieces.is_empty() {
            break;
        }
    }
    for (s, e) in pieces {
        if spans.len() >= MAX_ROW_SPANS {
            return true;
        }
        spans.push(HighlightSpan {
            start: s,
            end: e,
            style,
        });
    }
    spans.len() >= MAX_ROW_SPANS
}

pub struct TailEngine {
    pub path: PathBuf,
    /// On-demand access to the file: a shared handle plus a small block cache. No copy of
    /// the file lives in memory.
    pub source: FileSource,
    /// File-name pattern (`*` / `?`) of a pattern stream. `path` is then `dir/pattern`,
    /// the stable identity of the stream, and `current_file` the resolved file being
    /// tailed (`None` while nothing matches yet).
    pub pattern: Option<String>,
    pub current_file: Option<PathBuf>,
    /// Minimum time between two directory scans of a pattern stream (2 s; tests use 0).
    pub pattern_scan_interval: Duration,
    last_pattern_scan: Instant,
    /// Minimum time between fallback size checks via filesystem metadata (500 ms; tests use 0).
    pub size_check_interval: Duration,
    last_size_check: Instant,
    /// Name of the file the stream last switched to and when, for the stream bar notice.
    pub switch_notice: Option<(String, Instant)>,
    /// First bytes of the file and the bytes before the indexed end, used to tell a rewrite
    /// from an append (see `fingerprints_match`).
    head_fingerprint: Vec<u8>,
    tail_fingerprint: (u64, Vec<u8>),
    pub line_offsets: Vec<u64>,
    pub file_size: u64,
    pub last_modified: Option<std::time::SystemTime>,
    pub follow_tail: bool,
    pub is_watching: bool,
    pub has_new_data: bool,
    pub view_mode: ViewMode,
    pub hex_columns: usize,
    pub size_unit: SizeUnit,
    pub encoding: FileEncoding,
    /// Visible time window, either side optional (`None` = open). Applied on top of the
    /// include/exclude filters and the level filter.
    pub time_from: Option<i64>,
    pub time_to: Option<i64>,
    /// What the user typed into the two time fields, kept verbatim: the text is what the
    /// field shows, and re-reading it is how "14:02" follows the day of the log.
    pub time_from_text: String,
    pub time_to_text: String,
    /// One of the two time fields does not parse: the row says so instead of silently
    /// leaving that side open.
    pub time_range_error: bool,
    /// Include terms (all must match) and exclude terms (none may match), at most
    /// `MAX_FILTER_TERMS` each; empty rows are kept for the editor and ignored. The first
    /// term of each side is the stream bar's field (`include_filter` / `exclude_filter`).
    include_terms: Vec<String>,
    exclude_terms: Vec<String>,
    /// Preset last applied to this stream, in memory only: with the filter state edited
    /// since, the presets drop-down shows it as `name *` (see `filter_preset`).
    pub applied_preset: Option<String>,
    pub filter_case_sensitive: bool,
    pub filter_is_regex: bool,
    /// Minimum-level stage of the filter (`Unknown` = off) and its unknown-level toggle.
    pub min_level: LogLevel,
    pub show_unknown_levels: bool,
    /// Detected level of every line (`LogLevel as u8`), a prefix of the line index: lines
    /// `>= levels.len()` have not been examined yet. Filled on append, on open for files up
    /// to the job threshold, and by a background `Levels` job for larger ones.
    levels: Vec<u8>,
    /// Effective timestamp of each indexed line in milliseconds since the epoch, with
    /// `NO_TIMESTAMP` for the lines before the first one that carried a time. A line
    /// without a timestamp of its own inherits the previous line's, so a stack trace stays
    /// with the entry it belongs to. Filled lazily: a log is only timed when the time
    /// range or a time jump asks for it.
    timestamps: Vec<i64>,
    /// Format that matched last, tried first on the next line (see `crate::timestamp`).
    timestamp_hint: crate::timestamp::FormatHint,
    /// Lines that carried a timestamp of their own, out of the lines timed so far: the
    /// time controls are pointless on a log whose format we do not read.
    timestamps_parsed: usize,
    /// Set when a timed line goes back in time: go-to-time then scans instead of bisecting.
    timestamps_unordered: bool,
    /// Someone needs the whole stream timed (a time window, a time jump) and the
    /// background scan has not finished yet: started, or resumed from the prefix, as soon
    /// as the running job allows it (see `request_timestamps`).
    timestamps_wanted: bool,
    /// Line the time delta column measures from ("Set time anchor here"), instead of the
    /// previous visible row. A line index, so it survives filter changes; dropped with the
    /// selection when the index is rebuilt, and never persisted.
    time_anchor: Option<usize>,
    /// Time window waiting for the cache to be complete; the view keeps what it showed.
    pending_window: Option<PendingWindow>,
    /// Go-to-time input waiting for the cache, and its outcome once resolved, for the
    /// Ctrl+G popup to pick up (`None` inside: the time could not be resolved).
    pending_goto_time: Option<String>,
    goto_time_result: Option<Option<GotoTarget>>,
    /// Lines per level among the cached prefix, indexed by `LogLevel as u8`.
    pub level_counts: [u64; LogLevel::COUNT],
    /// ERROR and FATAL lines per `ERROR_BLOCK_LINES` lines of the cached level prefix,
    /// for the overview strip: block `b` covers lines `b * ERROR_BLOCK_LINES ..`.
    error_blocks: Vec<u32>,
    /// Lines per level over time (see `crate::time_histogram`), fed from the lines that
    /// have both a cached level and a cached timestamp: `0..histogram_len`.
    histogram: TimeHistogram,
    histogram_len: usize,
    /// Bumped whenever the histogram changes; keys the UI caches built over it.
    pub histogram_generation: u64,
    /// Compiled include/exclude filter, shared with filter and search jobs.
    filter: FilterSpec,
    /// Running background scan, if any (one at a time per stream).
    job: Option<ScanJob>,
    job_generation: u64,
    /// Line index from which the derived state must be refreshed once the job ends.
    pending_refresh_from: Option<usize>,
    /// Full filter / search recomputation requested while another job was running.
    pending_filter: bool,
    pending_search: bool,
    /// True while the line index is being built in the background.
    pub index_pending: bool,
    /// Files larger than these run filters/search, or the initial index, on a worker thread.
    pub job_threshold_bytes: u64,
    pub index_job_threshold_bytes: u64,
    pub filtered_lines: Vec<usize>,
    /// Bumped whenever `filtered_lines` or the filter behind it may have changed; keys
    /// the UI caches built over the visible rows.
    pub filter_generation: u64,
    pub search_query: String,
    /// Line hits of the active search in file order, at most `MAX_SEARCH_MATCHES`.
    pub search_matches: Vec<usize>,
    /// Every visible line matching the active search, counted past the cap too.
    search_total: usize,
    /// Upper bound on the last line counted in `search_total`, and whether it is that line
    /// exactly. Only read once the list is capped: an append that re-examines lines past
    /// the stored hits needs to know which of the counted-only hits it replaces.
    search_last_counted: usize,
    search_last_counted_exact: bool,
    /// Bumped whenever `search_matches` changes; keys the UI caches built over the hits.
    pub search_generation: u64,
    /// Selected rows (line indices) for copy/export; `selection_all` marks "every visible row".
    pub selection: BTreeSet<usize>,
    pub selection_all: bool,
    pub selection_anchor: Option<usize>,
    /// Go-to-line box state (per stream): open flag, typed text, last notice.
    pub goto_open: bool,
    pub goto_input: String,
    pub goto_notice: Option<String>,
    /// Short notice shown in the stream bar (e.g. Markdown refused above the size cap).
    pub view_notice: Option<String>,
    /// Bookmarked line indices, the last bookmark jumped to, and a dirty flag for persistence.
    pub bookmarks: BTreeSet<usize>,
    pub bookmark_cursor: Option<usize>,
    pub bookmarks_dirty: bool,
    /// Bumped whenever the bookmark set changes (keys the overview strip).
    pub bookmarks_generation: u64,
    /// Soft-wrap rows at the viewport width (per stream, persisted in the workspace) and a
    /// dirty flag for persistence.
    pub wrap_lines: bool,
    pub wrap_dirty: bool,
    /// Wrap-mode viewport state, see `wrap_layout`: the anchor row and its hidden pixels,
    /// the approximate offset last handed to the scroll bar, the running average row
    /// height behind that offset, whether the view sits on the last row (follow sticks
    /// only then) and a pending scroll request.
    pub wrap_anchor: WrapAnchor,
    pub wrap_virtual_offset: Option<f32>,
    /// User scrolling measured after the last frame: pixels moved against the offset the
    /// view handed to the scroll area, and the absolute offset it ended on.
    pub wrap_scroll_delta: f32,
    pub wrap_scroll_abs: f32,
    pub wrap_avg_row_height: f32,
    pub wrap_at_bottom: bool,
    pub wrap_request: Option<WrapScroll>,
    /// Background-tab activity: set by the viewer when the stream is drawn, counted by the
    /// engine while it is not; severity 0 = plain lines, 1 = highlight match, 2 = sound alert.
    pub displayed: bool,
    pub unseen_lines: usize,
    pub unseen_severity: u8,
    /// Byte-level hits `(offset, len)` used by the HEX view (text and hex-pattern queries).
    pub search_byte_matches: Vec<(usize, usize)>,
    search_byte_max_len: usize,
    pub current_match_idx: Option<usize>,
    pub last_searched_query: String,
    /// Byte offset the HEX view should scroll to after F3 / Shift+F3.
    pub scroll_to_byte: Option<usize>,
    /// When the user last typed in the search box (used to debounce the rescan).
    pub search_edited_at: Option<Instant>,
    /// Bumped every time the buffer / line index is rebuilt; keys derived caches.
    pub buffer_generation: u64,
    /// Bumped when line indices stop meaning the same lines: reload after a truncation,
    /// rotation or rewrite, a re-decode, a pattern switch. Appends leave it alone. Results
    /// kept outside the engine (a search across streams) compare it to know they are stale.
    pub reload_generation: u64,
    /// Line a search across streams asked the view to show (see `request_jump`); the
    /// stream viewer centres it on its next frame, when it knows the row geometry.
    pub pending_jump: Option<usize>,
    /// The stream bar's "search all streams" button was pressed: the app opens the Find
    /// results tab with this stream's query after the dock is drawn.
    pub find_all_request: bool,
    /// Markdown-mode text (HTML converted when needed), cached per buffer generation.
    pub markdown_text_cache: Option<(u64, String)>,
    pub highlight_rules: Vec<HighlightRule>,
    compiled_highlights: Vec<CompiledHighlight>,
    /// Quick labels (see `QuickLabel`) with their lower-cased text, evaluated after the
    /// user rules by `match_highlight_spans`.
    quick_labels: Vec<(QuickLabel, String)>,
    pub expanded_json_lines: HashSet<usize>,
    pub requested_scroll_x: Option<f32>,
    pub requested_scroll_y: Option<f32>,
    pub scroll_to_line: Option<usize>,
    pub markdown_cache: egui_commonmark::CommonMarkCache,
    pub markdown_max_bytes: u64,
    pub current_scroll_x: f32,
    pub current_scroll_y: f32,
    pub max_line_bytes: usize,
    pub max_detected_width: f32,
    pub last_sound_alert_time: Instant,
    /// Patterns of the highlight rules that have an external tool bound to them; set by
    /// the app from the tools list. Matches on appended lines are queued in
    /// `pending_tool_hits` as `(rule pattern, line index)` for the app to run, capped at
    /// `MAX_PENDING_TOOL_HITS` per poll (the runner throttles anyway).
    pub tool_bound_rules: HashSet<String>,
    pub pending_tool_hits: Vec<(String, usize)>,
    _watcher: Option<RecommendedWatcher>,
    rx: Receiver<notify::Result<Event>>,
    pub last_read_time: Instant,
    pub bytes_read_since_tick: u64,
    pub throughput_bps: f64,
    /// The stream opened on an empty file: its encoding and binary detection had no
    /// sample and run again once it holds `ENCODING_SAMPLE_BYTES` (or, for a compressed
    /// stream, when the decompression ends with fewer).
    pub encoding_pending: bool,
    /// ANSI escape handling chosen for the stream (`Auto` unless the user picked a mode),
    /// a dirty flag for persistence, whether auto-detection has seen an SGR sequence, and
    /// when auto mode switched to render on appended data (for the stream bar notice).
    pub ansi_mode: AnsiMode,
    pub ansi_dirty: bool,
    ansi_detected: bool,
    pub ansi_switched_at: Option<Instant>,
    /// Auto-detection samples the head of what the stream held when it was read from the
    /// start (bytes before this offset); every byte appended after it is examined.
    ansi_head_end: u64,
    /// Decompression job and spool of a stream read from a gzip file or a zip entry
    /// (see `compressed`); `path` is then the archive (or `archive/entry`) and
    /// `current_file` the spool. Declared last so the file handle above is closed before
    /// the spool is deleted.
    pub compressed: Option<crate::compressed::CompressedStream>,
    /// Copier and spool of the standard-input stream (see `stdin_source`); `path` is
    /// then `<stdin>` and `current_file` the spool. Last for the same reason.
    pub stdin: Option<crate::stdin_source::StdinStream>,
}

/// Directory scan cadence of a pattern stream.
pub const PATTERN_SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Minimum cadence between fallback size checks via filesystem metadata (500 ms).
pub const SIZE_CHECK_INTERVAL: Duration = Duration::from_millis(500);
pub const SWITCH_NOTICE_DURATION: Duration = Duration::from_secs(5);

/// Callback fired from the filesystem watcher thread as soon as an event arrives, used
/// by the app to wake the event loop so an idle software-rendered window still picks up
/// new lines without a periodic repaint.
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

impl TailEngine {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        Self::open_with_thresholds(path, JOB_THRESHOLD_BYTES, INDEX_JOB_THRESHOLD_BYTES)
    }

    /// Like `open`, with a callback invoked from the filesystem watcher thread as soon as
    /// an event arrives (used by the app to wake an idle event loop without repainting).
    pub fn open_with_wake<P: AsRef<Path>>(path: P, wake: WakeFn) -> Result<Self, std::io::Error> {
        Self::open_impl(
            path,
            JOB_THRESHOLD_BYTES,
            INDEX_JOB_THRESHOLD_BYTES,
            Some(wake),
        )
    }

    /// Like `open`, with explicit sizes above which filters/search and the initial index
    /// run on a worker thread (tests use 0 to exercise the background paths on small files).
    pub fn open_with_thresholds<P: AsRef<Path>>(
        path: P,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
    ) -> Result<Self, std::io::Error> {
        Self::open_impl(path, job_threshold_bytes, index_job_threshold_bytes, None)
    }

    fn open_impl<P: AsRef<Path>>(
        path: P,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
        wake: Option<WakeFn>,
    ) -> Result<Self, std::io::Error> {
        let path_buf = path.as_ref().to_path_buf();
        let metadata = std::fs::metadata(&path_buf)?;
        // Security check: ensure target path is a regular file.
        // Prevents opening directories, device nodes (/dev/zero, /dev/urandom),
        // FIFOs/named pipes, or sockets that cause hanging or infinite memory allocation (DoS).
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Target path is not a regular file",
            ));
        }
        let file_size = metadata.len();
        let last_modified = metadata.modified().ok();

        let source = FileSource::open(&path_buf)?;
        let sample = source.read_to_vec(0, 512);
        let (detected_encoding, is_binary) = Self::detect_encoding(&sample);
        let view_mode =
            Self::initial_view_mode(&path_buf, is_binary, file_size, MARKDOWN_MAX_BYTES);

        let (tx, rx) = channel();
        let mut watcher = Self::watcher_with_wake(wake, tx);
        if let Some(ref mut w) = watcher {
            let _ = w.watch(&path_buf, RecursiveMode::NonRecursive);
        }

        let current_file = Some(path_buf.clone());
        let opened_empty = file_size == 0;
        let mut engine = Self::assemble(
            path_buf,
            source,
            file_size,
            last_modified,
            detected_encoding,
            view_mode,
            watcher,
            rx,
            job_threshold_bytes,
            index_job_threshold_bytes,
        );
        engine.current_file = current_file;
        engine.encoding_pending = opened_empty;
        Ok(engine)
    }

    /// Opens a pattern stream: `dir/glob` where `glob` holds `*` / `?` wildcards. The
    /// stream tails the newest matching file and switches when a newer one appears; while
    /// nothing matches it is empty and picks up the first file that does.
    pub fn open_pattern<P: AsRef<Path>>(pattern_path: P) -> Result<Self, std::io::Error> {
        Self::open_pattern_with_thresholds(
            pattern_path,
            JOB_THRESHOLD_BYTES,
            INDEX_JOB_THRESHOLD_BYTES,
        )
    }

    /// Like `open_pattern`, with a filesystem-watcher wake callback (see `open_with_wake`).
    pub fn open_pattern_with_wake<P: AsRef<Path>>(
        pattern_path: P,
        wake: WakeFn,
    ) -> Result<Self, std::io::Error> {
        Self::open_pattern_impl(
            pattern_path,
            JOB_THRESHOLD_BYTES,
            INDEX_JOB_THRESHOLD_BYTES,
            Some(wake),
        )
    }

    /// Like `open_pattern`, with explicit background-job thresholds (see `open_with_thresholds`).
    pub fn open_pattern_with_thresholds<P: AsRef<Path>>(
        pattern_path: P,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
    ) -> Result<Self, std::io::Error> {
        Self::open_pattern_impl(
            pattern_path,
            job_threshold_bytes,
            index_job_threshold_bytes,
            None,
        )
    }

    fn open_pattern_impl<P: AsRef<Path>>(
        pattern_path: P,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
        wake: Option<WakeFn>,
    ) -> Result<Self, std::io::Error> {
        let pattern_path = pattern_path.as_ref().to_path_buf();
        let Some((dir, glob)) = split_pattern(&pattern_path) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Not a file-name pattern",
            ));
        };
        if !dir.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Pattern directory does not exist",
            ));
        }

        let (tx, rx) = channel();
        let mut watcher = Self::watcher_with_wake(wake, tx);
        if let Some(ref mut w) = watcher {
            let _ = w.watch(&dir, RecursiveMode::NonRecursive);
        }

        let mut engine = Self::assemble(
            pattern_path,
            FileSource::empty(),
            0,
            None,
            FileEncoding::Utf8,
            ViewMode::Text,
            watcher,
            rx,
            job_threshold_bytes,
            index_job_threshold_bytes,
        );
        engine.pattern = Some(glob.clone());
        if let Some(newest) = resolve_newest(&dir, &glob) {
            engine.switch_to(newest)?;
            engine.switch_notice = None;
        }
        Ok(engine)
    }

    /// Builds a filesystem watcher that forwards events to the channel and, when `wake`
    /// is set, calls it from the watcher thread as soon as an event arrives.
    fn watcher_with_wake(
        wake: Option<WakeFn>,
        tx: std::sync::mpsc::Sender<notify::Result<Event>>,
    ) -> Option<RecommendedWatcher> {
        match wake {
            Some(wake) => RecommendedWatcher::new(
                move |res: notify::Result<Event>| {
                    let _ = tx.send(res);
                    wake();
                },
                notify::Config::default(),
            )
            .ok(),
            None => RecommendedWatcher::new(tx, notify::Config::default()).ok(),
        }
    }

    /// True for a stream opened from a file-name pattern.
    pub fn is_pattern(&self) -> bool {
        self.pattern.is_some()
    }

    /// Name of the file currently tailed (the resolved file of a pattern stream), if any.
    pub fn current_file_name(&self) -> Option<String> {
        self.current_file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
    }

    /// The switch notice while it is younger than 5 seconds.
    pub fn active_switch_notice(&self) -> Option<&str> {
        match &self.switch_notice {
            Some((name, at)) if at.elapsed() < SWITCH_NOTICE_DURATION => Some(name.as_str()),
            _ => None,
        }
    }

    /// Rescans the pattern directory and switches to the newest match when it differs
    /// from the file being tailed. Returns true when a switch happened.
    pub fn rescan_pattern(&mut self) -> bool {
        self.last_pattern_scan = Instant::now();
        let Some(glob) = self.pattern.clone() else {
            return false;
        };
        let dir = match self.path.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let Some(newest) = resolve_newest(&dir, &glob) else {
            // Nothing matches (any more): keep the current file, if there is one.
            return false;
        };
        if self.current_file.as_ref() == Some(&newest) {
            return false;
        }
        self.switch_to(newest).is_ok()
    }

    /// Binds the stream to another file. Filters, highlight rules, search query, wrap and
    /// encoding are kept; the buffer, line index, per-line caches, bookmarks, selection and
    /// unseen counters start over. No sound is played.
    pub fn switch_to(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Target path is not a regular file",
            ));
        }
        let source = FileSource::open(&path)?;
        if self.current_file.is_none() {
            // First file of an empty pattern stream: nothing was detected yet.
            let sample = source.read_to_vec(0, 512);
            let (encoding, is_binary) = Self::detect_encoding(&sample);
            self.encoding = encoding;
            if is_binary {
                self.view_mode = ViewMode::Hex;
            }
        }
        if self.view_mode == ViewMode::Markdown && metadata.len() > self.markdown_max_bytes {
            self.view_mode = ViewMode::Text;
        }
        self.file_size = metadata.len();
        self.last_modified = metadata.modified().ok();
        self.source = source;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.current_file = Some(path);

        self.max_line_bytes = 0;
        self.max_detected_width = 0.0;
        self.expanded_json_lines.clear();
        self.markdown_text_cache = None;
        self.unseen_lines = 0;
        self.unseen_severity = 0;
        self.has_new_data = true;
        self.scroll_to_line = None;
        self.requested_scroll_x = None;
        self.requested_scroll_y = None;
        self.current_scroll_y = 0.0;
        self.wrap_dirty = true;
        self.wrap_anchor = WrapAnchor::TOP;
        self.wrap_virtual_offset = None;
        self.wrap_request = None;
        self.wrap_at_bottom = true;
        self.bytes_read_since_tick = 0;
        self.ansi_head_end = self.source.len();
        // Drops selection, bookmarks, jobs and per-line caches; re-applies filters and search.
        self.rebuild_line_index();
        self.update_fingerprints();
        self.switch_notice = Some((name, Instant::now()));
        Ok(())
    }

    /// Runs the encoding and binary detection again on the first bytes of the file; a
    /// binary file switches to the HEX view (the Markdown view was already chosen from
    /// the name when the file was opened).
    fn redetect_encoding(&mut self) {
        let sample = self.source.read_to_vec(0, ENCODING_SAMPLE_BYTES as usize);
        let (encoding, is_binary) = Self::detect_encoding(&sample);
        self.encoding = encoding;
        if is_binary {
            self.view_mode = ViewMode::Hex;
        }
    }

    /// Settles a detection still pending on a stream that will not grow any more (a
    /// finished decompression under the sample size), rebuilding the index when the
    /// encoding changes.
    pub fn finish_encoding_detection(&mut self) {
        if !self.encoding_pending {
            return;
        }
        self.encoding_pending = false;
        if self.file_size == 0 {
            return;
        }
        let before = (self.encoding, self.view_mode);
        self.redetect_encoding();
        if (self.encoding, self.view_mode) != before {
            self.rebuild_line_index();
        }
    }

    /// Encoding detection on the first bytes of a file: BOMs, UTF-16 without BOM, and a
    /// NUL byte marking a binary file (shown in HEX).
    fn detect_encoding(sample: &[u8]) -> (FileEncoding, bool) {
        if sample.is_empty() {
            return (FileEncoding::Utf8, false);
        }
        if sample.starts_with(&[0xEF, 0xBB, 0xBF]) {
            (FileEncoding::Utf8, false)
        } else if sample.starts_with(&[0xFF, 0xFE]) {
            (FileEncoding::UnicodeLe, false)
        } else if sample.starts_with(&[0xFE, 0xFF]) {
            (FileEncoding::UnicodeBe, false)
        } else if sample.len() >= 4
            && sample.iter().step_by(2).all(|&b| b != 0)
            && sample.iter().skip(1).step_by(2).all(|&b| b == 0)
        {
            (FileEncoding::UnicodeLe, false)
        } else if sample.len() >= 4
            && sample.iter().step_by(2).all(|&b| b == 0)
            && sample.iter().skip(1).step_by(2).all(|&b| b != 0)
        {
            (FileEncoding::UnicodeBe, false)
        } else if sample.contains(&0) {
            (FileEncoding::Utf8, true)
        } else {
            (FileEncoding::Utf8, false)
        }
    }

    fn initial_view_mode(
        path: &Path,
        is_binary: bool,
        file_size: u64,
        max_markdown_bytes: u64,
    ) -> ViewMode {
        let is_markdown = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
            .unwrap_or(false);
        if is_binary {
            ViewMode::Hex
        } else if is_markdown && file_size <= max_markdown_bytes {
            ViewMode::Markdown
        } else {
            ViewMode::Text
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        path_buf: PathBuf,
        source: FileSource,
        file_size: u64,
        last_modified: Option<std::time::SystemTime>,
        detected_encoding: FileEncoding,
        view_mode: ViewMode,
        watcher: Option<RecommendedWatcher>,
        rx: Receiver<notify::Result<Event>>,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
    ) -> Self {
        let mut engine = Self {
            path: path_buf,
            pattern: None,
            current_file: None,
            pattern_scan_interval: PATTERN_SCAN_INTERVAL,
            last_pattern_scan: Instant::now(),
            size_check_interval: Duration::ZERO,
            last_size_check: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            switch_notice: None,
            source,
            head_fingerprint: Vec::new(),
            tail_fingerprint: (0, Vec::new()),
            line_offsets: Vec::new(),
            file_size,
            last_modified,
            follow_tail: true,
            selection: BTreeSet::new(),
            selection_all: false,
            selection_anchor: None,
            goto_open: false,
            goto_input: String::new(),
            goto_notice: None,
            view_notice: None,
            bookmarks: BTreeSet::new(),
            bookmark_cursor: None,
            bookmarks_dirty: false,
            bookmarks_generation: 0,
            wrap_lines: false,
            wrap_dirty: false,
            wrap_anchor: WrapAnchor::TOP,
            wrap_virtual_offset: None,
            wrap_scroll_delta: 0.0,
            wrap_scroll_abs: 0.0,
            wrap_avg_row_height: 0.0,
            wrap_at_bottom: true,
            wrap_request: None,
            displayed: false,
            unseen_lines: 0,
            unseen_severity: 0,
            is_watching: true,
            has_new_data: false,
            view_mode,
            hex_columns: 16,
            size_unit: SizeUnit::Bytes,
            encoding: detected_encoding,
            include_terms: Vec::new(),
            exclude_terms: Vec::new(),
            applied_preset: None,
            filter_case_sensitive: false,
            filter_is_regex: false,
            time_from: None,
            time_to: None,
            time_from_text: String::new(),
            time_to_text: String::new(),
            time_range_error: false,
            min_level: LogLevel::Unknown,
            show_unknown_levels: false,
            levels: Vec::new(),
            timestamps: Vec::new(),
            timestamp_hint: crate::timestamp::FormatHint::default(),
            timestamps_parsed: 0,
            timestamps_unordered: false,
            timestamps_wanted: false,
            time_anchor: None,
            pending_window: None,
            pending_goto_time: None,
            goto_time_result: None,
            level_counts: [0; LogLevel::COUNT],
            error_blocks: Vec::new(),
            histogram: TimeHistogram::default(),
            histogram_len: 0,
            histogram_generation: 0,
            filter: FilterSpec::default(),
            job: None,
            job_generation: 0,
            pending_refresh_from: None,
            pending_filter: false,
            pending_search: false,
            index_pending: false,
            job_threshold_bytes,
            index_job_threshold_bytes,
            filtered_lines: Vec::new(),
            filter_generation: 0,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_total: 0,
            search_last_counted: 0,
            search_last_counted_exact: false,
            search_generation: 0,
            search_byte_matches: Vec::new(),
            search_byte_max_len: 0,
            current_match_idx: None,
            last_searched_query: String::new(),
            scroll_to_byte: None,
            search_edited_at: None,
            buffer_generation: 0,
            reload_generation: 0,
            pending_jump: None,
            find_all_request: false,
            markdown_text_cache: None,
            highlight_rules: Vec::new(),
            compiled_highlights: Vec::new(),
            quick_labels: Vec::new(),
            expanded_json_lines: HashSet::new(),
            requested_scroll_x: None,
            requested_scroll_y: None,
            scroll_to_line: None,
            markdown_cache: egui_commonmark::CommonMarkCache::default(),
            markdown_max_bytes: MARKDOWN_MAX_BYTES,
            current_scroll_x: 0.0,
            current_scroll_y: 0.0,
            max_line_bytes: 0,
            max_detected_width: 0.0,
            last_sound_alert_time: Instant::now(),
            tool_bound_rules: HashSet::new(),
            pending_tool_hits: Vec::new(),
            _watcher: watcher,
            rx,
            last_read_time: Instant::now(),
            bytes_read_since_tick: 0,
            throughput_bps: 0.0,
            encoding_pending: false,
            compressed: None,
            stdin: None,
            ansi_mode: AnsiMode::Auto,
            ansi_dirty: false,
            ansi_detected: false,
            ansi_switched_at: None,
            ansi_head_end: 0,
        };

        engine.ansi_head_end = engine.source.len();
        engine.rebuild_line_index();
        engine.update_fingerprints();
        engine
    }

    /// Raw bytes `[offset, offset + len)` for the HEX view, read through the block cache.
    pub fn get_bytes(&self, offset: usize, len: usize) -> Option<Vec<u8>> {
        if offset as u64 >= self.source.len() {
            return None;
        }
        Some(self.source.read_to_vec(offset as u64, len))
    }

    pub fn total_hex_rows(&self, bytes_per_row: usize) -> usize {
        if bytes_per_row == 0 || self.file_size == 0 {
            return 0;
        }
        (self.file_size as usize).div_ceil(bytes_per_row)
    }

    pub fn next_size_unit(&mut self) {
        self.size_unit = match self.size_unit {
            SizeUnit::Bytes => SizeUnit::MB,
            SizeUnit::MB => SizeUnit::GB,
            SizeUnit::GB => SizeUnit::Hex,
            SizeUnit::Hex => SizeUnit::Bytes,
        };
    }

    pub fn format_size(&self) -> String {
        match self.size_unit {
            SizeUnit::Bytes => format!("{} B", self.file_size),
            SizeUnit::MB => format!("{:.2} MB", self.file_size as f64 / (1024.0 * 1024.0)),
            SizeUnit::GB => format!(
                "{:.3} GB",
                self.file_size as f64 / (1024.0 * 1024.0 * 1024.0)
            ),
            SizeUnit::Hex => format!("0x{:X}", self.file_size),
        }
    }

    pub fn set_highlight_rules(&mut self, rules: Vec<HighlightRule>) {
        self.compiled_highlights = rules
            .iter()
            .map(|r| {
                let regex = if r.is_regex {
                    regex::RegexBuilder::new(&r.pattern)
                        .case_insensitive(!r.case_sensitive)
                        .build()
                        .ok()
                } else {
                    None
                };
                let pattern_lower = r.pattern.to_lowercase();
                let style = r.style();
                CompiledHighlight {
                    regex,
                    pattern_lower,
                    style,
                    enabled: r.enabled,
                    captures_only: r.captures_only,
                    case_sensitive: r.case_sensitive,
                    pattern: r.pattern.clone(),
                    sound_alert: r.sound_alert,
                }
            })
            .collect();
        self.highlight_rules = rules;
        self.recompute_filtered_lines();
    }

    pub fn refresh_filters(&mut self) {
        self.filter = self.build_filter();
        self.recompute_filtered_lines();
    }

    fn build_filter(&self) -> FilterSpec {
        FilterSpec::build(
            &self.include_terms,
            &self.exclude_terms,
            self.filter_case_sensitive,
            self.filter_is_regex,
        )
        .with_levels(self.min_level, self.show_unknown_levels)
    }

    /// Sets the minimum level a line must have to be visible (`Unknown` turns it off).
    pub fn set_min_level(&mut self, level: LogLevel) {
        if self.min_level != level {
            self.min_level = level;
            self.refresh_filters();
        }
    }

    pub fn set_show_unknown_levels(&mut self, show: bool) {
        if self.show_unknown_levels != show {
            self.show_unknown_levels = show;
            self.refresh_filters();
        }
    }

    /// Level of line `idx`: from the cache when it has been reached, detected on the fly
    /// otherwise (the cache stays a prefix of the index).
    pub fn level_of(&self, idx: usize) -> LogLevel {
        match self.levels.get(idx) {
            Some(&v) => LogLevel::from_u8(v),
            None => self
                .get_line(idx)
                .map(|line| detect_level(&line))
                .unwrap_or(LogLevel::Unknown),
        }
    }

    pub fn level_count(&self, level: LogLevel) -> u64 {
        self.level_counts[level as usize]
    }

    /// True once every indexed line has its level cached (the counters are complete).
    pub fn levels_complete(&self) -> bool {
        self.levels.len() >= self.line_offsets.len()
    }

    fn push_levels(&mut self, fresh: &[u8]) {
        for (idx, &v) in (self.levels.len()..).zip(fresh) {
            self.level_counts[(v as usize).min(LogLevel::COUNT - 1)] += 1;
            let block = idx / ERROR_BLOCK_LINES;
            if self.error_blocks.len() <= block {
                self.error_blocks.resize(block + 1, 0);
            }
            if is_error_level(v) {
                self.error_blocks[block] += 1;
            }
        }
        self.levels.extend_from_slice(fresh);
        self.feed_histogram();
    }

    /// Adds to the histogram the lines that now have both a level and a timestamp. Called
    /// whenever either cache grows, so each line is counted once, right after the work
    /// that produced the second of the two.
    fn feed_histogram(&mut self) {
        let end = self.levels.len().min(self.timestamps.len());
        if end <= self.histogram_len {
            return;
        }
        for idx in self.histogram_len..end {
            self.histogram.add(self.timestamps[idx], self.levels[idx]);
        }
        self.histogram_len = end;
        self.histogram_generation = self.histogram_generation.wrapping_add(1);
    }

    /// Takes the lines `>= keep` out of the histogram, before either cache drops them
    /// (their level and time are still cached). `keep == 0` starts over at one-second
    /// buckets; otherwise the width stays.
    fn truncate_histogram(&mut self, keep: usize) {
        if keep >= self.histogram_len {
            return;
        }
        if keep == 0 {
            self.histogram.reset();
        } else {
            for idx in keep..self.histogram_len {
                self.histogram
                    .remove(self.timestamps[idx], self.levels[idx]);
            }
        }
        self.histogram_len = keep;
        self.histogram_generation = self.histogram_generation.wrapping_add(1);
    }

    /// Lines per level over time of every timed line of the stream, whatever the filters.
    pub fn time_histogram(&self) -> &TimeHistogram {
        &self.histogram
    }

    /// Lines counted by the histogram so far (timed and with a level).
    pub fn histogram_lines(&self) -> usize {
        self.histogram_len
    }

    /// The timeline histogram wants the stream timed: starts timing (in the background for
    /// a large file) unless it is complete or already on its way.
    pub fn request_timeline(&mut self) {
        if self.timestamps_complete() || self.timestamps_wanted {
            return;
        }
        self.request_timestamps();
    }

    /// Detected levels (`LogLevel as u8`) of the cached prefix of the lines.
    pub fn cached_levels(&self) -> &[u8] {
        &self.levels
    }

    /// ERROR and FATAL lines per block of `ERROR_BLOCK_LINES` lines of the cached prefix.
    pub fn error_block_counts(&self) -> &[u32] {
        &self.error_blocks
    }

    /// ERROR and FATAL lines among `lines`, counting only lines whose level is cached:
    /// whole blocks from the block counts, the partial ends from the level cache.
    pub fn error_lines_in(&self, lines: std::ops::Range<usize>) -> u64 {
        let end = lines.end.min(self.levels.len());
        let mut at = lines.start.min(end);
        let mut count = 0u64;
        while at < end {
            let block = at / ERROR_BLOCK_LINES;
            let block_start = block * ERROR_BLOCK_LINES;
            let block_end = block_start + ERROR_BLOCK_LINES;
            if at == block_start && block_end <= end {
                count += u64::from(self.error_blocks[block]);
                at = block_end;
            } else {
                let stop = block_end.min(end);
                count += self.levels[at..stop]
                    .iter()
                    .filter(|&&v| is_error_level(v))
                    .count() as u64;
                at = stop;
            }
        }
        count
    }

    /// Whether every indexed line has been timed.
    pub fn timestamps_complete(&self) -> bool {
        self.timestamps.len() >= self.line_offsets.len()
    }

    /// Effective timestamp of a line: its own, or the one it inherits from the entry it
    /// continues. `None` when the line has not been timed yet or precedes the first time.
    pub fn line_timestamp(&self, idx: usize) -> Option<i64> {
        match self.timestamps.get(idx) {
            Some(&NO_TIMESTAMP) | None => None,
            Some(&millis) => Some(millis),
        }
    }

    /// Share of timed lines that carried a timestamp of their own. `None` until enough
    /// lines have been timed to judge.
    pub fn timestamp_rate(&self) -> Option<f32> {
        let timed = self.timestamps.len();
        if timed < TIMESTAMP_RATE_SAMPLE.min(self.line_offsets.len().max(1)) {
            return None;
        }
        (timed > 0).then(|| self.timestamps_parsed as f32 / timed as f32)
    }

    /// Share of timed lines a window can place in time: those with a timestamp of their
    /// own or inherited from the entry they continue (a stack trace line). Only the lines
    /// before the first timestamp have none, and they form a prefix of the cache, so the
    /// count is a binary search. `None` until enough lines have been timed to judge.
    pub fn timestamp_coverage(&self) -> Option<f32> {
        let timed = self.timestamps.len();
        if timed < TIMESTAMP_RATE_SAMPLE.min(self.line_offsets.len().max(1)) {
            return None;
        }
        let untimed = if self.timestamps_parsed == 0 {
            timed
        } else {
            self.timestamps.partition_point(|&ts| ts == NO_TIMESTAMP)
        };
        (timed > 0).then(|| (timed - untimed) as f32 / timed as f32)
    }

    /// Whether a time window is currently narrowing the view.
    pub fn is_time_filtered(&self) -> bool {
        self.time_from.is_some() || self.time_to.is_some()
    }

    /// Whether the time range and the time jump can work on this stream at all: most of
    /// its lines can be placed in time. A log made mostly of stack traces qualifies, since
    /// every trace line inherits the timestamp of its entry.
    pub fn timestamps_usable(&self) -> bool {
        self.timestamp_coverage()
            .map(|rate| rate >= MIN_TIMESTAMP_RATE)
            .unwrap_or(false)
    }

    /// The timestamp cache as it stands: the effective timestamps of the timed prefix, how
    /// many of those lines carried a timestamp of their own, the out-of-order flag and the
    /// format that matched last. For the tests and the benchmark, which compare the
    /// background scan with the synchronous fill.
    #[doc(hidden)]
    pub fn timestamp_cache(&self) -> (&[i64], usize, bool, crate::timestamp::FormatHint) {
        (
            &self.timestamps,
            self.timestamps_parsed,
            self.timestamps_unordered,
            self.timestamp_hint,
        )
    }

    /// Times the next chunk of untimed lines and reports whether the cache is complete.
    /// The synchronous path; `scan_job`'s `Timestamps` scan applies the same rules on a
    /// worker thread when a large part of the file is still untimed.
    pub fn fill_timestamps(&mut self) -> bool {
        let from = self.timestamps.len();
        let total = self.total_lines();
        if from >= total {
            return true;
        }
        let to = (from + TIMESTAMP_FILL_BUDGET).min(total);
        let mut inherited = if from == 0 {
            NO_TIMESTAMP
        } else {
            self.timestamps[from - 1]
        };
        let mut hint = self.timestamp_hint;
        let mut parsed = 0usize;
        let mut unordered = false;
        let mut fresh = Vec::with_capacity(to - from);
        self.scan_lines(from, to, |_, text| {
            match crate::timestamp::detect_timestamp(text, hint) {
                Some((millis, format)) => {
                    hint = format;
                    parsed += 1;
                    if inherited != NO_TIMESTAMP && millis < inherited {
                        unordered = true;
                    }
                    inherited = millis;
                }
                // No timestamp of its own: it belongs to the entry above it.
                None => {}
            }
            fresh.push(inherited);
            true
        });
        self.timestamps.extend_from_slice(&fresh);
        self.timestamp_hint = hint;
        self.timestamps_parsed += parsed;
        self.timestamps_unordered |= unordered;
        self.feed_histogram();
        self.timestamps.len() >= self.total_lines()
    }

    /// Forgets the cached timestamps of lines `>= keep` (the index changed from there).
    fn truncate_timestamps(&mut self, keep: usize) {
        self.truncate_histogram(keep);
        if self.timestamps.len() <= keep {
            return;
        }
        // The parsed count cannot be corrected line by line without re-reading them, and a
        // rewritten tail is small compared to the file: start the rate over from what is
        // left rather than carry a wrong one.
        self.timestamps.truncate(keep);
        self.timestamps_parsed = self.timestamps_parsed.min(keep);
        if keep == 0 {
            self.timestamp_hint = crate::timestamp::FormatHint::default();
            self.timestamps_unordered = false;
        }
    }

    /// Forgets the cached levels of lines `>= keep` (the index changed from there).
    fn truncate_levels(&mut self, keep: usize) {
        self.truncate_histogram(keep);
        if self.levels.len() <= keep {
            return;
        }
        for (idx, &v) in self.levels.iter().enumerate().skip(keep) {
            let slot = &mut self.level_counts[(v as usize).min(LogLevel::COUNT - 1)];
            *slot = slot.saturating_sub(1);
            if is_error_level(v) {
                let block = &mut self.error_blocks[idx / ERROR_BLOCK_LINES];
                *block = block.saturating_sub(1);
            }
        }
        self.levels.truncate(keep);
        self.error_blocks.truncate(keep.div_ceil(ERROR_BLOCK_LINES));
    }

    /// Detects the levels of the lines not cached yet: synchronously when the remaining
    /// bytes are below the job threshold, otherwise on a worker thread once no other
    /// scan is running (a `Levels` job has the lowest priority).
    fn ensure_levels(&mut self) {
        if self.index_pending {
            return;
        }
        let from = self.levels.len();
        let total = self.total_lines();
        if from >= total {
            return;
        }
        let remaining = self.source.len().saturating_sub(self.line_offsets[from]);
        if remaining > self.job_threshold_bytes {
            if self.job.is_none() {
                self.start_job(JobSpec::Levels, from);
            }
            return;
        }
        let mut fresh = Vec::with_capacity(total - from);
        self.scan_lines(from, total, |_, text| {
            fresh.push(detect_level(text) as u8);
            true
        });
        self.push_levels(&fresh);
    }

    /// First include term: the stream bar's include field and the `--filter` option.
    pub fn include_filter(&self) -> &str {
        self.include_terms.first().map_or("", String::as_str)
    }

    /// First exclude term: the stream bar's exclude field and the `--exclude` option.
    pub fn exclude_filter(&self) -> &str {
        self.exclude_terms.first().map_or("", String::as_str)
    }

    /// Every include term row, empty ones included.
    pub fn include_terms(&self) -> &[String] {
        &self.include_terms
    }

    pub fn exclude_terms(&self) -> &[String] {
        &self.exclude_terms
    }

    /// Replaces the first include term, keeping the others.
    pub fn set_include_filter(&mut self, filter: &str) {
        set_first_term(&mut self.include_terms, filter);
        self.refresh_filters();
    }

    pub fn set_exclude_filter(&mut self, filter: &str) {
        set_first_term(&mut self.exclude_terms, filter);
        self.refresh_filters();
    }

    /// Replaces both term lists (capped at `MAX_FILTER_TERMS` each) in one recomputation.
    /// Rows added or removed empty change nothing visible: they skip the recomputation,
    /// which on a large file is a background scan.
    pub fn set_filter_terms(&mut self, mut include: Vec<String>, mut exclude: Vec<String>) {
        include.truncate(MAX_FILTER_TERMS);
        exclude.truncate(MAX_FILTER_TERMS);
        let non_empty =
            |v: &[String]| -> Vec<String> { v.iter().filter(|t| !t.is_empty()).cloned().collect() };
        let same = non_empty(&include) == non_empty(&self.include_terms)
            && non_empty(&exclude) == non_empty(&self.exclude_terms);
        self.include_terms = include;
        self.exclude_terms = exclude;
        if same {
            // Same filter, rows moved: only the per-row error flags need the new order.
            self.filter = self.build_filter();
        } else {
            self.refresh_filters();
        }
    }

    /// Non-empty include terms after the first: the `+N` badge of the include field.
    pub fn extra_include_terms(&self) -> usize {
        non_empty_after_first(&self.include_terms)
    }

    pub fn extra_exclude_terms(&self) -> usize {
        non_empty_after_first(&self.exclude_terms)
    }

    /// Whether include term `i` is a regex that does not compile (it matches nothing).
    pub fn include_term_invalid(&self, i: usize) -> bool {
        self.filter.include.get(i).is_some_and(|t| t.invalid)
    }

    pub fn exclude_term_invalid(&self, i: usize) -> bool {
        self.filter.exclude.get(i).is_some_and(|t| t.invalid)
    }

    /// Replaces the terms, the case and regex toggles and the level stage of the filter
    /// in one recomputation, and with `time` the time range as typed (a preset without
    /// one leaves the window as it is). Returns nothing: a time text that does not parse
    /// sets `time_range_error`, as typing it would.
    pub fn set_filter_state(&mut self, state: &crate::filter_preset::FilterState) {
        let mut include = state.include.clone();
        let mut exclude = state.exclude.clone();
        include.truncate(MAX_FILTER_TERMS);
        exclude.truncate(MAX_FILTER_TERMS);
        self.include_terms = include;
        self.exclude_terms = exclude;
        self.filter_case_sensitive = state.case_sensitive;
        self.filter_is_regex = state.is_regex;
        self.min_level = state.min_level;
        self.show_unknown_levels = state.show_unknown_levels;
        let generation = self.filter_generation;
        if let Some((from, to)) = &state.time {
            // A window that changes refreshes the filter itself, fields above included.
            let (from_ok, to_ok) = self.apply_time_range_text(from, to);
            self.time_range_error = !from_ok || !to_ok;
        }
        if self.filter_generation == generation {
            self.refresh_filters();
        }
    }

    pub fn set_encoding(&mut self, encoding: FileEncoding) {
        self.encoding = encoding;
        self.encoding_pending = false;
        // Read from the start again: only the head is sampled for colour codes.
        self.ansi_head_end = self.source.len();
        self.rebuild_line_index();
    }

    pub fn rebuild_line_index(&mut self) {
        // The file was truncated, rewritten or re-decoded: row indices no longer mean the same.
        self.reload_generation = self.reload_generation.wrapping_add(1);
        self.job = None;
        self.index_pending = false;
        self.pending_refresh_from = None;
        self.clear_selection();
        self.clear_bookmarks();
        self.time_anchor = None;
        self.rebuild_line_index_from(0);
    }

    /// Rebuilds the line offsets and refreshes the derived state (filter visibility,
    /// search matches). Lines before `unchanged_lines` are known to be identical to the
    /// previous index, so their derived state is kept instead of being rescanned.
    fn rebuild_line_index_from(&mut self, unchanged_lines: usize) {
        let mut total_len = self.source.len();
        // Auto mode looks for colour codes in the head of the file, then in each append
        // until it finds one. Switching to render on appended data changes the text of
        // every line: their derived state is rebuilt from the start (the index itself
        // stays incremental).
        let examined_from = if unchanged_lines == 0 {
            self.bom_len()
        } else {
            self.line_offsets
                .get(unchanged_lines)
                .copied()
                .unwrap_or(total_len)
        };
        let derived_from = if self.detect_ansi(examined_from) && unchanged_lines > 0 {
            self.job = None;
            self.pending_refresh_from = None;
            self.markdown_text_cache = None;
            self.ansi_switched_at = Some(Instant::now());
            0
        } else {
            unchanged_lines
        };
        // The level and timestamp caches follow the index: drop what a running level or
        // timestamp scan would push out of order, and forget the lines that are about to
        // be rescanned. Both scans resume from their prefix afterwards.
        if self
            .job
            .as_ref()
            .map(|j| matches!(j.kind, ScanKind::Levels | ScanKind::Timestamps))
            .unwrap_or(false)
        {
            self.job = None;
        }
        self.truncate_levels(derived_from);
        self.truncate_timestamps(derived_from);
        if derived_from == 0 {
            self.hold_time_window();
        }
        if unchanged_lines == 0 && total_len > self.index_job_threshold_bytes {
            // Large file: index on a worker thread; the view shows lines as they arrive.
            self.line_offsets = Vec::new();
            self.filtered_lines = Vec::new();
            self.filter_generation = self.filter_generation.wrapping_add(1);
            self.clear_search_hits();
            self.search_byte_matches = Vec::new();
            self.current_match_idx = None;
            self.max_line_bytes = 0;
            self.buffer_generation = self.buffer_generation.wrapping_add(1);
            self.scroll_to_line = None;
            self.index_pending = true;
            self.pending_filter = self.filter.is_active();
            self.pending_search = !self.last_searched_query.is_empty();
            self.start_job(JobSpec::Index, 0);
            return;
        }
        if total_len == 0 {
            self.line_offsets = Vec::new();
            self.max_line_bytes = 0;
            self.max_detected_width = self.max_detected_width.max(120.0);
            self.buffer_generation = self.buffer_generation.wrapping_add(1);
            self.scroll_to_line = None;
            if self.wants_timestamps() {
                // Nothing to time: a held window applies at once, over no lines.
                self.request_timestamps();
            }
            self.refresh_derived_state_from(0);
            return;
        }

        let bom_len = self.bom_len();
        let char_bytes: u64 = match self.encoding {
            FileEncoding::UnicodeLe | FileEncoding::UnicodeBe => 2,
            _ => 1,
        };

        // Incremental scan: keep the offsets of the lines known to be unchanged and rescan
        // only from the start of the first changed line (the previously last, maybe partial,
        // line). The bytes are streamed in chunks straight from the file and never kept.
        let (scan_from, mut max_bytes) =
            if unchanged_lines > 0 && unchanged_lines < self.line_offsets.len() {
                let from = self.line_offsets[unchanged_lines];
                self.line_offsets.truncate(unchanged_lines);
                (from.min(total_len), self.max_line_bytes)
            } else {
                self.line_offsets = Vec::new();
                (bom_len.min(total_len), 0usize)
            };

        if scan_from < total_len {
            self.line_offsets.push(scan_from);
        }
        let mut prev_offset = scan_from;
        let mut chunk = vec![0u8; SCAN_CHUNK];
        let mut pos = scan_from;
        while pos < total_len {
            let want = ((total_len - pos) as usize).min(chunk.len());
            let n = match self.source.read_direct(pos, &mut chunk[..want]) {
                Ok(n) if n > 0 => n,
                _ => break,
            };
            match self.encoding {
                FileEncoding::Utf8 | FileEncoding::Ascii | FileEncoding::Ansi => {
                    for rel in memchr::memchr_iter(b'\n', &chunk[..n]) {
                        let i = pos + rel as u64;
                        let len = (i - prev_offset) as usize;
                        if len > max_bytes {
                            max_bytes = len;
                        }
                        if i + 1 < total_len {
                            self.line_offsets.push(i + 1);
                            prev_offset = i + 1;
                        }
                    }
                    pos += n as u64;
                }
                FileEncoding::UnicodeLe | FileEncoding::UnicodeBe => {
                    let (lo, hi) = if self.encoding == FileEncoding::UnicodeLe {
                        (0x0A, 0x00)
                    } else {
                        (0x00, 0x0A)
                    };
                    let n_even = n & !1;
                    if n_even == 0 {
                        break;
                    }
                    let mut k = 0;
                    while k + 1 < n_even {
                        if chunk[k] == lo && chunk[k + 1] == hi {
                            let i = pos + k as u64;
                            let len = ((i - prev_offset) / 2) as usize;
                            if len > max_bytes {
                                max_bytes = len;
                            }
                            if i + 2 < total_len {
                                self.line_offsets.push(i + 2);
                                prev_offset = i + 2;
                            }
                        }
                        k += 2;
                    }
                    pos += n_even as u64;
                }
            }
        }
        if pos < total_len {
            total_len = pos;
            self.file_size = pos;
            self.source.set_len(pos);
            self.line_offsets.retain(|&off| off < total_len);
        }
        // Length of the final (possibly unterminated) line, without its newline.
        let tail = self
            .source
            .read_to_vec(total_len.saturating_sub(char_bytes), char_bytes as usize);
        let ends_with_newline = match self.encoding {
            FileEncoding::UnicodeLe => tail == [0x0A, 0x00],
            FileEncoding::UnicodeBe => tail == [0x00, 0x0A],
            _ => tail == *b"\n",
        };
        let trailing = if ends_with_newline { char_bytes } else { 0 };
        let last_len = (total_len
            .saturating_sub(prev_offset)
            .saturating_sub(trailing)
            / char_bytes) as usize;
        if last_len > max_bytes {
            max_bytes = last_len;
        }
        self.max_line_bytes = max_bytes;
        let estimated_width = (max_bytes as f32) * 8.5 + 120.0;
        self.max_detected_width = self.max_detected_width.max(estimated_width);
        self.buffer_generation = self.buffer_generation.wrapping_add(1);
        if self
            .scroll_to_line
            .map(|l| l >= self.total_lines())
            .unwrap_or(false)
        {
            self.scroll_to_line = None;
        }
        // Time the new lines before refreshing the view: the window filter reads their
        // timestamps. A timed stream stays timed as it grows.
        if self.wants_timestamps() || !self.timestamps.is_empty() {
            self.request_timestamps();
        }
        self.refresh_derived_state_from(derived_from);
        self.ensure_levels();
    }

    /// Every line is about to be timed again, and a window over an empty cache would hide
    /// the whole file: holds the window until timing finishes. The fields are re-read
    /// then, so a rotated log gets the day of its own first line.
    fn hold_time_window(&mut self) {
        if !self.is_time_filtered() {
            return;
        }
        self.pending_window = Some(
            if self.time_from_text.trim().is_empty() && self.time_to_text.trim().is_empty() {
                PendingWindow::Range(self.time_from, self.time_to)
            } else {
                PendingWindow::Texts
            },
        );
        self.time_from = None;
        self.time_to = None;
    }

    // ----- ANSI escape sequences -----

    /// The mode the stream behaves in: `Auto` is render once an SGR sequence was found,
    /// raw until then (identical to the others on a log without escapes).
    pub fn ansi_effective(&self) -> AnsiMode {
        match self.ansi_mode {
            AnsiMode::Auto if self.ansi_detected => AnsiMode::Render,
            AnsiMode::Auto => AnsiMode::Raw,
            mode => mode,
        }
    }

    /// Whether the text features see the lines without their escape sequences.
    fn strips_ansi(&self) -> bool {
        self.ansi_effective().strips()
    }

    /// Whether auto mode switched to render on appended data less than 5 seconds ago.
    pub fn ansi_switch_notice(&self) -> bool {
        self.ansi_switched_at
            .map(|at| at.elapsed() < SWITCH_NOTICE_DURATION)
            .unwrap_or(false)
    }

    /// Chooses how escape sequences are handled. When the resulting behaviour changes,
    /// the text of every line changes with it: see `line_text_changed`.
    pub fn set_ansi_mode(&mut self, mode: AnsiMode) {
        if self.ansi_mode == mode {
            return;
        }
        let before = self.ansi_effective();
        self.ansi_mode = mode;
        self.ansi_dirty = true;
        self.ansi_switched_at = None;
        if mode == AnsiMode::Auto && !self.index_pending {
            // Back to auto: look at the head again, like at open (the whole stream is
            // not read again on a click).
            let head = self.bom_len();
            let head_end = self.ansi_head_end;
            self.ansi_head_end = self.source.len();
            self.detect_ansi(head);
            self.ansi_head_end = head_end;
        }
        if self.ansi_effective() != before {
            self.line_text_changed();
        }
    }

    /// Auto-detection on the bytes from `from`: returns true when it finds the stream's
    /// first SGR sequence. Of the content the stream held when read from the start (before
    /// `ansi_head_end`) at most `DETECT_SAMPLE_BYTES` are sampled, so opening a large file
    /// stays cheap; every appended byte after it is examined, in chunks, however large
    /// the append (a decompressed stream arrives in chunks of a megabyte). A no-op outside
    /// auto mode and once a sequence was found (the switch happens at most once per stream).
    fn detect_ansi(&mut self, from: u64) -> bool {
        if self.ansi_mode != AnsiMode::Auto || self.ansi_detected {
            return false;
        }
        let end = self.source.len();
        if from >= end {
            return false;
        }
        let head_end = self.ansi_head_end.min(end);
        let mut found = false;
        if from < head_end {
            let len = ((head_end - from) as usize).min(crate::ansi::DETECT_SAMPLE_BYTES);
            found = self.sgr_in(&self.source.read_to_vec(from, len));
        }
        if !found && end > head_end.max(from) {
            found = self.sgr_in_range(head_end.max(from), end);
        }
        self.ansi_detected = found;
        found
    }

    /// True when the bytes `[from, end)` hold an SGR sequence. Read straight from the file
    /// in chunks (the block cache is left to the view); the chunks overlap so a sequence
    /// across a boundary is seen. Bytes without an `ESC` cost a `memchr` pass.
    fn sgr_in_range(&self, from: u64, end: u64) -> bool {
        const CHUNK: usize = 256 * 1024;
        const OVERLAP: u64 = 256;
        let mut buf = vec![0u8; CHUNK.min((end - from) as usize)];
        let mut pos = from;
        while pos < end {
            let len = ((end - pos) as usize).min(buf.len());
            let n = match self.source.read_direct(pos, &mut buf[..len]) {
                Ok(n) if n > 0 => n,
                _ => return false,
            };
            if self.sgr_in(&buf[..n]) {
                return true;
            }
            let next = pos + n as u64;
            if next >= end {
                break;
            }
            // Step back by an even amount, so UTF-16 chunks stay aligned.
            pos = if n as u64 > OVERLAP {
                next - OVERLAP
            } else {
                next
            };
        }
        false
    }

    /// True when the raw bytes `bytes`, read at an even offset, hold an SGR sequence.
    fn sgr_in(&self, bytes: &[u8]) -> bool {
        match self.encoding {
            FileEncoding::UnicodeLe | FileEncoding::UnicodeBe => {
                let even = bytes.len() & !1;
                let text = decode_content(&bytes[..even], self.encoding, true);
                crate::ansi::contains_sgr(text.as_bytes())
            }
            _ => crate::ansi::contains_sgr(bytes),
        }
    }

    /// The text of every line changed while the line index did not (the ANSI mode): the
    /// level and timestamp caches start over and the filters and the search run again,
    /// as after a rewrite, without re-reading the line offsets.
    fn line_text_changed(&mut self) {
        self.buffer_generation = self.buffer_generation.wrapping_add(1);
        self.markdown_text_cache = None;
        if self.index_pending {
            // Nothing is derived yet: the index job ends with the refreshes it deferred.
            return;
        }
        let timed = !self.timestamps.is_empty();
        self.job = None;
        self.pending_refresh_from = None;
        self.truncate_levels(0);
        self.truncate_timestamps(0);
        self.hold_time_window();
        if timed || self.wants_timestamps() {
            self.request_timestamps();
        }
        self.refresh_derived_state_from(0);
        self.ensure_levels();
    }

    pub fn poll_updates(&mut self) {
        self.drain_job();
        if !self.is_watching {
            return;
        }
        // A decompression job appended to the spool: index it without waiting for the
        // watcher or the size check.
        let mut needs_refresh = self
            .compressed
            .as_ref()
            .is_some_and(|c| c.has_unindexed(self.file_size));
        // Standard input: the same, and a spool restarted at its limit rebuilds first.
        if self.stdin.is_some() {
            self.poll_stdin_restart();
            needs_refresh |= self
                .stdin
                .as_ref()
                .is_some_and(|s| s.has_unindexed(self.file_size));
        }

        // Drain filesystem watcher events (the file, or the directory of a pattern stream)
        while let Ok(event_res) = self.rx.try_recv() {
            if let Ok(event) = event_res {
                if event.kind.is_modify() || event.kind.is_create() {
                    needs_refresh = true;
                }
            }
        }

        // Pattern stream: look for a newer match on directory activity and every 2 s.
        if self.pattern.is_some()
            && (needs_refresh || self.last_pattern_scan.elapsed() >= self.pattern_scan_interval)
            && self.rescan_pattern()
        {
            // The switch rebuilt everything from the new file.
            needs_refresh = false;
        }

        // Periodic size check (handles network drives, atomic saves, and Windows handle caches)
        let do_size_check = needs_refresh
            || self.size_check_interval.is_zero()
            || (self.is_pattern() && self.pattern_scan_interval.is_zero())
            || self.last_size_check.elapsed() >= self.size_check_interval;
        if do_size_check {
            self.last_size_check = Instant::now();
            if let Some(current) = &self.current_file {
                if let Ok(metadata) = std::fs::metadata(current) {
                    let current_len = metadata.len();
                    if current_len != self.file_size
                        || metadata.modified().ok() != self.last_modified
                        || !self.source.is_same_file_as_path()
                    {
                        needs_refresh = true;
                    }
                }
            } else {
                needs_refresh = false;
            }
        }

        if needs_refresh {
            self.refresh_file();
        }
        if self.compressed.is_some() {
            self.poll_compressed();
        }
        if self.stdin.is_some() {
            self.poll_stdin_end();
        }

        // Update throughput measurement once every 500ms
        let elapsed = self.last_read_time.elapsed().as_secs_f64();
        if elapsed >= 0.5 {
            self.throughput_bps = (self.bytes_read_since_tick as f64) / elapsed;
            self.bytes_read_since_tick = 0;
            self.last_read_time = Instant::now();
        }
    }

    fn refresh_file(&mut self) {
        let Some(current) = self.current_file.clone() else {
            return;
        };
        let Ok(metadata) = std::fs::metadata(&current) else {
            return;
        };
        let new_size = metadata.len();
        let new_modified = metadata.modified().ok();

        if new_size < self.file_size {
            // Truncated or rotated: start over and give the memory back.
            self.reload_from_start(new_size, new_modified);
            return;
        }

        if new_size > self.file_size {
            // If the underlying handle points to a replaced file (e.g. atomic save by an editor)
            // or does not yet reflect the new size, reopen to obtain the live file handle.
            if !self.source.is_same_file_as_path()
                || self.source.handle_len().unwrap_or(0) < new_size
            {
                let _ = self.source.reopen();
            }

            let prev_lines_count = self.line_offsets.len();
            let added_bytes = new_size - self.file_size;
            self.bytes_read_since_tick += added_bytes;

            // Opened empty: the first sample decides the encoding and the view; the index
            // is rebuilt when they change, else the append goes on as usual.
            if self.encoding_pending && new_size >= ENCODING_SAMPLE_BYTES {
                self.encoding_pending = false;
                self.source.set_len(new_size);
                let before = (self.encoding, self.view_mode);
                self.redetect_encoding();
                if (self.encoding, self.view_mode) != before {
                    // The bytes are the appended ones, read in the right encoding: all of
                    // them are still examined for colour codes.
                    self.redecode_from_start(new_size, new_modified);
                    return;
                }
            }

            // A file reset and regrown past its old size can keep the same header (same log
            // format): the fingerprints of the head and of the old end tell a rewrite apart
            // from an append.
            if !self.fingerprints_match() {
                self.reload_from_start(new_size, new_modified);
                return;
            }

            self.file_size = new_size;
            self.last_modified = new_modified;
            self.source.set_len(new_size);
            self.has_new_data = true;
            if self.view_mode == ViewMode::Markdown && self.markdown_too_large() {
                self.view_mode = ViewMode::Text;
            }
            if self.index_pending {
                // The index job covers the old range; the rest is indexed when it ends.
                return;
            }
            // On a pure append every previously complete line is unchanged; the last line
            // may have been partial, so it is re-evaluated together with the new ones.
            self.rebuild_line_index_from(prev_lines_count.saturating_sub(1));
            self.update_tail_fingerprint();
            self.check_sound_alerts(prev_lines_count);
            self.collect_tool_hits(prev_lines_count);
            self.note_unseen(prev_lines_count);
            return;
        }

        // In-place modification where size remains identical but timestamp changed or file was replaced
        if new_modified != self.last_modified || !self.source.is_same_file_as_path() {
            self.reload_from_start(new_size, new_modified);
        }
    }

    /// Rebuilds from byte 0 after the file was restarted from empty (a standard-input
    /// spool at its limit), whatever size it has grown back to since.
    pub(crate) fn restart_from_empty(&mut self) {
        let (size, modified) = self
            .current_file
            .as_ref()
            .and_then(|f| std::fs::metadata(f).ok())
            .map(|m| (m.len(), m.modified().ok()))
            .unwrap_or((0, None));
        self.reload_from_start(size, modified);
    }

    /// Full reload after a truncation, rotation or in-place rewrite: reopens the handle,
    /// drops the cache and the index, and rebuilds from byte 0.
    fn reload_from_start(&mut self, new_size: u64, new_modified: Option<std::time::SystemTime>) {
        // New content read from the start: only its head is sampled for colour codes.
        self.ansi_head_end = new_size;
        self.redecode_from_start(new_size, new_modified);
    }

    /// `reload_from_start` keeping the extent of the head sampled for colour codes.
    fn redecode_from_start(&mut self, new_size: u64, new_modified: Option<std::time::SystemTime>) {
        self.file_size = new_size;
        self.last_modified = new_modified;
        self.max_line_bytes = 0;
        self.max_detected_width = 0.0;
        if self.source.reopen().is_err() {
            self.source.clear();
        }
        self.source.set_len(new_size);
        self.has_new_data = true;
        self.rebuild_line_index();
        self.update_fingerprints();
        if self.view_mode == ViewMode::Markdown && self.markdown_too_large() {
            self.view_mode = ViewMode::Text;
        }
    }

    /// Bytes before `end` used to recognise the file across polls (at most 64).
    fn fingerprint_window(&self, end: u64) -> (u64, Vec<u8>) {
        let len = end.min(FINGERPRINT_LEN as u64) as usize;
        let start = end - len as u64;
        let mut buf = vec![0u8; len];
        match self.source.read_direct(start, &mut buf) {
            Ok(n) => {
                buf.truncate(n);
                (start, buf)
            }
            Err(_) => (start, Vec::new()),
        }
    }

    fn update_fingerprints(&mut self) {
        self.head_fingerprint = self
            .fingerprint_window(self.file_size.min(FINGERPRINT_LEN as u64))
            .1;
        self.update_tail_fingerprint();
    }

    fn update_tail_fingerprint(&mut self) {
        self.tail_fingerprint = self.fingerprint_window(self.file_size);
    }

    /// True when the file still starts with the remembered head and still holds the
    /// remembered bytes where the indexed data ended.
    fn fingerprints_match(&self) -> bool {
        if !self.head_fingerprint.is_empty() {
            let mut buf = vec![0u8; self.head_fingerprint.len()];
            match self.source.read_direct(0, &mut buf) {
                Ok(n) if n == buf.len() && buf == self.head_fingerprint => {}
                _ => return false,
            }
        }
        let (start, bytes) = &self.tail_fingerprint;
        if !bytes.is_empty() {
            let mut buf = vec![0u8; bytes.len()];
            match self.source.read_direct(*start, &mut buf) {
                Ok(n) if n == buf.len() && &buf == bytes => {}
                _ => return false,
            }
        }
        true
    }

    /// Length of the byte-order mark for the current encoding, read from the file.
    fn bom_len(&self) -> u64 {
        let head = self.source.read_to_vec(0, 3);
        match self.encoding {
            FileEncoding::Utf8 if head.starts_with(&[0xEF, 0xBB, 0xBF]) => 3,
            FileEncoding::UnicodeLe if head.starts_with(&[0xFF, 0xFE]) => 2,
            FileEncoding::UnicodeBe if head.starts_with(&[0xFE, 0xFF]) => 2,
            _ => 0,
        }
    }

    pub fn check_sound_alerts(&mut self, start_idx: usize) {
        if self.last_sound_alert_time.elapsed().as_millis() < 250 {
            return;
        }
        let total = self.line_offsets.len();
        if start_idx >= total {
            return;
        }

        for idx in start_idx..total {
            if let Some(line) = self.get_line(idx) {
                for ch in &self.compiled_highlights {
                    if ch.enabled && ch.sound_alert != SoundAlertPreset::None {
                        let is_match = if let Some(re) = &ch.regex {
                            re.is_match(&line)
                        } else if ch.case_sensitive {
                            line.contains(&ch.pattern)
                        } else {
                            contains_case_insensitive(&line, &ch.pattern_lower)
                        };
                        if is_match {
                            ch.sound_alert.play();
                            self.last_sound_alert_time = Instant::now();
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Queues `(rule pattern, line index)` for every appended line from `start_idx` that
    /// matches a rule with an external tool bound to it (see `tool_bound_rules`). The app
    /// drains the queue each frame and runs the tools through the throttled runner.
    pub fn collect_tool_hits(&mut self, start_idx: usize) {
        if self.tool_bound_rules.is_empty() {
            return;
        }
        let total = self.line_offsets.len();
        let mut hits = Vec::new();
        for idx in start_idx..total {
            if self.pending_tool_hits.len() + hits.len() >= MAX_PENDING_TOOL_HITS {
                break;
            }
            let Some(line) = self.get_line(idx) else {
                continue;
            };
            for ch in &self.compiled_highlights {
                if !ch.enabled || !self.tool_bound_rules.contains(&ch.pattern) {
                    continue;
                }
                let is_match = if let Some(re) = &ch.regex {
                    re.is_match(&line)
                } else if ch.case_sensitive {
                    line.contains(&ch.pattern)
                } else {
                    contains_case_insensitive(&line, &ch.pattern_lower)
                };
                if is_match {
                    hits.push((ch.pattern.clone(), idx));
                }
            }
        }
        self.pending_tool_hits.extend(hits);
    }

    /// Row an external tool or a stream-menu action applies to: the last clicked row of
    /// the selection, else the current search hit, else the last line.
    pub fn current_row(&self) -> Option<usize> {
        if let Some(anchor) = self.selection_anchor.filter(|a| self.selection.contains(a)) {
            return Some(anchor);
        }
        if let Some(last) = self.selection.iter().next_back() {
            return Some(*last);
        }
        if let Some(hit) = self.current_search_line() {
            return Some(hit);
        }
        self.line_offsets.len().checked_sub(1)
    }

    pub fn total_lines(&self) -> usize {
        self.line_offsets.len()
    }

    /// Start offset, byte length to read (capped at `MAX_LINE_BYTES`) and truncation flag
    /// of line `idx`.
    fn line_span(&self, idx: usize) -> Option<(u64, usize, bool)> {
        if idx >= self.line_offsets.len() {
            return None;
        }
        let start = self.line_offsets[idx];
        let next_start = if idx + 1 < self.line_offsets.len() {
            self.line_offsets[idx + 1]
        } else {
            self.source.len()
        };
        if start > next_start {
            return None;
        }
        let raw_len = (next_start - start) as usize;
        Some(if raw_len > MAX_LINE_BYTES {
            (start, MAX_LINE_BYTES, true)
        } else {
            (start, raw_len, false)
        })
    }

    /// The text of line `idx`, the one every text feature uses: without its escape
    /// sequences in render and strip modes, as stored in raw mode.
    pub fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        let (start, len, truncated) = self.line_span(idx)?;
        let encoding = self.encoding;
        let strip = self.strips_ansi();
        self.source.read_with(start, len, |bytes| {
            Cow::Owned(decode_line_ansi(bytes, encoding, truncated, strip))
        })
    }

    /// Line `idx` for the text view: `get_line` plus, in render mode, the SGR style runs
    /// of the line, and in raw mode whether it holds `ESC` bytes to draw as `␛`.
    pub fn get_row(&self, idx: usize) -> Option<RowText> {
        let (start, len, truncated) = self.line_span(idx)?;
        let encoding = self.encoding;
        let mode = self.ansi_effective();
        self.source.read_with(start, len, |bytes| {
            let text = decode_content(bytes, encoding, truncated);
            let escaped = crate::ansi::has_escape(text.as_bytes());
            let (mut line, ansi, visible_escapes) = match mode {
                AnsiMode::Render if escaped => {
                    let (line, runs) = crate::ansi::strip_and_style(&text);
                    (line, runs, false)
                }
                AnsiMode::Strip if escaped => (crate::ansi::strip_owned(text), Vec::new(), false),
                AnsiMode::Raw => (text, Vec::new(), escaped),
                _ => (text, Vec::new(), false),
            };
            if truncated {
                line.push_str(TRUNCATED_LINE_MARKER);
            }
            RowText {
                line,
                ansi,
                visible_escapes,
            }
        })
    }

    pub fn matches_filter(&self, line: &str) -> bool {
        self.filter.matches(line)
    }

    /// Whole-row style of the first enabled rule matching `line`; captures-only rules
    /// never colour a whole row (see `match_highlight_spans`).
    pub fn match_highlight(&self, line: &str) -> Option<HighlightStyle> {
        for ch in &self.compiled_highlights {
            if !ch.enabled || ch.captures_only {
                continue;
            }
            let is_match = if let Some(re) = &ch.regex {
                re.is_match(line)
            } else if ch.case_sensitive {
                line.contains(&ch.pattern)
            } else {
                contains_case_insensitive(line, &ch.pattern_lower)
            };

            if is_match {
                return Some(ch.style);
            }
        }
        None
    }

    pub fn set_quick_labels(&mut self, labels: &[QuickLabel]) {
        self.quick_labels = labels
            .iter()
            .map(|l| (l.clone(), l.text.to_lowercase()))
            .collect();
    }

    pub fn quick_labels(&self) -> Vec<QuickLabel> {
        self.quick_labels.iter().map(|(l, _)| l.clone()).collect()
    }

    /// True when rows need the span path: an enabled captures-only regex rule or a quick
    /// label exists. Otherwise the renderer keeps the whole-row `match_highlight`.
    pub fn has_span_rules(&self) -> bool {
        !self.quick_labels.is_empty()
            || self
                .compiled_highlights
                .iter()
                .any(|ch| ch.enabled && ch.captures_only && ch.regex.is_some())
    }

    /// Span evaluation of a row, top-down like `match_highlight`, first rule winning per
    /// byte: a captures-only rule claims its captured groups (the whole match when the
    /// pattern has no group); a whole-row rule claims every byte still free and ends the
    /// walk (`rest`); quick labels come after the rules and claim what is left. Spans are
    /// returned sorted, non-overlapping and capped at `MAX_ROW_SPANS`.
    pub fn match_highlight_spans(&self, line: &str) -> SpanHighlight {
        self.match_highlight_spans_with(line, &[])
    }

    /// Span evaluation of a row of the text view: `match_highlight_spans` plus, in render
    /// mode, the row's ANSI colours in the bytes still free (see `match_highlight_spans_with`).
    pub fn match_row_spans(&self, row: &RowText) -> SpanHighlight {
        self.match_highlight_spans_with(&row.line, &row.ansi)
    }

    /// `match_highlight_spans` with the ANSI style runs of the line ranked last: user
    /// rules, then quick labels, then the ANSI colours claim the bytes left, all within
    /// the budget of `MAX_ROW_SPANS`. A whole-row rule leaves nothing to the ANSI colours.
    pub fn match_highlight_spans_with(&self, line: &str, ansi: &[StyleRun]) -> SpanHighlight {
        let mut out = SpanHighlight::default();
        let mut full = false;
        for ch in &self.compiled_highlights {
            if !ch.enabled {
                continue;
            }
            if ch.captures_only {
                let Some(re) = &ch.regex else {
                    continue;
                };
                let style = SpanStyle::Rule(ch.style);
                for caps in re.captures_iter(line) {
                    let groups = if caps.len() > 1 { 1..caps.len() } else { 0..1 };
                    for g in groups {
                        if let Some(m) = caps.get(g) {
                            if m.start() < m.end() {
                                full = claim_span(&mut out.spans, m.start(), m.end(), style);
                            }
                        }
                        if full {
                            break;
                        }
                    }
                    if full {
                        break;
                    }
                }
            } else {
                let is_match = if let Some(re) = &ch.regex {
                    re.is_match(line)
                } else if ch.case_sensitive {
                    line.contains(&ch.pattern)
                } else {
                    contains_case_insensitive(line, &ch.pattern_lower)
                };
                if is_match {
                    out.rest = Some(ch.style);
                    full = true;
                }
            }
            if full {
                break;
            }
        }
        if !full {
            for (label, lower) in &self.quick_labels {
                find_case_insensitive_cb(line, lower, |s, e| {
                    if claim_span(&mut out.spans, s, e, SpanStyle::Label(label.color)) {
                        full = true;
                        false
                    } else {
                        true
                    }
                });
                if full {
                    break;
                }
            }
        }
        if !full {
            for run in ansi {
                if claim_span(
                    &mut out.spans,
                    run.start,
                    run.end,
                    SpanStyle::Ansi(run.style),
                ) {
                    break;
                }
            }
        }
        out.spans.sort_by_key(|s| s.start);
        out
    }

    pub fn is_json_line(line: &str) -> bool {
        let trimmed = line.trim();
        (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    }

    pub fn is_stacktrace_continuation(line: &str) -> bool {
        let trimmed = line.trim_start();
        line.starts_with("  ")
            || line.starts_with('\t')
            || trimmed.starts_with("at ")
            || trimmed.starts_with("Caused by:")
            || trimmed.starts_with("File \"")
            || trimmed.starts_with("Traceback ")
            || trimmed.starts_with("goroutine ")
    }

    pub fn is_filter_active(&self) -> bool {
        self.include_terms.iter().any(|t| !t.is_empty())
            || self.exclude_terms.iter().any(|t| !t.is_empty())
            || self.min_level != LogLevel::Unknown
            || self.time_from.is_some()
            || self.time_to.is_some()
    }

    pub fn recompute_filtered_lines(&mut self) {
        self.refresh_derived_state_from(0);
    }

    /// Recomputes filter visibility and search matches for lines `>= unchanged_lines`,
    /// keeping the already-computed state for the lines before it.
    fn refresh_derived_state_from(&mut self, unchanged_lines: usize) {
        self.recompute_filtered_lines_from(unchanged_lines);
        self.refresh_search_from(unchanged_lines);
    }

    /// Postpones a filter refresh from `start` until the running scan ends.
    fn defer_filter(&mut self, start: usize) {
        if start == 0 {
            self.pending_filter = true;
        } else {
            self.pending_refresh_from =
                Some(self.pending_refresh_from.map_or(start, |p| p.min(start)));
        }
    }

    /// Whether a running timestamp scan holds back filter and search scans: it does while
    /// a time window is set or waits for it, because their result depends on the cache.
    /// Otherwise a filter or search takes over and the timing resumes afterwards.
    fn timestamps_hold_scans(&self) -> bool {
        self.job
            .as_ref()
            .map(|j| j.kind == ScanKind::Timestamps)
            .unwrap_or(false)
            && (self.is_time_filtered() || self.pending_window.is_some())
    }

    fn recompute_filtered_lines_from(&mut self, start: usize) {
        self.filter_generation = self.filter_generation.wrapping_add(1);
        if !self.is_filter_active() {
            self.filtered_lines = Vec::new();
            return;
        }
        if self.index_pending {
            self.pending_filter = true;
            return;
        }
        if self.job.is_some() && start > 0 {
            // A scan is running: refresh the appended tail once it ends.
            self.pending_refresh_from =
                Some(self.pending_refresh_from.map_or(start, |p| p.min(start)));
            return;
        }
        let total = self.total_lines();
        let start = start.min(total);
        if self.is_time_filtered() && self.timestamps.len() < total {
            // The window reads the timestamp cache, and a line not timed yet would be
            // hidden for no reason: filter once the timing is done.
            self.request_timestamps();
            if self.timestamps.len() < total {
                self.defer_filter(start);
                return;
            }
        }
        if start == 0 && self.source.len() > self.job_threshold_bytes {
            if self.timestamps_hold_scans() {
                self.pending_filter = true;
                return;
            }
            // Full recomputation of a large file: worker thread, results stream in. The
            // job filters text and level; the time window is applied to its batches, by
            // index, when they are drained (see `drain_job`).
            self.filtered_lines = Vec::new();
            if !self.last_searched_query.is_empty() {
                self.pending_search = true;
            }
            self.start_job(JobSpec::Filter(self.filter.clone()), 0);
            return;
        }
        if start == 0 {
            self.filtered_lines = Vec::new();
        } else {
            let keep = self.filtered_lines.partition_point(|&idx| idx < start);
            self.filtered_lines.truncate(keep);
        }
        let mut fresh = Vec::new();
        // Continuation lines of a stack trace follow their parent's visibility.
        let mut parent_visible = self.parent_visible_before(start);
        self.scan_lines(start, total, |idx, line| {
            let (visible, next) = self.filter.visible_in_sequence(line, parent_visible);
            parent_visible = next;
            // The time window is applied by index, on top of the text and level filters:
            // a continuation line inherits the entry's time, so it travels with it.
            if visible && self.in_time_range(idx) {
                fresh.push(idx);
            }
            true
        });
        self.filtered_lines.extend(fresh);
    }

    /// Streams lines `[start, end)` from the file in 1 MB chunks and calls `f(idx, text)`
    /// for each one, borrowing the text from the chunk whenever the encoding allows it
    /// (UTF-8 without invalid sequences). `f` returns `false` to stop. This is the path for
    /// full scans (filters, search): no per-line allocation, no cache churn.
    fn scan_lines(&self, start: usize, end: usize, mut f: impl FnMut(usize, &str) -> bool) {
        let total = self.line_offsets.len();
        let end = end.min(total);
        if start >= end {
            return;
        }
        let file_len = self.source.len();
        let line_end = |i: usize| -> u64 {
            if i + 1 < total {
                self.line_offsets[i + 1]
            } else {
                file_len
            }
        };
        let strip = self.strips_ansi();
        let mut chunk = vec![0u8; SCAN_CHUNK];
        let mut i = start;
        while i < end {
            let base = self.line_offsets[i];
            // Last line whose end fits in the chunk; at least one line (capped) per step.
            let limit = base.saturating_add(SCAN_CHUNK as u64);
            let mut k = self.line_offsets[i..end].partition_point(|&o| o <= limit) + i;
            // `k` is the first line starting past the limit; lines i..k start inside, but
            // the last of them may end past it: keep only lines that end within the chunk.
            while k > i + 1 && line_end(k - 1) > limit {
                k -= 1;
            }
            let read_end = if k > i + 1 || line_end(i) <= limit {
                line_end(k - 1)
            } else {
                // A single line longer than the chunk: read it capped, like `get_line`.
                base + (MAX_LINE_BYTES as u64).min(line_end(i) - base)
            };
            let want = ((read_end - base) as usize).min(chunk.len().max(MAX_LINE_BYTES));
            if chunk.len() < want {
                chunk.resize(want, 0);
            }
            let n = match self.source.read_direct(base, &mut chunk[..want]) {
                Ok(n) if n > 0 => n,
                _ => return,
            };
            for idx in i..k {
                let s = (self.line_offsets[idx] - base) as usize;
                let e = ((line_end(idx) - base) as usize).min(n);
                if s > n {
                    // The file shrank under the scan: stop, the poll will rebuild.
                    return;
                }
                let bytes = &chunk[s..e.max(s)];
                let truncated = (line_end(idx) - self.line_offsets[idx]) as usize > MAX_LINE_BYTES;
                let go_on = match self.encoding {
                    FileEncoding::Utf8 if !truncated => {
                        let content = &bytes[..trim_newline_1(bytes)];
                        match std::str::from_utf8(content) {
                            Ok(text) if strip => f(idx, &crate::ansi::strip(text)),
                            Ok(text) => f(idx, text),
                            Err(_) => {
                                let text = String::from_utf8_lossy(content);
                                if strip {
                                    f(idx, &crate::ansi::strip(&text))
                                } else {
                                    f(idx, &text)
                                }
                            }
                        }
                    }
                    enc => f(idx, &decode_line_ansi(bytes, enc, truncated, strip)),
                };
                if !go_on {
                    return;
                }
            }
            i = k;
        }
    }

    pub fn visible_line_count(&self) -> usize {
        if self.is_filter_active() {
            self.filtered_lines.len()
        } else {
            self.total_lines()
        }
    }

    pub fn get_actual_line_idx(&self, visible_row: usize) -> Option<usize> {
        if self.is_filter_active() {
            self.filtered_lines.get(visible_row).copied()
        } else if visible_row < self.total_lines() {
            Some(visible_row)
        } else {
            None
        }
    }

    pub fn get_visible_row_of_line(&self, line_idx: usize) -> Option<usize> {
        if self.is_filter_active() {
            // The visible lines are in file order.
            self.filtered_lines.binary_search(&line_idx).ok()
        } else if line_idx < self.total_lines() {
            Some(line_idx)
        } else {
            None
        }
    }

    pub fn is_line_visible(&self, idx: usize) -> bool {
        if !self.in_time_range(idx) {
            return false;
        }
        let Some(line) = self.get_line(idx) else {
            return false;
        };
        if self.filter.excluded(&line) {
            return false;
        }
        if self.filter.included(&line) && self.filter.level_passes(&line) {
            return true;
        }
        // A stack trace continuation line is shown with its (nearest non-continuation) parent.
        if Self::is_stacktrace_continuation(&line) {
            return self.parent_visible_before(idx);
        }
        false
    }

    /// Whether the time window lets this line through. A line with no timestamp at all -
    /// the banner lines before the first timed entry - cannot be placed in time, so it is
    /// hidden while a window is set; without a window nothing is filtered.
    pub fn in_time_range(&self, idx: usize) -> bool {
        if self.time_from.is_none() && self.time_to.is_none() {
            return true;
        }
        time_window_contains(self.line_timestamp(idx), self.time_from, self.time_to)
    }

    /// The time window, when one is set: `(from, to)`, either side optional.
    pub fn time_window(&self) -> Option<(Option<i64>, Option<i64>)> {
        self.is_time_filtered()
            .then_some((self.time_from, self.time_to))
    }

    /// The include / exclude / level filter as a job evaluates it, `None` when it is off.
    pub fn filter_spec(&self) -> Option<FilterSpec> {
        self.filter.is_active().then(|| self.filter.clone())
    }

    /// The file a job reads and the range covering every line indexed so far, from the
    /// start of the file, as the stream's own full scans see it (encoding, BOM, ANSI mode).
    pub fn full_scan_range(&self) -> (PathBuf, ScanRange) {
        let file = self
            .current_file
            .clone()
            .unwrap_or_else(|| self.path.clone());
        let range = ScanRange {
            start_offset: self.bom_len(),
            end_offset: self.source.len(),
            start_line: 0,
            encoding: self.encoding,
            ansi: self.ansi_effective(),
            parent_visible: false,
        };
        (file, range)
    }

    /// Sets the time window and refreshes what is visible. `None` on a side leaves it open.
    /// The window needs the whole file timed: over a half-timed cache it would hide the
    /// lines that simply have not been read yet. On a stream still being timed in the
    /// background the window is held (`time_range_pending`) and applied when timing
    /// finishes; until then the view keeps what it showed.
    pub fn set_time_range(&mut self, from: Option<i64>, to: Option<i64>) {
        self.pending_window = None;
        if from.is_none() && to.is_none() {
            self.apply_time_window(None, None);
            return;
        }
        self.pending_window = Some(PendingWindow::Range(from, to));
        self.request_timestamps();
    }

    /// Makes `[from, to]` the visible window over a complete cache. Returns whether the
    /// view was refreshed (it is not when the window did not change).
    fn apply_time_window(&mut self, from: Option<i64>, to: Option<i64>) -> bool {
        if self.time_from == from && self.time_to == to {
            return false;
        }
        self.time_from = from;
        self.time_to = to;
        self.refresh_filters();
        true
    }

    /// Whether a window has been entered and waits for the stream to be timed.
    pub fn time_range_pending(&self) -> bool {
        self.pending_window.is_some()
    }

    /// The instant bare times like `14:02` are anchored to: the first timestamp in the
    /// stream, so a log from last week reads the way it is written. Falls back to now for
    /// a stream that has no timestamps at all.
    pub fn time_reference(&self) -> i64 {
        self.timestamps
            .iter()
            .find(|&&ts| ts != NO_TIMESTAMP)
            .copied()
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
            })
    }

    /// Reads the two time fields: each side's instant (`None` = open or unreadable) and
    /// whether it parsed. The "to" side covers the whole minute or second it names.
    fn parse_time_fields(&self) -> (Option<i64>, bool, Option<i64>, bool) {
        let reference = self.time_reference();
        let parse = |text: &str| -> (Option<i64>, bool) {
            if text.trim().is_empty() {
                return (None, true);
            }
            match crate::timestamp::parse_user_time(text, reference) {
                Some(millis) => (Some(millis), true),
                None => (None, false),
            }
        };
        let (from, from_ok) = parse(&self.time_from_text);
        let (to, to_ok) = parse(&self.time_to_text);
        let to = to.map(|millis| crate::timestamp::end_of_typed_time(&self.time_to_text, millis));
        (from, from_ok, to, to_ok)
    }

    /// Applies the two fields as the user typed them. Returns which side failed to parse,
    /// so the field can say so; an empty side is an open end, not an error. On a stream
    /// still being timed in the background the window is held and the fields are read
    /// again when it applies: the day a bare `14:02` belongs to comes from the first
    /// timestamp of the log.
    pub fn apply_time_range_text(&mut self, from_text: &str, to_text: &str) -> (bool, bool) {
        self.time_from_text = from_text.to_owned();
        self.time_to_text = to_text.to_owned();
        if from_text.trim().is_empty() && to_text.trim().is_empty() {
            self.pending_window = None;
            self.apply_time_window(None, None);
            return (true, true);
        }
        self.pending_window = Some(PendingWindow::Texts);
        self.request_timestamps();
        let (_, from_ok, _, to_ok) = self.parse_time_fields();
        (from_ok, to_ok)
    }

    /// Clears the window, a held one included, and the two fields. A background timing
    /// scan keeps running: the cache is useful to the next window or time jump.
    pub fn clear_time_range(&mut self) {
        self.time_from_text.clear();
        self.time_to_text.clear();
        self.pending_window = None;
        self.apply_time_window(None, None);
    }

    /// Times every indexed line, in bounded passes, on the calling thread. The interface
    /// goes through `request_timestamps`, which moves a large file to a background scan;
    /// this stays for the tests and the benchmark.
    pub fn ensure_timestamps(&mut self) {
        while !self.fill_timestamps() {}
    }

    /// Whether anything waits on the timestamp cache being complete.
    fn wants_timestamps(&self) -> bool {
        self.timestamps_wanted
            || self.pending_window.is_some()
            || self.pending_goto_time.is_some()
            || self.is_time_filtered()
    }

    /// Asks for every line to be timed. When the untimed lines take at most
    /// `job_threshold_bytes` this happens right here, as appends always did; above that on
    /// a background `Timestamps` scan, started now or as soon as the running scan allows
    /// it: after the index, in place of a level scan (which resumes afterwards), after a
    /// filter or search scan. Whatever waits on the cache is applied once it is complete.
    fn request_timestamps(&mut self) {
        if self.timestamps_complete() {
            self.on_timestamps_complete();
            return;
        }
        self.timestamps_wanted = true;
        if self.index_pending {
            return;
        }
        let running = self.job.as_ref().map(|j| j.kind);
        if running == Some(ScanKind::Timestamps) {
            return;
        }
        let from = self.timestamps.len();
        let remaining = self.source.len().saturating_sub(self.line_offsets[from]);
        if remaining <= self.job_threshold_bytes {
            self.ensure_timestamps();
            self.on_timestamps_complete();
            return;
        }
        match running {
            None | Some(ScanKind::Levels) => {
                // The scan continues the prefix: what the first untimed line inherits and
                // the format that matched last, exactly what `fill_timestamps` reads.
                let inherited = if from == 0 {
                    NO_TIMESTAMP
                } else {
                    self.timestamps[from - 1]
                };
                let hint = self.timestamp_hint;
                self.start_job(JobSpec::Timestamps { inherited, hint }, from);
            }
            // Queued behind the filter or search scan, started from `finish_job`.
            _ => {}
        }
    }

    /// The cache covers every line: apply the window and the time jump that waited for
    /// it. Returns whether a held window was applied, in which case the filters and the
    /// search have just been refreshed from scratch.
    fn on_timestamps_complete(&mut self) -> bool {
        self.timestamps_wanted = false;
        let refreshed = match self.pending_window.take() {
            None => false,
            Some(PendingWindow::Range(from, to)) => self.apply_time_window(from, to),
            Some(PendingWindow::Texts) => {
                let (from, _, to, _) = self.parse_time_fields();
                self.apply_time_window(from, to)
            }
        };
        if let Some(input) = self.pending_goto_time.take() {
            let result = self.resolve_goto_time(&input);
            if let Some(target) = result {
                // The jump the user asked for a while ago: like any jump, it stops follow.
                self.scroll_to_line = Some(target.line);
                self.follow_tail = false;
            }
            self.goto_time_result = Some(result);
        }
        refreshed
    }

    /// Whether a go-to-time request waits for the stream to be timed.
    pub fn goto_time_waiting(&self) -> bool {
        self.pending_goto_time.is_some()
    }

    /// Outcome of a go-to-time request that waited for the timing, once, for the Ctrl+G
    /// popup: `Some(None)` when the time could not be resolved.
    pub fn take_goto_time_result(&mut self) -> Option<Option<GotoTarget>> {
        self.goto_time_result.take()
    }

    /// Drops a go-to-time request that is still waiting (the popup was closed). The
    /// timing itself goes on.
    pub fn cancel_goto_time(&mut self) {
        self.pending_goto_time = None;
        self.goto_time_result = None;
    }

    /// First line at or after `millis`. Bisects when the log runs forward in time, which
    /// is the normal case; scans when it does not, because a bisection would land anywhere.
    pub fn goto_time(&self, millis: i64) -> Option<usize> {
        let timed = self.timestamps.len();
        if timed == 0 {
            return None;
        }
        if self.timestamps_unordered {
            return self.timestamps[..timed]
                .iter()
                .position(|&ts| ts != NO_TIMESTAMP && ts >= millis);
        }
        let at = self.timestamps[..timed].partition_point(|&ts| ts == NO_TIMESTAMP || ts < millis);
        (at < timed).then_some(at)
    }

    /// Earliest and latest timestamp among the visible lines, for the stream status bar.
    /// On a forward-running log these are the first and the last visible timed lines,
    /// found from each end without walking the whole view and without needing the cache:
    /// a stream nobody has filtered by time still shows its span. An out-of-order log (known
    /// only once the cache is built) is scanned instead, bounded so a filter over millions
    /// of rows stays cheap.
    pub fn visible_time_span(&self) -> Option<(i64, i64)> {
        let count = self.visible_line_count();
        let nth = |i: usize| -> usize {
            if self.is_filter_active() {
                self.filtered_lines[i]
            } else {
                i
            }
        };
        if self.timestamps_unordered {
            let mut span: Option<(i64, i64)> = None;
            for idx in (0..count).take(MAX_SPAN_SCAN).map(nth) {
                if let Some(millis) = self.line_timestamp(idx) {
                    span = Some(match span {
                        Some((lo, hi)) => (lo.min(millis), hi.max(millis)),
                        None => (millis, millis),
                    });
                }
            }
            return span;
        }
        let first = (0..count)
            .take(SPAN_PROBE_LINES)
            .find_map(|i| self.probe_timestamp(nth(i)))?;
        let last = (0..count)
            .rev()
            .take(SPAN_PROBE_LINES)
            .find_map(|i| self.probe_timestamp(nth(i)))?;
        Some((first.min(last), first.max(last)))
    }

    /// Timestamp of a line from the cache when it has been timed, else read from its own
    /// text. A continuation line is `None` on the second path: the caller moves on to the
    /// next line, which is what the span wants anyway.
    fn probe_timestamp(&self, idx: usize) -> Option<i64> {
        if idx < self.timestamps.len() {
            return self.line_timestamp(idx);
        }
        let line = self.get_line(idx)?;
        crate::timestamp::detect_timestamp(&line, self.timestamp_hint).map(|(millis, _)| millis)
    }

    // ----- Time delta column, time anchor and selection elapsed time -----

    /// Asks for the stream to be timed for the time delta column. Never blocks on a large
    /// file: past the job threshold the timing runs as a background scan, and the rows
    /// not timed yet read `TimeDelta::Pending` meanwhile. Cheap to call every frame.
    pub fn want_timestamps(&mut self) {
        if !self.timestamps_complete() {
            self.request_timestamps();
        }
    }

    /// Line the time delta column measures from, if any.
    pub fn time_anchor(&self) -> Option<usize> {
        self.time_anchor
    }

    /// Makes `line` the time anchor; on the anchor line itself this clears it instead.
    pub fn toggle_time_anchor(&mut self, line: usize) {
        self.time_anchor = if self.time_anchor == Some(line) {
            None
        } else {
            Some(line)
        };
    }

    pub fn clear_time_anchor(&mut self) {
        self.time_anchor = None;
    }

    /// The time delta column of visible row `row`. Only a line with a timestamp of its own
    /// shows a value (a stack trace inherits its entry's time and would read `+0.000`),
    /// told apart by reading the line's head, which is cheap for the rows on screen. The
    /// value is the difference with the previous visible row, so it follows the filters;
    /// or, while an anchor is set, with the anchor line.
    pub fn row_time_delta(&self, row: usize) -> TimeDelta {
        let Some(line) = self.get_actual_line_idx(row) else {
            return TimeDelta::Blank;
        };
        if self.time_anchor == Some(line) {
            return TimeDelta::Anchor;
        }
        if line >= self.timestamps.len() {
            return TimeDelta::Pending;
        }
        let own = self.get_line(line).is_some_and(|text| {
            crate::timestamp::detect_timestamp(&text, self.timestamp_hint).is_some()
        });
        let Some(millis) = self.line_timestamp(line).filter(|_| own) else {
            return TimeDelta::Blank;
        };
        let reference = match self.time_anchor {
            Some(anchor) if anchor >= self.timestamps.len() => return TimeDelta::Pending,
            Some(anchor) => anchor,
            None if row == 0 => return TimeDelta::Blank,
            // The previous visible row precedes this line, so it is timed too.
            None => match self.get_actual_line_idx(row - 1) {
                Some(prev) => prev,
                None => return TimeDelta::Blank,
            },
        };
        match self.line_timestamp(reference) {
            Some(from) => TimeDelta::Millis(millis.saturating_sub(from)),
            None => TimeDelta::Blank,
        }
    }

    /// Time from the first to the last selected row, in file order, and the number of
    /// selected rows, for the stream status bar. Ctrl+A spans the first and the last
    /// visible rows. `None` below two rows, on a stream without usable timestamps, or
    /// when either end has no known time; a negative span (out-of-order log) is kept.
    pub fn selection_elapsed(&self) -> Option<(i64, usize)> {
        if !self.timestamps_usable() {
            return None;
        }
        let (first, last, rows) = if self.selection_all {
            let rows = self.visible_line_count();
            let last = self.get_actual_line_idx(rows.checked_sub(1)?)?;
            (self.get_actual_line_idx(0)?, last, rows)
        } else {
            let first = *self.selection.first()?;
            let last = *self.selection.last()?;
            (first, last, self.selection.len())
        };
        if rows < 2 {
            return None;
        }
        let span = self
            .line_timestamp(last)?
            .saturating_sub(self.line_timestamp(first)?);
        Some((span, rows))
    }

    /// Visibility of the nearest non-continuation line before `idx`, the state a sequential
    /// scan starting at `idx` needs for continuation lines.
    fn parent_visible_before(&self, idx: usize) -> bool {
        let mut curr = idx;
        while curr > 0 {
            curr -= 1;
            if let Some(line) = self.get_line(curr) {
                if !Self::is_stacktrace_continuation(&line) {
                    return self.filter.matches(&line);
                }
            }
        }
        false
    }

    pub fn is_line_visible_filtered(&self, idx: usize) -> bool {
        self.is_line_visible(idx)
    }

    pub fn find_matches(&self, query: &str) -> Vec<usize> {
        self.find_matches_from(query, 0, MAX_SEARCH_MATCHES).lines
    }

    /// Case-insensitive search over the visible lines `>= start`: the first `limit` hits
    /// are listed, the others only counted.
    fn find_matches_from(&self, query: &str, start: usize, limit: usize) -> LineHits {
        let mut hits = LineHits::default();
        if query.is_empty() {
            return hits;
        }
        let q_lower = query.to_lowercase();

        let check_match = |line: &str| -> bool { contains_case_insensitive(line, &q_lower) };
        let mut record = |idx: usize| {
            if hits.lines.len() < limit {
                hits.lines.push(idx);
            }
            hits.total += 1;
            hits.last = Some(idx);
        };
        let total = self.total_lines();

        if self.is_filter_active() {
            let first = self.filtered_lines.partition_point(|&idx| idx < start);
            let visible = &self.filtered_lines[first..];
            if visible.len() * 4 < total.saturating_sub(start) {
                // Sparse filter: reading only the visible lines beats scanning the file.
                for &idx in visible {
                    if let Some(line) = self.get_line(idx) {
                        if check_match(&line) {
                            record(idx);
                        }
                    }
                }
            } else {
                // Dense filter: one sequential pass, skipping the hidden lines.
                let mut p = 0;
                self.scan_lines(start, total, |idx, line| {
                    while p < visible.len() && visible[p] < idx {
                        p += 1;
                    }
                    if p < visible.len() && visible[p] == idx && check_match(line) {
                        record(idx);
                    }
                    true
                });
            }
        } else {
            self.scan_lines(start, total, |idx, line| {
                if check_match(line) {
                    record(idx);
                }
                true
            });
        }
        hits
    }

    /// Every visible line matching the active search, the ones past `MAX_SEARCH_MATCHES`
    /// included (those are counted, not listed). Byte hits in HEX view.
    pub fn search_total(&self) -> usize {
        if self.view_mode == ViewMode::Hex {
            self.search_byte_matches.len()
        } else {
            self.search_total.max(self.search_matches.len())
        }
    }

    /// Whether the search matched more lines than it lists (see `MAX_SEARCH_MATCHES`).
    pub fn search_capped(&self) -> bool {
        self.view_mode != ViewMode::Hex && self.search_total > self.search_matches.len()
    }

    /// Forgets the line hits of the search, listed and counted.
    fn clear_search_hits(&mut self) {
        self.search_matches = Vec::new();
        self.search_total = 0;
        self.search_last_counted = 0;
        self.search_last_counted_exact = false;
        self.search_generation = self.search_generation.wrapping_add(1);
    }

    /// Whether a search must wait for the running scan: the index, a filter scan (search
    /// covers the visible lines), or a timestamp scan a time window depends on.
    fn search_waits(&self) -> bool {
        self.index_pending
            || self
                .job
                .as_ref()
                .map(|j| j.kind == ScanKind::Filter)
                .unwrap_or(false)
            || self.timestamps_hold_scans()
    }

    /// Re-runs the active search over lines `>= start` after the buffer or the filter
    /// changed, keeping the current match on the same line whenever it still matches.
    fn refresh_search_from(&mut self, start: usize) {
        if self.last_searched_query.is_empty() {
            return;
        }
        if self.search_waits() {
            // Search follows the filter: rerun it when the index / filter / timing job ends.
            self.pending_search = true;
            return;
        }
        if self.job.is_some() && start > 0 {
            self.pending_refresh_from =
                Some(self.pending_refresh_from.map_or(start, |p| p.min(start)));
            return;
        }
        let keep = self.search_matches.partition_point(|&idx| idx < start);
        // The counted hits the rescan replaces: those at or after `start`.
        let replaced = if start == 0 {
            Some(self.search_total)
        } else {
            counted_hits_from(
                start,
                keep,
                self.search_matches.len(),
                self.search_total,
                self.search_last_counted,
                self.search_last_counted_exact,
            )
        };
        let Some(replaced) = replaced else {
            // Capped, and the counted-only hits reach into the changed lines: which of
            // them were before `start` is not known, count everything again.
            self.refresh_search_from(0);
            return;
        };
        if start == 0 && self.source.len() > self.job_threshold_bytes {
            self.start_search_job();
            return;
        }
        let current_line = self.current_search_line();
        let current_byte = self.current_search_byte().map(|(off, _)| off);
        let query = std::mem::take(&mut self.last_searched_query);

        self.search_matches.truncate(keep);
        let remaining = MAX_SEARCH_MATCHES.saturating_sub(self.search_matches.len());
        let fresh = self.find_matches_from(&query, start, remaining);
        self.search_total = self.search_total.saturating_sub(replaced) + fresh.total;
        match fresh.last {
            Some(last) => {
                self.search_last_counted = last;
                self.search_last_counted_exact = true;
            }
            None if replaced > 0 => {
                // The last counted hit was dropped: the one before it lies before `start`.
                self.search_last_counted = start.saturating_sub(1);
                self.search_last_counted_exact = false;
            }
            None => {}
        }
        self.search_matches.extend(fresh.lines);
        self.search_generation = self.search_generation.wrapping_add(1);

        // Byte matches: everything ending before the first changed byte is still valid
        let start_offset = self
            .line_offsets
            .get(start)
            .map(|&o| o as usize)
            .unwrap_or(0);
        let keep_bytes = self
            .search_byte_matches
            .partition_point(|&(off, len)| off + len <= start_offset);
        self.search_byte_matches.truncate(keep_bytes);
        let rescan_from = start_offset.saturating_sub(self.search_byte_max_len.saturating_sub(1));
        let remaining = MAX_BYTE_MATCHES.saturating_sub(self.search_byte_matches.len());
        let (fresh, max_len) = self.find_byte_matches_from(&query, rescan_from, remaining);
        self.search_byte_matches.extend(
            fresh
                .into_iter()
                .filter(|&(off, len)| off + len > start_offset),
        );
        self.search_byte_max_len = max_len;
        self.last_searched_query = query;

        let restore =
            |matches_len: usize, wanted: Option<usize>, position: Option<usize>| -> Option<usize> {
                if matches_len == 0 {
                    None
                } else {
                    match (wanted, position) {
                        (Some(_), Some(i)) => Some(i.min(matches_len - 1)),
                        _ => Some(0),
                    }
                }
            };
        self.current_match_idx = if self.view_mode == ViewMode::Hex {
            let pos = current_byte.map(|off| {
                match self
                    .search_byte_matches
                    .binary_search_by_key(&off, |&(o, _)| o)
                {
                    Ok(i) | Err(i) => i,
                }
            });
            restore(self.search_byte_matches.len(), current_byte, pos)
        } else {
            let pos = current_line.map(|line| match self.search_matches.binary_search(&line) {
                Ok(i) | Err(i) => i,
            });
            restore(self.search_matches.len(), current_line, pos)
        };
    }

    /// Byte-level search used by the HEX view: the query is matched as ASCII text
    /// (case-insensitive) and, when it looks like a hex byte pattern (`0A 0D`, `0a0d`),
    /// as raw bytes as well. Returns `(matches, longest pattern length)`.
    fn find_byte_matches_from(
        &self,
        query: &str,
        start: usize,
        limit: usize,
    ) -> (Vec<(usize, usize)>, usize) {
        let mut matches = Vec::new();
        let trimmed = query.trim();
        let total = self.source.len() as usize;
        if trimmed.is_empty() || limit == 0 || start >= total {
            return (matches, 0);
        }

        let mut patterns: Vec<(Vec<u8>, bool)> = Vec::new(); // (bytes, case_insensitive)
        let text = trimmed.to_lowercase().into_bytes();
        if !text.is_empty() {
            patterns.push((text, true));
        }
        let no_spaces: String = trimmed
            .chars()
            .filter(|c| !c.is_whitespace() && *c != ':')
            .collect();
        if no_spaces.len() >= 2
            && no_spaces.len().is_multiple_of(2)
            && no_spaces.chars().all(|c| c.is_ascii_hexdigit())
        {
            let bytes: Vec<u8> = (0..no_spaces.len())
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(&no_spaces[i..i + 2], 16).ok())
                .collect();
            if !bytes.is_empty() {
                patterns.push((bytes, false));
            }
        }
        let max_len = patterns.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
        if max_len == 0 {
            return (matches, 0);
        }

        // Stream the file in chunks that overlap by `max_len - 1` bytes so no hit is missed
        // at a boundary; duplicates from the overlap are removed at the end.
        let overlap = max_len - 1;
        let mut chunk = vec![0u8; SCAN_CHUNK + overlap];
        let mut pos = start;
        'outer: while pos < total {
            let want = (total - pos).min(chunk.len());
            let n = match self.source.read_direct(pos as u64, &mut chunk[..want]) {
                Ok(n) if n > 0 => n,
                _ => break,
            };
            let haystack = &chunk[..n];
            for (pattern, ci) in &patterns {
                if pattern.len() > haystack.len() {
                    continue;
                }
                for (i, window) in haystack.windows(pattern.len()).enumerate() {
                    let hit = if *ci {
                        window.eq_ignore_ascii_case(pattern)
                    } else {
                        window == pattern.as_slice()
                    };
                    if hit {
                        matches.push((pos + i, pattern.len()));
                        if matches.len() >= limit * 2 {
                            break 'outer;
                        }
                    }
                }
            }
            if n < want || pos + n >= total {
                break;
            }
            pos += n - overlap;
        }
        matches.sort_unstable();
        matches.dedup();
        matches.truncate(limit);
        (matches, max_len)
    }

    /// Number of hits in the list the current view navigates (bytes in HEX, lines otherwise).
    pub fn active_match_count(&self) -> usize {
        if self.view_mode == ViewMode::Hex {
            self.search_byte_matches.len()
        } else {
            self.search_matches.len()
        }
    }

    /// Switches the view and keeps the search cursor inside the list that view navigates.
    /// Switches soft wrapping for this stream. The viewport keeps its top row: the wrap
    /// anchor starts from the current top row and the scroll bar is re-synchronised.
    pub fn set_wrap_lines(&mut self, wrap: bool, top_row: usize) {
        if self.wrap_lines == wrap {
            return;
        }
        self.wrap_lines = wrap;
        self.wrap_dirty = true;
        self.wrap_anchor = WrapAnchor {
            row: top_row,
            within: 0.0,
        };
        self.wrap_virtual_offset = None;
        self.wrap_scroll_delta = 0.0;
        self.wrap_at_bottom = self.follow_tail;
        self.wrap_request = if self.follow_tail {
            Some(WrapScroll::Bottom)
        } else {
            None
        };
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if mode == ViewMode::Markdown && self.markdown_too_large() {
            // Rendered Markdown needs the whole text in memory: stay in the current view.
            return;
        }
        self.view_notice = None;
        // The current hit of the text view, carried to the same file bytes in HEX.
        let text_hit = if mode == ViewMode::Hex && self.view_mode != ViewMode::Hex {
            self.current_search_line()
        } else {
            None
        };
        self.view_mode = mode;
        if mode == ViewMode::Hex
            && !self.last_searched_query.is_empty()
            && self.search_byte_matches.is_empty()
        {
            let (byte_matches, max_len) =
                self.find_byte_matches_from(&self.last_searched_query.clone(), 0, MAX_BYTE_MATCHES);
            self.search_byte_matches = byte_matches;
            self.search_byte_max_len = max_len;
        }
        if let Some(offset) = text_hit.and_then(|line| self.search_hit_file_offset(line)) {
            let pos = self
                .search_byte_matches
                .partition_point(|&(off, _)| off < offset);
            if let Some(&(off, _)) = self.search_byte_matches.get(pos) {
                self.current_match_idx = Some(pos);
                self.scroll_to_byte = Some(off);
                return;
            }
        }
        let len = self.active_match_count();
        self.current_match_idx = match self.current_match_idx {
            _ if len == 0 => None,
            Some(i) if i < len => Some(i),
            _ => Some(0),
        };
    }

    /// File offset of the first hit of the search query in `line`. The hit is found in
    /// the text the view shows (without escape sequences in render and strip modes) and
    /// converted back through the removed sequences and the encoding, so HEX lands on
    /// the bytes of the word, not on a position shifted by the sequences before it.
    fn search_hit_file_offset(&self, line: usize) -> Option<usize> {
        let (start, len, truncated) = self.line_span(line)?;
        let query = self.last_searched_query.to_lowercase();
        let encoding = self.encoding;
        let strip = self.strips_ansi();
        self.source
            .read_with(start, len, |bytes| {
                let text = decode_content(bytes, encoding, truncated);
                let stripped = if strip {
                    crate::ansi::strip_with_map(&text)
                } else {
                    crate::ansi::Stripped {
                        text: text.clone(),
                        map: vec![(0, 0)],
                    }
                };
                let mut hit = None;
                find_case_insensitive_cb(&stripped.text, &query, |s, _| {
                    hit = Some(s);
                    false
                });
                let raw = stripped.to_raw(hit?).min(text.len());
                let prefix = text.get(..raw)?;
                Some(start as usize + encoded_len(prefix, encoding))
            })
            .flatten()
    }

    pub fn current_search_byte(&self) -> Option<(usize, usize)> {
        self.current_match_idx
            .and_then(|idx| self.search_byte_matches.get(idx).copied())
    }

    /// True when any byte match overlaps the HEX row `[row_start, row_end)`.
    pub fn hex_row_matches(&self, row_start: usize, row_end: usize) -> bool {
        let first = self
            .search_byte_matches
            .partition_point(|&(off, len)| off + len <= row_start);
        self.search_byte_matches[first..]
            .iter()
            .take_while(|&&(off, _)| off < row_end)
            .next()
            .is_some()
    }

    /// Text shown in Markdown mode: the buffer as UTF-8, converted from HTML when it looks
    /// like HTML. Cached per buffer generation so the conversion is not redone every frame.
    pub fn markdown_text(&mut self) -> &str {
        self.ensure_markdown_text();
        self.markdown_text_cache
            .as_ref()
            .map(|(_, s)| s.as_str())
            .unwrap_or("")
    }

    pub fn ensure_markdown_text(&mut self) {
        let generation = self.buffer_generation;
        if self
            .markdown_text_cache
            .as_ref()
            .map(|(g, _)| *g == generation)
            .unwrap_or(false)
        {
            return;
        }
        if self.markdown_too_large() {
            self.markdown_text_cache = Some((generation, String::new()));
            return;
        }
        let total = self.source.len() as usize;
        let mut bytes = vec![0u8; total];
        let n = self.source.read_direct(0, &mut bytes).unwrap_or(0);
        bytes.truncate(n);
        let mut raw = String::from_utf8_lossy(&bytes);
        if self.strips_ansi() && crate::ansi::has_escape(raw.as_bytes()) {
            // Line by line: an unterminated sequence must not swallow the next lines.
            raw = Cow::Owned(
                raw.split('\n')
                    .map(crate::ansi::strip)
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let text = if crate::html_converter::contains_html(&raw) {
            crate::html_converter::html_to_markdown(&raw)
        } else {
            raw.into_owned()
        };
        self.markdown_text_cache = Some((generation, text));
    }

    /// Rendered Markdown needs the whole text in memory: refused above `markdown_max_bytes`.
    pub fn markdown_too_large(&self) -> bool {
        self.source.len() > self.markdown_max_bytes
    }

    /// Updates the maximum allowed byte size for Markdown rendering.
    /// If the current view is Markdown and exceeds this threshold, flips back to Text mode.
    pub fn set_markdown_max_bytes(&mut self, max_bytes: u64) {
        self.markdown_max_bytes = max_bytes;
        self.markdown_text_cache = None;
        if self.view_mode == ViewMode::Markdown && self.markdown_too_large() {
            self.view_mode = ViewMode::Text;
        }
    }

    pub fn update_search(&mut self, query: &str) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            if !self.search_matches.is_empty() || self.search_total > 0 {
                self.clear_search_hits();
            }
            self.search_byte_matches.clear();
            self.search_byte_max_len = 0;
            self.current_match_idx = None;
            self.last_searched_query.clear();
            return;
        }
        if trimmed == self.last_searched_query {
            return;
        }
        self.last_searched_query = trimmed.to_string();
        if self.search_waits() {
            self.clear_search_hits();
            self.search_byte_matches = Vec::new();
            self.current_match_idx = None;
            self.pending_search = true;
            return;
        }
        if self.source.len() > self.job_threshold_bytes {
            self.start_search_job();
            return;
        }
        let hits = self.find_matches_from(trimmed, 0, MAX_SEARCH_MATCHES);
        self.search_matches = hits.lines;
        self.search_total = hits.total;
        self.search_last_counted = hits.last.unwrap_or(0);
        self.search_last_counted_exact = hits.last.is_some();
        self.search_generation = self.search_generation.wrapping_add(1);
        let (byte_matches, max_len) = self.find_byte_matches_from(trimmed, 0, MAX_BYTE_MATCHES);
        self.search_byte_matches = byte_matches;
        self.search_byte_max_len = max_len;
        self.current_match_idx = if self.active_match_count() > 0 {
            Some(0)
        } else {
            None
        };
    }

    /// Starts a background search over the visible lines for the active query. Byte-level
    /// hits (HEX view) are computed synchronously only while the HEX view is showing.
    fn start_search_job(&mut self) {
        let query = self.last_searched_query.clone();
        self.clear_search_hits();
        self.current_match_idx = None;
        let filter = if self.filter.is_active() {
            Some(self.filter.clone())
        } else {
            None
        };
        // The time window is applied to the hits as they are drained, so the worker must
        // not stop at the cap counting hits the window will drop; the drain caps and
        // counts instead. Otherwise the worker lists up to the cap and counts past it.
        let limit = if self.is_time_filtered() {
            usize::MAX
        } else {
            MAX_SEARCH_MATCHES
        };
        self.start_job(
            JobSpec::Search {
                query_lower: query.to_lowercase(),
                filter,
                limit,
                count_past_limit: true,
            },
            0,
        );
        if self.view_mode == ViewMode::Hex {
            let (byte_matches, max_len) = self.find_byte_matches_from(&query, 0, MAX_BYTE_MATCHES);
            self.search_byte_matches = byte_matches;
            self.search_byte_max_len = max_len;
            self.current_match_idx = if self.search_byte_matches.is_empty() {
                None
            } else {
                Some(0)
            };
        } else {
            self.search_byte_matches = Vec::new();
            self.search_byte_max_len = 0;
        }
    }

    /// Spawns a job over `[start_line, end of file)` and makes it the running one.
    fn start_job(&mut self, spec: JobSpec, start_line: usize) {
        self.job_generation = self.job_generation.wrapping_add(1);
        let start_offset = if start_line == 0 {
            self.bom_len()
        } else {
            self.line_offsets
                .get(start_line)
                .copied()
                .unwrap_or(self.source.len())
        };
        let range = ScanRange {
            start_offset,
            end_offset: self.source.len(),
            start_line,
            encoding: self.encoding,
            ansi: self.ansi_effective(),
            parent_visible: self.parent_visible_before(start_line),
        };
        // The file being tailed: for a pattern stream `path` is the pattern itself.
        let file = self.current_file.as_deref().unwrap_or(&self.path);
        self.job = Some(ScanJob::spawn(self.job_generation, file, range, spec));
    }

    /// Running background scan, if any: kind, progress in `0..=1`, hits so far.
    pub fn scan_progress(&self) -> Option<(ScanKind, f32, usize)> {
        self.job.as_ref().map(|j| (j.kind, j.progress, j.hits))
    }

    /// Applies the batches the running job sent since the last poll and finishes it.
    fn drain_job(&mut self) {
        let Some(mut job) = self.job.take() else {
            return;
        };
        let mut finished: Option<Result<usize, ()>> = None;
        while let Some(batch) = job.try_recv() {
            match batch {
                ScanBatch::Lines(mut lines) => {
                    // The job evaluates text and level only; the time window is applied
                    // here, by index, over the complete timestamp cache.
                    if self.is_time_filtered() {
                        lines.retain(|&idx| self.in_time_range(idx));
                    }
                    job.hits += lines.len();
                    match job.kind {
                        ScanKind::Filter => {
                            self.filtered_lines.extend(lines);
                            self.filter_generation = self.filter_generation.wrapping_add(1);
                        }
                        ScanKind::Search => {
                            // Hits past the cap are counted, not stored (a job under a
                            // time window sends them all, see `start_search_job`).
                            if let Some(&last) = lines.last() {
                                self.search_last_counted = last;
                                self.search_last_counted_exact = true;
                            }
                            self.search_total += lines.len();
                            let room = MAX_SEARCH_MATCHES.saturating_sub(self.search_matches.len());
                            self.search_matches.extend(lines.into_iter().take(room));
                            self.search_generation = self.search_generation.wrapping_add(1);
                            if self.current_match_idx.is_none()
                                && self.view_mode != ViewMode::Hex
                                && !self.search_matches.is_empty()
                            {
                                self.current_match_idx = Some(0);
                            }
                        }
                        ScanKind::Index | ScanKind::Levels | ScanKind::Timestamps => {}
                    }
                }
                ScanBatch::Counted { hits, last_line } => {
                    // Search hits past the cap: counted by the worker, never listed.
                    job.hits += hits;
                    self.search_total += hits;
                    self.search_last_counted = last_line;
                    self.search_last_counted_exact = true;
                }
                ScanBatch::Levels(levels) => {
                    self.push_levels(&levels);
                    job.hits = self.levels.len();
                }
                ScanBatch::Timestamps {
                    values,
                    parsed,
                    unordered,
                    hint,
                } => {
                    // What `fill_timestamps` does at the end of a pass.
                    self.timestamps.extend_from_slice(&values);
                    self.timestamps_parsed += parsed;
                    self.timestamps_unordered |= unordered;
                    self.timestamp_hint = hint;
                    self.feed_histogram();
                    job.hits = self.timestamps.len();
                }
                ScanBatch::Offsets {
                    offsets,
                    max_line_bytes,
                } => {
                    self.line_offsets.extend(offsets);
                    job.hits = self.line_offsets.len();
                    if max_line_bytes > self.max_line_bytes {
                        self.max_line_bytes = max_line_bytes;
                        let estimated_width = (max_line_bytes as f32) * 8.5 + 120.0;
                        self.max_detected_width = self.max_detected_width.max(estimated_width);
                    }
                }
                ScanBatch::Progress(p) => {
                    job.progress = if job.kind == ScanKind::Timestamps && job.range.end_offset > 0 {
                        // A resumed timing scan reports the share of the whole file.
                        let start = job.range.start_offset as f32;
                        let end = job.range.end_offset as f32;
                        (start + p * (end - start)) / end
                    } else {
                        p
                    };
                }
                ScanBatch::Done { lines } => {
                    finished = Some(Ok(lines));
                    break;
                }
                ScanBatch::Failed => {
                    finished = Some(Err(()));
                    break;
                }
            }
        }
        match finished {
            None => self.job = Some(job),
            Some(Err(())) => {
                drop(job);
                let file = self
                    .current_file
                    .clone()
                    .unwrap_or_else(|| self.path.clone());
                if let Ok(metadata) = std::fs::metadata(&file) {
                    let modified = metadata.modified().ok();
                    self.reload_from_start(metadata.len(), modified);
                }
            }
            Some(Ok(_)) => {
                let kind = job.kind;
                let end_offset = job.range.end_offset;
                drop(job);
                self.finish_job(kind, end_offset);
            }
        }
    }

    fn finish_job(&mut self, kind: ScanKind, end_offset: u64) {
        match kind {
            ScanKind::Index => {
                self.index_pending = false;
                self.buffer_generation = self.buffer_generation.wrapping_add(1);
                if self.source.len() > end_offset {
                    // The file grew while indexing: index the rest incrementally.
                    let last = self.line_offsets.len().saturating_sub(1);
                    if last > 0 {
                        self.rebuild_line_index_from(last);
                    } else {
                        self.rebuild_line_index_from(0);
                        return;
                    }
                }
                self.update_tail_fingerprint();
                if self.pending_window.is_some() {
                    // A window waits on the timing: time first, the filters would only
                    // run again once it applies.
                    self.request_timestamps();
                }
                if self.pending_filter {
                    self.pending_filter = false;
                    self.recompute_filtered_lines_from(0);
                } else if self.pending_search {
                    self.pending_search = false;
                    self.refresh_search_from(0);
                }
            }
            ScanKind::Filter => {
                if let Some(from) = self.pending_refresh_from.take() {
                    self.recompute_filtered_lines_from(from);
                }
                if self.pending_search {
                    self.pending_search = false;
                    self.refresh_search_from(0);
                } else if !self.last_searched_query.is_empty() {
                    self.refresh_search_from(0);
                }
            }
            ScanKind::Search => {
                if self.current_match_idx.is_none() && self.active_match_count() > 0 {
                    self.current_match_idx = Some(0);
                }
                if let Some(from) = self.pending_refresh_from.take() {
                    self.refresh_derived_state_from(from);
                }
            }
            ScanKind::Levels => {
                if let Some(from) = self.pending_refresh_from.take() {
                    self.refresh_derived_state_from(from);
                }
            }
            ScanKind::Timestamps => {
                // Appends cancel the scan, so it normally ends with every line timed; if
                // not, the deferred work stays for the scan that finishes the job.
                if self.timestamps_complete() {
                    let pending_filter = std::mem::take(&mut self.pending_filter);
                    let pending_search = std::mem::take(&mut self.pending_search);
                    let pending_from = self.pending_refresh_from.take();
                    // A held window refreshes filters and search from scratch; otherwise
                    // run what was deferred while the scan ran.
                    if !self.on_timestamps_complete() {
                        if pending_filter {
                            self.recompute_filtered_lines_from(0);
                        } else if let Some(from) = pending_from {
                            self.recompute_filtered_lines_from(from);
                        }
                        if pending_search {
                            self.refresh_search_from(0);
                        } else if let Some(from) = pending_from {
                            self.refresh_search_from(from);
                        }
                    }
                }
            }
        }
        // A timestamp scan queued behind this one, or displaced by it, resumes from the
        // first untimed line; then the levels, the lowest priority.
        if self.job.is_none() && self.timestamps_wanted {
            self.request_timestamps();
        }
        // Lines appended while a scan ran, or a level scan displaced by another job.
        self.ensure_levels();
    }

    /// Moves to the next hit and returns its target: a line index, or a byte offset in HEX view.
    pub fn search_next(&mut self, sound_enabled: bool) -> Option<usize> {
        let len = self.active_match_count();
        if len == 0 {
            return None;
        }
        let (next_idx, wrapped) = match self.current_match_idx {
            Some(curr) if curr + 1 < len => (curr + 1, false),
            Some(_) => (0, true),
            None => (0, false),
        };
        Some(self.jump_to_match(next_idx, wrapped && sound_enabled))
    }

    /// Moves to the previous hit and returns its target: a line index, or a byte offset in HEX view.
    pub fn search_prev(&mut self, sound_enabled: bool) -> Option<usize> {
        let len = self.active_match_count();
        if len == 0 {
            return None;
        }
        let (prev_idx, wrapped) = match self.current_match_idx {
            Some(curr) if curr > 0 => (curr - 1, false),
            Some(_) => (len - 1, true),
            None => (len - 1, false),
        };
        Some(self.jump_to_match(prev_idx, wrapped && sound_enabled))
    }

    /// Makes hit `idx` of the navigated list the current match (a click in the search
    /// results pane) and returns its target like `search_next`; `None` out of range.
    pub fn select_match(&mut self, idx: usize) -> Option<usize> {
        (idx < self.active_match_count()).then(|| self.jump_to_match(idx, false))
    }

    fn jump_to_match(&mut self, idx: usize, beep: bool) -> usize {
        self.current_match_idx = Some(idx);
        let target = if self.view_mode == ViewMode::Hex {
            let offset = self.search_byte_matches[idx].0;
            self.scroll_to_byte = Some(offset);
            offset
        } else {
            let line = self.search_matches[idx];
            self.scroll_to_line = Some(line);
            line
        };
        if beep {
            crate::audio::SoundAlertPreset::Beep.play();
        }
        target
    }

    // ----- Bookmarks -----

    pub fn toggle_bookmark(&mut self, idx: usize) {
        if idx >= self.total_lines() {
            return;
        }
        if !self.bookmarks.remove(&idx) {
            self.bookmarks.insert(idx);
        }
        self.bookmark_cursor = Some(idx);
        self.bookmarks_dirty = true;
        self.bookmarks_generation = self.bookmarks_generation.wrapping_add(1);
    }

    pub fn clear_bookmarks(&mut self) {
        if !self.bookmarks.is_empty() {
            self.bookmarks_dirty = true;
        }
        self.bookmarks.clear();
        self.bookmark_cursor = None;
        self.bookmarks_generation = self.bookmarks_generation.wrapping_add(1);
    }

    /// Replaces the bookmarks (used when restoring them from the configuration).
    pub fn set_bookmarks<I: IntoIterator<Item = usize>>(&mut self, lines: I) {
        let total = self.total_lines();
        self.bookmarks = lines.into_iter().filter(|&l| l < total).collect();
        self.bookmark_cursor = None;
        self.bookmarks_dirty = false;
        self.bookmarks_generation = self.bookmarks_generation.wrapping_add(1);
    }

    pub fn has_bookmarks(&self) -> bool {
        !self.bookmarks.is_empty()
    }

    pub fn is_bookmarked(&self, idx: usize) -> bool {
        self.bookmarks.contains(&idx)
    }

    /// Bookmarks that pass the active filters, in file order.
    fn visible_bookmarks(&self) -> Vec<usize> {
        self.bookmarks
            .iter()
            .copied()
            .filter(|&l| self.is_line_visible(l))
            .collect()
    }

    /// Jumps to the next visible bookmark after the cursor (or after `from` when there is no
    /// cursor), wrapping around. Returns the target line.
    pub fn bookmark_next(&mut self, from: usize) -> Option<usize> {
        let visible = self.visible_bookmarks();
        if visible.is_empty() {
            return None;
        }
        let start = self.bookmark_cursor.unwrap_or(from);
        let target = match self.bookmark_cursor {
            Some(_) => visible.iter().copied().find(|&l| l > start),
            None => visible.iter().copied().find(|&l| l >= start),
        }
        .unwrap_or(visible[0]);
        self.bookmark_cursor = Some(target);
        self.scroll_to_line = Some(target);
        Some(target)
    }

    /// Jumps to the previous visible bookmark before the cursor, wrapping around.
    pub fn bookmark_prev(&mut self, from: usize) -> Option<usize> {
        let visible = self.visible_bookmarks();
        if visible.is_empty() {
            return None;
        }
        let start = self.bookmark_cursor.unwrap_or(from);
        let target = match self.bookmark_cursor {
            Some(_) => visible.iter().rev().copied().find(|&l| l < start),
            None => visible.iter().rev().copied().find(|&l| l <= start),
        }
        .unwrap_or(*visible.last().unwrap());
        self.bookmark_cursor = Some(target);
        self.scroll_to_line = Some(target);
        Some(target)
    }

    // ----- Background-tab activity -----

    /// Counts lines appended while the stream is not displayed and records the most severe
    /// highlight among them (2 = rule with a sound alert, 1 = any rule, 0 = none).
    fn note_unseen(&mut self, start_idx: usize) {
        if self.displayed {
            return;
        }
        let total = self.total_lines();
        if start_idx >= total {
            return;
        }
        self.unseen_lines = self.unseen_lines.saturating_add(total - start_idx);
        // Bound the per-poll scan so a burst of writes cannot stall the UI thread.
        let scan_end = total.min(start_idx + 2_000);
        for idx in start_idx..scan_end {
            if self.unseen_severity >= 2 {
                break;
            }
            if let Some(line) = self.get_line(idx) {
                for ch in &self.compiled_highlights {
                    if !ch.enabled {
                        continue;
                    }
                    let is_match = if let Some(re) = &ch.regex {
                        re.is_match(&line)
                    } else if ch.case_sensitive {
                        line.contains(&ch.pattern)
                    } else {
                        contains_case_insensitive(&line, &ch.pattern_lower)
                    };
                    if is_match {
                        let sev = if ch.sound_alert != SoundAlertPreset::None {
                            2
                        } else {
                            1
                        };
                        self.unseen_severity = self.unseen_severity.max(sev);
                        break;
                    }
                }
            }
        }
    }

    /// Called by the viewer when the stream is drawn: clears the unseen counter.
    pub fn mark_seen(&mut self) {
        self.displayed = true;
        self.unseen_lines = 0;
        self.unseen_severity = 0;
    }

    // ----- Go to line -----

    /// Resolves a go-to request. `input` is a 1-based line number, or `+N` / `-N` relative to
    /// `current_line` (0-based). Returns the resolved 0-based target and whether the requested
    /// line was hidden by the filters (in which case the first visible line at or after it,
    /// or the last visible line, is returned). `None` when the input is not a number.
    pub fn resolve_goto(&mut self, input: &str, current_line: usize) -> Option<GotoTarget> {
        let total = self.total_lines();
        if total == 0 {
            return None;
        }
        let s = input.trim();
        // A time rather than a line number: "14:02", "14:02:05" or a whole timestamp.
        // Checked first, because `14:02` is not a line number in any reading.
        // A new target replaces a time jump still waiting for the timing.
        self.cancel_goto_time();
        if s.contains(':') {
            // The cache is otherwise built only once a time window is set; a jump by time
            // on a fresh stream needs it as much. On a large stream it is built in the
            // background and the jump happens when it is complete.
            self.request_timestamps();
            if !self.timestamps_complete() {
                self.pending_goto_time = Some(s.to_owned());
                return Some(GotoTarget {
                    requested: 0,
                    line: 0,
                    hidden: false,
                    waiting: true,
                });
            }
            return self.resolve_goto_time(s);
        }
        let requested: usize = if let Some(rel) = s.strip_prefix('+') {
            current_line.saturating_add(rel.trim().parse::<usize>().ok()?)
        } else if let Some(rel) = s.strip_prefix('-') {
            current_line.saturating_sub(rel.trim().parse::<usize>().ok()?)
        } else {
            s.parse::<usize>().ok()?.checked_sub(1)?
        };
        let clamped = requested.min(total - 1);
        Some(self.goto_target_for(clamped, total))
    }

    /// A go-to-time input over the complete cache: the first line at or after that time.
    fn resolve_goto_time(&self, input: &str) -> Option<GotoTarget> {
        let millis = crate::timestamp::parse_user_time(input, self.time_reference())?;
        let line = self.goto_time(millis)?;
        Some(self.goto_target_for(line, self.total_lines()))
    }

    /// Where a jump to `line` actually lands: the line itself when it is visible, the next
    /// visible one when a filter hides it.
    pub fn goto_target_for(&self, line: usize, total: usize) -> GotoTarget {
        let clamped = line.min(total.saturating_sub(1));
        if !self.is_filter_active() {
            return GotoTarget {
                requested: clamped,
                line: clamped,
                hidden: false,
                waiting: false,
            };
        }
        let Some(&last) = self.filtered_lines.last() else {
            // Every line is filtered out: there is nowhere to jump, so stay put.
            return GotoTarget {
                requested: clamped,
                line: clamped,
                hidden: true,
                waiting: false,
            };
        };
        let pos = self.filtered_lines.partition_point(|&l| l < clamped);
        let line = if pos < self.filtered_lines.len() {
            self.filtered_lines[pos]
        } else {
            last
        };
        GotoTarget {
            requested: clamped,
            line,
            hidden: line != clamped,
            waiting: false,
        }
    }

    /// Shows `line` from outside the stream (a result of a search across streams): the
    /// line itself, or the next visible one when the filters now hide it (as for Ctrl+G,
    /// with the same notice). Follow pauses, the row is selected, and the viewer centres
    /// it on its next frame. The stream's own search is left alone.
    pub fn request_jump(&mut self, line: usize, lang: crate::i18n::Language) -> GotoTarget {
        let target = self.goto_target_for(line, self.total_lines());
        self.follow_tail = false;
        self.pending_jump = Some(target.line);
        self.select_row(target.line);
        self.view_notice = target.hidden.then(|| {
            format!(
                "{} {} {}",
                target.requested + 1,
                crate::i18n::t(lang, "goto_hidden"),
                target.line + 1
            )
        });
        target
    }

    // ----- Row selection, clipboard text and export -----

    /// Selects only `idx` and makes it the anchor for Shift+click ranges.
    pub fn select_row(&mut self, idx: usize) {
        self.selection.clear();
        self.selection_all = false;
        self.selection.insert(idx);
        self.selection_anchor = Some(idx);
    }

    /// Adds or removes `idx` (Ctrl+click) and makes it the anchor.
    pub fn toggle_row(&mut self, idx: usize) {
        if self.selection_all {
            // Materialise "all visible" before removing one row from it.
            self.selection = (0..self.visible_line_count())
                .filter_map(|r| self.get_actual_line_idx(r))
                .collect();
            self.selection_all = false;
        }
        if !self.selection.remove(&idx) {
            self.selection.insert(idx);
        }
        self.selection_anchor = Some(idx);
    }

    /// Selects the visible rows between the anchor and `idx` (Shift+click); hidden rows
    /// in between stay unselected. Without a usable anchor this selects `idx` alone.
    pub fn extend_selection_to(&mut self, idx: usize) {
        let anchor_row = self
            .selection_anchor
            .and_then(|a| self.get_visible_row_of_line(a));
        let target_row = self.get_visible_row_of_line(idx);
        match (anchor_row, target_row) {
            (Some(a), Some(b)) => {
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                self.selection_all = false;
                for row in lo..=hi {
                    if let Some(line) = self.get_actual_line_idx(row) {
                        self.selection.insert(line);
                    }
                }
            }
            _ => self.select_row(idx),
        }
    }

    /// Selects every row visible under the active filters (Ctrl+A), lazily.
    pub fn select_all_visible(&mut self) {
        self.selection.clear();
        self.selection_all = true;
        self.selection_anchor = None;
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.selection_all = false;
        self.selection_anchor = None;
    }

    pub fn has_selection(&self) -> bool {
        self.selection_all || !self.selection.is_empty()
    }

    pub fn is_selected(&self, idx: usize) -> bool {
        if self.selection_all {
            self.is_line_visible(idx)
        } else {
            self.selection.contains(&idx)
        }
    }

    /// Selected line indices in file order.
    pub fn selected_lines(&self) -> Vec<usize> {
        if self.selection_all {
            (0..self.visible_line_count())
                .filter_map(|r| self.get_actual_line_idx(r))
                .collect()
        } else {
            self.selection.iter().copied().collect()
        }
    }

    /// Plain text for the clipboard: the selected rows, or the current search hit when
    /// nothing is selected. One line per row, no line numbers or markers.
    pub fn copy_selection_text(&self) -> Option<String> {
        let lines = if self.has_selection() {
            self.selected_lines()
        } else {
            self.current_search_line().into_iter().collect()
        };
        if lines.is_empty() {
            return None;
        }
        let mut out = String::new();
        for (n, idx) in lines.iter().enumerate() {
            if n > 0 {
                out.push('\n');
            }
            if let Some(line) = self.get_line(*idx) {
                out.push_str(&line);
            }
        }
        Some(out)
    }

    /// Writes the given rows as text lines to `sink`, streaming; returns the row count.
    pub fn export_lines<I, W>(&self, indices: I, sink: &mut W) -> std::io::Result<usize>
    where
        I: IntoIterator<Item = usize>,
        W: Write,
    {
        let mut count = 0;
        for idx in indices {
            if let Some(line) = self.get_line(idx) {
                sink.write_all(line.as_bytes())?;
                sink.write_all(b"\n")?;
                count += 1;
            }
        }
        sink.flush()?;
        Ok(count)
    }

    /// Exports the rows that pass the active filters (all rows without filters).
    pub fn export_visible<W: Write>(&self, sink: &mut W) -> std::io::Result<usize> {
        let rows = (0..self.visible_line_count()).filter_map(|r| self.get_actual_line_idx(r));
        self.export_lines(rows, sink)
    }

    /// Exports the lines matching the current search query.
    pub fn export_search_matches<W: Write>(&self, sink: &mut W) -> std::io::Result<usize> {
        self.export_lines(self.search_matches.iter().copied(), sink)
    }

    pub fn current_search_line(&self) -> Option<usize> {
        self.current_match_idx
            .and_then(|idx| self.search_matches.get(idx).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_case_insensitive, counted_hits_from, find_case_insensitive};

    #[test]
    fn counted_hits_replaced_by_a_rescan() {
        // Not capped: the listed hits at or after `start`.
        assert_eq!(counted_hits_from(10, 3, 5, 5, 40, true), Some(2));
        // Capped with a listed hit past `start`: every counted-only hit follows it.
        assert_eq!(counted_hits_from(10, 3, 5, 9, 90, true), Some(6));
        // Capped, every listed hit before `start`: known only at the edges.
        assert_eq!(counted_hits_from(100, 5, 5, 9, 90, true), Some(0));
        assert_eq!(counted_hits_from(100, 5, 5, 9, 99, false), Some(0));
        assert_eq!(counted_hits_from(90, 5, 5, 9, 90, true), Some(1));
        assert_eq!(counted_hits_from(90, 5, 5, 9, 90, false), None);
        assert_eq!(counted_hits_from(80, 5, 5, 9, 90, true), None);
    }

    #[test]
    fn ascii_needle_matches_inside_utf8_haystack_on_char_boundaries() {
        // The fast path scans bytes: UTF-8 continuation bytes are >= 0x80, so an ASCII
        // needle can only ever match at a character boundary. Slicing must not panic.
        let haystack = "città: ERROR débordement Error";
        let hits = find_case_insensitive(haystack, "error");
        assert_eq!(hits.len(), 2);
        for (s, e) in hits {
            assert_eq!(haystack[s..e].to_lowercase(), "error");
        }
        assert!(contains_case_insensitive(haystack, "error"));
        assert!(!contains_case_insensitive(haystack, "warn"));
    }

    #[test]
    fn non_ascii_needle_still_uses_the_lowercasing_path() {
        let haystack = "ERRORE CITTÀ";
        let hits = find_case_insensitive(haystack, "città");
        assert_eq!(hits.len(), 1);
        let (s, e) = hits[0];
        assert_eq!(&haystack[s..e], "CITTÀ");
        assert!(contains_case_insensitive(haystack, "città"));
    }

    #[test]
    fn matches_are_non_overlapping_and_case_folded() {
        let haystack = "aaAAaa";
        assert_eq!(
            find_case_insensitive(haystack, "aa"),
            vec![(0, 2), (2, 4), (4, 6)]
        );
        assert!(find_case_insensitive(haystack, "").is_empty());
        assert!(contains_case_insensitive(haystack, ""));
    }
}
