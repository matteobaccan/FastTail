use crate::audio::SoundAlertPreset;
use crate::file_source::FileSource;
use crate::log_level::{detect_level, LogLevel};
use crate::scan_job::{FilterSpec, JobSpec, ScanBatch, ScanJob, ScanKind, ScanRange};
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

/// Style of a painted span: a captures-only rule's style, or preset colour `1..=9` of a
/// quick label (resolved by the theme in the renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpanStyle {
    Rule(HighlightStyle),
    Label(u8),
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

/// Upper bound on remembered search hits, keeps F3 navigation responsive on huge files.
const MAX_SEARCH_MATCHES: usize = 20_000;
/// Bytes read per step by sequential scans (indexing, byte search).
const SCAN_CHUNK: usize = 1024 * 1024;
/// A single line longer than this is shown truncated, with a marker.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Rendered Markdown needs the whole text: larger files stay in text mode.
pub const MARKDOWN_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Files larger than this run filters and search on a worker thread.
pub const JOB_THRESHOLD_BYTES: u64 = 16 * 1024 * 1024;
/// Files larger than this build their line index on a worker thread.
pub const INDEX_JOB_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;
/// Bytes remembered at the head and at the indexed end to recognise rewrites.
const FINGERPRINT_LEN: usize = 64;
/// Marker appended to a line cut at `MAX_LINE_BYTES`.
pub const TRUNCATED_LINE_MARKER: &str = " …[line truncated]";

/// Decodes one raw line (bytes between two offsets, newline included) in `encoding`,
/// dropping the trailing newline / CR LF unless the line was cut by the length cap.
pub(crate) fn decode_line(bytes: &[u8], encoding: FileEncoding, truncated: bool) -> String {
    let mut s = match encoding {
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
    };
    if truncated {
        s.push_str(TRUNCATED_LINE_MARKER);
    }
    s
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
}

/// Efficient case-insensitive substring search.
/// For ASCII haystack and pre-lowercased needle, avoids heap allocation and uses
/// SIMD-accelerated `memchr2` to skip non-matching positions rapidly.
#[inline]
/// Byte ranges of every non-overlapping, case-insensitive occurrence of `needle_lower`
/// (already lower-cased) in `haystack`.
pub(crate) fn find_case_insensitive(haystack: &str, needle_lower: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle_lower.is_empty() {
        return out;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let h = haystack.as_bytes();
        let n = needle_lower.as_bytes();
        let mut i = 0;
        while i + n.len() <= h.len() {
            if h[i..i + n.len()].eq_ignore_ascii_case(n) {
                out.push((i, i + n.len()));
                i += n.len();
            } else {
                i += 1;
            }
        }
        return out;
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
        out.push((starts[s], ends[e - 1]));
        from = e;
    }
    out
}

pub(crate) fn contains_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let h_bytes = haystack.as_bytes();
        let n_bytes = needle_lower.as_bytes();
        let n_len = n_bytes.len();
        if n_len > h_bytes.len() {
            return false;
        }

        let first_lower = n_bytes[0];
        let first_upper = first_lower.to_ascii_uppercase();
        let max_pos = h_bytes.len() - n_len;
        let mut curr = 0;

        while curr <= max_pos {
            // SIMD-accelerated search for candidate starting positions using first char
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
        false
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

/// Adds `[start, end)` minus the bytes already claimed by `spans`; returns `true` once the
/// cap of `MAX_ROW_SPANS` is reached.
fn claim_span(spans: &mut Vec<HighlightSpan>, start: usize, end: usize, style: SpanStyle) -> bool {
    let mut pieces = vec![(start, end)];
    for sp in spans.iter() {
        let mut next = Vec::with_capacity(pieces.len() + 1);
        for (s, e) in pieces {
            if e <= sp.start || s >= sp.end {
                next.push((s, e));
                continue;
            }
            if s < sp.start {
                next.push((s, sp.start));
            }
            if e > sp.end {
                next.push((sp.end, e));
            }
        }
        pieces = next;
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
    pub include_filter: String,
    pub exclude_filter: String,
    pub filter_case_sensitive: bool,
    pub filter_is_regex: bool,
    /// Minimum-level stage of the filter (`Unknown` = off) and its unknown-level toggle.
    pub min_level: LogLevel,
    pub show_unknown_levels: bool,
    /// Detected level of every line (`LogLevel as u8`), a prefix of the line index: lines
    /// `>= levels.len()` have not been examined yet. Filled on append, on open for files up
    /// to the job threshold, and by a background `Levels` job for larger ones.
    levels: Vec<u8>,
    /// Lines per level among the cached prefix, indexed by `LogLevel as u8`.
    pub level_counts: [u64; LogLevel::COUNT],
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
    pub search_query: String,
    pub search_matches: Vec<usize>,
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
    /// Markdown-mode text (HTML converted when needed), cached per buffer generation.
    pub markdown_text_cache: Option<(u64, String)>,
    pub highlight_rules: Vec<HighlightRule>,
    compiled_highlights: Vec<(Option<Regex>, String, HighlightRule)>,
    /// Quick labels (see `QuickLabel`) with their lower-cased text, evaluated after the
    /// user rules by `match_highlight_spans`.
    quick_labels: Vec<(QuickLabel, String)>,
    pub expanded_json_lines: HashSet<usize>,
    pub requested_scroll_x: Option<f32>,
    pub requested_scroll_y: Option<f32>,
    pub scroll_to_line: Option<usize>,
    pub markdown_cache: egui_commonmark::CommonMarkCache,
    pub current_scroll_x: f32,
    pub current_scroll_y: f32,
    pub max_line_bytes: usize,
    pub max_detected_width: f32,
    pub last_sound_alert_time: Instant,
    _watcher: Option<RecommendedWatcher>,
    rx: Receiver<notify::Result<Event>>,
    pub last_read_time: Instant,
    pub bytes_read_since_tick: u64,
    pub throughput_bps: f64,
}

/// Directory scan cadence of a pattern stream.
pub const PATTERN_SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// How long the "switched to <file>" notice stays in the stream bar.
pub const SWITCH_NOTICE_DURATION: Duration = Duration::from_secs(5);

impl TailEngine {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        Self::open_with_thresholds(path, JOB_THRESHOLD_BYTES, INDEX_JOB_THRESHOLD_BYTES)
    }

    /// Like `open`, with explicit sizes above which filters/search and the initial index
    /// run on a worker thread (tests use 0 to exercise the background paths on small files).
    pub fn open_with_thresholds<P: AsRef<Path>>(
        path: P,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
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
        let view_mode = Self::initial_view_mode(&path_buf, is_binary);

        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).ok();
        if let Some(ref mut w) = watcher {
            let _ = w.watch(&path_buf, RecursiveMode::NonRecursive);
        }

        let current_file = Some(path_buf.clone());
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

    /// Like `open_pattern`, with explicit background-job thresholds (see `open_with_thresholds`).
    pub fn open_pattern_with_thresholds<P: AsRef<Path>>(
        pattern_path: P,
        job_threshold_bytes: u64,
        index_job_threshold_bytes: u64,
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
        let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).ok();
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
        // Drops selection, bookmarks, jobs and per-line caches; re-applies filters and search.
        self.rebuild_line_index();
        self.update_fingerprints();
        self.switch_notice = Some((name, Instant::now()));
        Ok(())
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

    fn initial_view_mode(path: &Path, is_binary: bool) -> ViewMode {
        let is_markdown = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
            .unwrap_or(false);
        if is_binary {
            ViewMode::Hex
        } else if is_markdown {
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
            include_filter: String::new(),
            exclude_filter: String::new(),
            filter_case_sensitive: false,
            filter_is_regex: false,
            min_level: LogLevel::Unknown,
            show_unknown_levels: false,
            levels: Vec::new(),
            level_counts: [0; LogLevel::COUNT],
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
            search_query: String::new(),
            search_matches: Vec::new(),
            search_byte_matches: Vec::new(),
            search_byte_max_len: 0,
            current_match_idx: None,
            last_searched_query: String::new(),
            scroll_to_byte: None,
            search_edited_at: None,
            buffer_generation: 0,
            markdown_text_cache: None,
            highlight_rules: Vec::new(),
            compiled_highlights: Vec::new(),
            quick_labels: Vec::new(),
            expanded_json_lines: HashSet::new(),
            requested_scroll_x: None,
            requested_scroll_y: None,
            scroll_to_line: None,
            markdown_cache: egui_commonmark::CommonMarkCache::default(),
            current_scroll_x: 0.0,
            current_scroll_y: 0.0,
            max_line_bytes: 0,
            max_detected_width: 0.0,
            last_sound_alert_time: Instant::now(),
            _watcher: watcher,
            rx,
            last_read_time: Instant::now(),
            bytes_read_since_tick: 0,
            throughput_bps: 0.0,
        };

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
                let re = if r.is_regex {
                    regex::RegexBuilder::new(&r.pattern)
                        .case_insensitive(!r.case_sensitive)
                        .build()
                        .ok()
                } else {
                    None
                };
                let lower = r.pattern.to_lowercase();
                (re, lower, r.clone())
            })
            .collect();
        self.highlight_rules = rules;
        self.recompute_filtered_lines();
    }

    pub fn refresh_filters(&mut self) {
        self.filter = FilterSpec::build(
            &self.include_filter,
            &self.exclude_filter,
            self.filter_case_sensitive,
            self.filter_is_regex,
        )
        .with_levels(self.min_level, self.show_unknown_levels);
        self.recompute_filtered_lines();
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
        for &v in fresh {
            self.level_counts[(v as usize).min(LogLevel::COUNT - 1)] += 1;
        }
        self.levels.extend_from_slice(fresh);
    }

    /// Forgets the cached levels of lines `>= keep` (the index changed from there).
    fn truncate_levels(&mut self, keep: usize) {
        if self.levels.len() <= keep {
            return;
        }
        for &v in &self.levels[keep..] {
            let slot = &mut self.level_counts[(v as usize).min(LogLevel::COUNT - 1)];
            *slot = slot.saturating_sub(1);
        }
        self.levels.truncate(keep);
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

    pub fn set_include_filter(&mut self, filter: &str) {
        self.include_filter = filter.to_string();
        self.refresh_filters();
    }

    pub fn set_exclude_filter(&mut self, filter: &str) {
        self.exclude_filter = filter.to_string();
        self.refresh_filters();
    }

    pub fn set_encoding(&mut self, encoding: FileEncoding) {
        self.encoding = encoding;
        self.rebuild_line_index();
    }

    pub fn rebuild_line_index(&mut self) {
        // The file was truncated, rewritten or re-decoded: row indices no longer mean the same.
        self.job = None;
        self.index_pending = false;
        self.pending_refresh_from = None;
        self.clear_selection();
        self.clear_bookmarks();
        self.rebuild_line_index_from(0);
    }

    /// Rebuilds the line offsets and refreshes the derived state (filter visibility,
    /// search matches). Lines before `unchanged_lines` are known to be identical to the
    /// previous index, so their derived state is kept instead of being rescanned.
    fn rebuild_line_index_from(&mut self, unchanged_lines: usize) {
        let total_len = self.source.len();
        // The level cache follows the index: drop what a running level scan would push out
        // of order, and forget the lines that are about to be rescanned.
        if self
            .job
            .as_ref()
            .map(|j| j.kind == ScanKind::Levels)
            .unwrap_or(false)
        {
            self.job = None;
        }
        self.truncate_levels(unchanged_lines);
        if unchanged_lines == 0 && total_len > self.index_job_threshold_bytes {
            // Large file: index on a worker thread; the view shows lines as they arrive.
            self.line_offsets = Vec::new();
            self.filtered_lines = Vec::new();
            self.search_matches = Vec::new();
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
        self.refresh_derived_state_from(unchanged_lines);
        self.ensure_levels();
    }

    pub fn poll_updates(&mut self) {
        self.drain_job();
        if !self.is_watching {
            return;
        }
        let mut needs_refresh = false;

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

        // Periodic size check (handles network drives and Windows handle caches)
        if let Some(current) = &self.current_file {
            if let Ok(metadata) = std::fs::metadata(current) {
                let current_len = metadata.len();
                if current_len != self.file_size {
                    needs_refresh = true;
                }
            }
        } else {
            needs_refresh = false;
        }

        if needs_refresh {
            self.refresh_file();
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
            let prev_lines_count = self.line_offsets.len();
            let added_bytes = new_size - self.file_size;
            self.bytes_read_since_tick += added_bytes;

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
            if self.index_pending {
                // The index job covers the old range; the rest is indexed when it ends.
                return;
            }
            // On a pure append every previously complete line is unchanged; the last line
            // may have been partial, so it is re-evaluated together with the new ones.
            self.rebuild_line_index_from(prev_lines_count.saturating_sub(1));
            self.update_tail_fingerprint();
            self.check_sound_alerts(prev_lines_count);
            self.note_unseen(prev_lines_count);
            return;
        }

        // In-place modification where size remains identical but timestamp changed
        if new_modified != self.last_modified {
            self.reload_from_start(new_size, new_modified);
        }
    }

    /// Full reload after a truncation, rotation or in-place rewrite: reopens the handle,
    /// drops the cache and the index, and rebuilds from byte 0.
    fn reload_from_start(&mut self, new_size: u64, new_modified: Option<std::time::SystemTime>) {
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
                for (re_opt, pat_lower, rule) in &self.compiled_highlights {
                    if rule.enabled && rule.sound_alert != SoundAlertPreset::None {
                        let is_match = if let Some(re) = re_opt {
                            re.is_match(&line)
                        } else if rule.case_sensitive {
                            line.contains(&rule.pattern)
                        } else {
                            contains_case_insensitive(&line, pat_lower)
                        };
                        if is_match {
                            rule.sound_alert.play();
                            self.last_sound_alert_time = Instant::now();
                            return;
                        }
                    }
                }
            }
        }
    }

    pub fn total_lines(&self) -> usize {
        self.line_offsets.len()
    }

    pub fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
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
        let (len, truncated) = if raw_len > MAX_LINE_BYTES {
            (MAX_LINE_BYTES, true)
        } else {
            (raw_len, false)
        };
        let encoding = self.encoding;
        self.source.read_with(start, len, |bytes| {
            Cow::Owned(decode_line(bytes, encoding, truncated))
        })
    }

    pub fn matches_filter(&self, line: &str) -> bool {
        self.filter.matches(line)
    }

    /// Whole-row style of the first enabled rule matching `line`; captures-only rules
    /// never colour a whole row (see `match_highlight_spans`).
    pub fn match_highlight(&self, line: &str) -> Option<HighlightStyle> {
        for (re_opt, pat_lower, rule) in &self.compiled_highlights {
            if !rule.enabled || rule.captures_only {
                continue;
            }
            let is_match = if let Some(re) = re_opt {
                re.is_match(line)
            } else if rule.case_sensitive {
                line.contains(&rule.pattern)
            } else {
                contains_case_insensitive(line, pat_lower)
            };

            if is_match {
                return Some(rule.style());
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
                .any(|(re, _, rule)| rule.enabled && rule.captures_only && re.is_some())
    }

    /// Span evaluation of a row, top-down like `match_highlight`, first rule winning per
    /// byte: a captures-only rule claims its captured groups (the whole match when the
    /// pattern has no group); a whole-row rule claims every byte still free and ends the
    /// walk (`rest`); quick labels come after the rules and claim what is left. Spans are
    /// returned sorted, non-overlapping and capped at `MAX_ROW_SPANS`.
    pub fn match_highlight_spans(&self, line: &str) -> SpanHighlight {
        let mut out = SpanHighlight::default();
        let mut full = false;
        for (re_opt, pat_lower, rule) in &self.compiled_highlights {
            if !rule.enabled {
                continue;
            }
            if rule.captures_only {
                let Some(re) = re_opt else {
                    continue;
                };
                let style = SpanStyle::Rule(rule.style());
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
                let is_match = if let Some(re) = re_opt {
                    re.is_match(line)
                } else if rule.case_sensitive {
                    line.contains(&rule.pattern)
                } else {
                    contains_case_insensitive(line, pat_lower)
                };
                if is_match {
                    out.rest = Some(rule.style());
                    full = true;
                }
            }
            if full {
                break;
            }
        }
        if !full {
            for (label, lower) in &self.quick_labels {
                for (s, e) in find_case_insensitive(line, lower) {
                    if claim_span(&mut out.spans, s, e, SpanStyle::Label(label.color)) {
                        full = true;
                        break;
                    }
                }
                if full {
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
        !self.include_filter.is_empty()
            || !self.exclude_filter.is_empty()
            || self.min_level != LogLevel::Unknown
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

    fn recompute_filtered_lines_from(&mut self, start: usize) {
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
        if start == 0 && self.source.len() > self.job_threshold_bytes {
            // Full recomputation of a large file: worker thread, results stream in.
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
            if visible {
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
                            Ok(text) => f(idx, text),
                            Err(_) => f(idx, &String::from_utf8_lossy(content)),
                        }
                    }
                    enc => f(idx, &decode_line(bytes, enc, truncated)),
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
            self.filtered_lines.iter().position(|&idx| idx == line_idx)
        } else if line_idx < self.total_lines() {
            Some(line_idx)
        } else {
            None
        }
    }

    pub fn is_line_visible(&self, idx: usize) -> bool {
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
        self.find_matches_from(query, 0, MAX_SEARCH_MATCHES)
    }

    /// Case-insensitive search over the visible lines `>= start`, at most `limit` hits.
    fn find_matches_from(&self, query: &str, start: usize, limit: usize) -> Vec<usize> {
        let mut matches = Vec::new();
        if query.is_empty() || limit == 0 {
            return matches;
        }
        let q_lower = query.to_lowercase();

        let check_match = |line: &str| -> bool { contains_case_insensitive(line, &q_lower) };
        let total = self.total_lines();

        if self.is_filter_active() {
            let first = self.filtered_lines.partition_point(|&idx| idx < start);
            let visible = &self.filtered_lines[first..];
            if visible.len() * 4 < total.saturating_sub(start) {
                // Sparse filter: reading only the visible lines beats scanning the file.
                for &idx in visible {
                    if let Some(line) = self.get_line(idx) {
                        if check_match(&line) {
                            matches.push(idx);
                            if matches.len() >= limit {
                                break;
                            }
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
                        matches.push(idx);
                        if matches.len() >= limit {
                            return false;
                        }
                    }
                    true
                });
            }
        } else {
            self.scan_lines(start, total, |idx, line| {
                if check_match(line) {
                    matches.push(idx);
                    if matches.len() >= limit {
                        return false;
                    }
                }
                true
            });
        }
        matches
    }

    /// Re-runs the active search over lines `>= start` after the buffer or the filter
    /// changed, keeping the current match on the same line whenever it still matches.
    fn refresh_search_from(&mut self, start: usize) {
        if self.last_searched_query.is_empty() {
            return;
        }
        if self.index_pending
            || self
                .job
                .as_ref()
                .map(|j| j.kind == ScanKind::Filter)
                .unwrap_or(false)
        {
            // Search follows the filter: rerun it when the index / filter job ends.
            self.pending_search = true;
            return;
        }
        if self.job.is_some() && start > 0 {
            self.pending_refresh_from =
                Some(self.pending_refresh_from.map_or(start, |p| p.min(start)));
            return;
        }
        if start == 0 && self.source.len() > self.job_threshold_bytes {
            self.start_search_job();
            return;
        }
        let current_line = self.current_search_line();
        let current_byte = self.current_search_byte().map(|(off, _)| off);
        let query = std::mem::take(&mut self.last_searched_query);

        let keep = self.search_matches.partition_point(|&idx| idx < start);
        self.search_matches.truncate(keep);
        let remaining = MAX_SEARCH_MATCHES.saturating_sub(self.search_matches.len());
        let fresh = self.find_matches_from(&query, start, remaining);
        self.search_matches.extend(fresh);

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
        let remaining = MAX_SEARCH_MATCHES.saturating_sub(self.search_byte_matches.len());
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
        self.view_mode = mode;
        if mode == ViewMode::Hex
            && !self.last_searched_query.is_empty()
            && self.search_byte_matches.is_empty()
        {
            let (byte_matches, max_len) = self.find_byte_matches_from(
                &self.last_searched_query.clone(),
                0,
                MAX_SEARCH_MATCHES,
            );
            self.search_byte_matches = byte_matches;
            self.search_byte_max_len = max_len;
        }
        let len = self.active_match_count();
        self.current_match_idx = match self.current_match_idx {
            _ if len == 0 => None,
            Some(i) if i < len => Some(i),
            _ => Some(0),
        };
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
        let raw = String::from_utf8_lossy(&bytes);
        let text = if crate::html_converter::contains_html(&raw) {
            crate::html_converter::html_to_markdown(&raw)
        } else {
            raw.into_owned()
        };
        self.markdown_text_cache = Some((generation, text));
    }

    /// Rendered Markdown needs the whole text in memory: refused above `MARKDOWN_MAX_BYTES`.
    pub fn markdown_too_large(&self) -> bool {
        self.source.len() > MARKDOWN_MAX_BYTES
    }

    pub fn update_search(&mut self, query: &str) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.search_matches.clear();
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
        if self.index_pending
            || self
                .job
                .as_ref()
                .map(|j| j.kind == ScanKind::Filter)
                .unwrap_or(false)
        {
            self.search_matches = Vec::new();
            self.search_byte_matches = Vec::new();
            self.current_match_idx = None;
            self.pending_search = true;
            return;
        }
        if self.source.len() > self.job_threshold_bytes {
            self.start_search_job();
            return;
        }
        self.search_matches = self.find_matches(trimmed);
        let (byte_matches, max_len) = self.find_byte_matches_from(trimmed, 0, MAX_SEARCH_MATCHES);
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
        self.search_matches = Vec::new();
        self.current_match_idx = None;
        let filter = if self.filter.is_active() {
            Some(self.filter.clone())
        } else {
            None
        };
        self.start_job(
            JobSpec::Search {
                query_lower: query.to_lowercase(),
                filter,
                limit: MAX_SEARCH_MATCHES,
            },
            0,
        );
        if self.view_mode == ViewMode::Hex {
            let (byte_matches, max_len) =
                self.find_byte_matches_from(&query, 0, MAX_SEARCH_MATCHES);
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
            parent_visible: self.parent_visible_before(start_line),
        };
        self.job = Some(ScanJob::spawn(self.job_generation, &self.path, range, spec));
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
                ScanBatch::Lines(lines) => {
                    job.hits += lines.len();
                    match job.kind {
                        ScanKind::Filter => self.filtered_lines.extend(lines),
                        ScanKind::Search => {
                            self.search_matches.extend(lines);
                            if self.current_match_idx.is_none()
                                && self.view_mode != ViewMode::Hex
                                && !self.search_matches.is_empty()
                            {
                                self.current_match_idx = Some(0);
                            }
                        }
                        ScanKind::Index | ScanKind::Levels => {}
                    }
                }
                ScanBatch::Levels(levels) => {
                    self.push_levels(&levels);
                    job.hits = self.levels.len();
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
                ScanBatch::Progress(p) => job.progress = p,
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
                if let Ok(metadata) = std::fs::metadata(&self.path) {
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
        }
        // Lines appended while a scan ran, or a level scan displaced by a filter/search job.
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
    }

    pub fn clear_bookmarks(&mut self) {
        if !self.bookmarks.is_empty() {
            self.bookmarks_dirty = true;
        }
        self.bookmarks.clear();
        self.bookmark_cursor = None;
    }

    /// Replaces the bookmarks (used when restoring them from the configuration).
    pub fn set_bookmarks<I: IntoIterator<Item = usize>>(&mut self, lines: I) {
        let total = self.total_lines();
        self.bookmarks = lines.into_iter().filter(|&l| l < total).collect();
        self.bookmark_cursor = None;
        self.bookmarks_dirty = false;
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
                for (re_opt, pat_lower, rule) in &self.compiled_highlights {
                    if !rule.enabled {
                        continue;
                    }
                    let is_match = if let Some(re) = re_opt {
                        re.is_match(&line)
                    } else if rule.case_sensitive {
                        line.contains(&rule.pattern)
                    } else {
                        contains_case_insensitive(&line, pat_lower)
                    };
                    if is_match {
                        let sev = if rule.sound_alert != SoundAlertPreset::None {
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
    pub fn resolve_goto(&self, input: &str, current_line: usize) -> Option<GotoTarget> {
        let total = self.total_lines();
        if total == 0 {
            return None;
        }
        let s = input.trim();
        let requested: usize = if let Some(rel) = s.strip_prefix('+') {
            current_line.saturating_add(rel.trim().parse::<usize>().ok()?)
        } else if let Some(rel) = s.strip_prefix('-') {
            current_line.saturating_sub(rel.trim().parse::<usize>().ok()?)
        } else {
            s.parse::<usize>().ok()?.checked_sub(1)?
        };
        let clamped = requested.min(total - 1);
        if !self.is_filter_active() {
            return Some(GotoTarget {
                requested: clamped,
                line: clamped,
                hidden: false,
            });
        }
        if self.filtered_lines.is_empty() {
            return None;
        }
        let pos = self.filtered_lines.partition_point(|&l| l < clamped);
        let line = if pos < self.filtered_lines.len() {
            self.filtered_lines[pos]
        } else {
            *self.filtered_lines.last().unwrap()
        };
        Some(GotoTarget {
            requested: clamped,
            line,
            hidden: line != clamped,
        })
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
