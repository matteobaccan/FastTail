use crate::audio::SoundAlertPreset;
use egui::Color32;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

fn open_file_shared(path: &Path) -> Result<File, std::io::Error> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(7); // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    }
    options.open(path)
}

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
        }
    }
}

/// Upper bound on remembered search hits, keeps F3 navigation responsive on huge files.
const MAX_SEARCH_MATCHES: usize = 20_000;

/// Efficient case-insensitive substring search.
/// For ASCII haystack and pre-lowercased needle, avoids heap allocation by checking ASCII byte windows.
#[inline]
fn contains_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let h_bytes = haystack.as_bytes();
        let n_bytes = needle_lower.as_bytes();
        if n_bytes.len() > h_bytes.len() {
            return false;
        }
        h_bytes
            .windows(n_bytes.len())
            .any(|w| w.eq_ignore_ascii_case(n_bytes))
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

pub struct TailEngine {
    pub path: PathBuf,
    pub buffer: Vec<u8>,
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
    include_regex: Option<Regex>,
    exclude_regex: Option<Regex>,
    pub filtered_lines: Vec<usize>,
    pub search_query: String,
    pub search_matches: Vec<usize>,
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
    include_filter_lower: String,
    exclude_filter_lower: String,
    pub highlight_rules: Vec<HighlightRule>,
    compiled_highlights: Vec<(Option<Regex>, String, HighlightRule)>,
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

impl TailEngine {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
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

        let mut buffer = Vec::new();
        if file_size > 0 {
            let mut file = open_file_shared(&path_buf)?;
            if let Ok(size_usize) = usize::try_from(file_size) {
                buffer.reserve_exact(size_usize);
            }
            file.read_to_end(&mut buffer)?;
            // `file` dropped here, closing file handle immediately!
        }

        let (detected_encoding, is_binary) = if !buffer.is_empty() {
            if buffer.starts_with(&[0xEF, 0xBB, 0xBF]) {
                (FileEncoding::Utf8, false)
            } else if buffer.starts_with(&[0xFF, 0xFE]) {
                (FileEncoding::UnicodeLe, false)
            } else if buffer.starts_with(&[0xFE, 0xFF]) {
                (FileEncoding::UnicodeBe, false)
            } else {
                let sample_len = buffer.len().min(512);
                let sample = &buffer[..sample_len];
                if sample.len() >= 4
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
        } else {
            (FileEncoding::Utf8, false)
        };

        let is_markdown = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
            .unwrap_or(false);

        let view_mode = if is_binary {
            ViewMode::Hex
        } else if is_markdown {
            ViewMode::Markdown
        } else {
            ViewMode::Text
        };

        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).ok();
        if let Some(ref mut w) = watcher {
            let _ = w.watch(&path_buf, RecursiveMode::NonRecursive);
        }

        let mut engine = Self {
            path: path_buf,
            buffer,
            line_offsets: Vec::new(),
            file_size,
            last_modified,
            follow_tail: true,
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
            include_regex: None,
            exclude_regex: None,
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
            include_filter_lower: String::new(),
            exclude_filter_lower: String::new(),
            highlight_rules: Vec::new(),
            compiled_highlights: Vec::new(),
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
        Ok(engine)
    }

    pub fn get_bytes(&self, offset: usize, len: usize) -> Option<&[u8]> {
        if offset >= self.buffer.len() {
            return None;
        }
        let end = (offset + len).min(self.buffer.len());
        Some(&self.buffer[offset..end])
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

    pub fn release_mmap(&mut self) {
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        self.buffer_generation = self.buffer_generation.wrapping_add(1);
        self.markdown_text_cache = None;
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
        self.include_filter_lower = self.include_filter.to_lowercase();
        self.exclude_filter_lower = self.exclude_filter.to_lowercase();
        if self.filter_is_regex {
            self.include_regex = if self.include_filter.is_empty() {
                None
            } else {
                regex::RegexBuilder::new(&self.include_filter)
                    .case_insensitive(!self.filter_case_sensitive)
                    .build()
                    .ok()
            };
            self.exclude_regex = if self.exclude_filter.is_empty() {
                None
            } else {
                regex::RegexBuilder::new(&self.exclude_filter)
                    .case_insensitive(!self.filter_case_sensitive)
                    .build()
                    .ok()
            };
        } else {
            self.include_regex = None;
            self.exclude_regex = None;
        }
        self.recompute_filtered_lines();
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
        self.rebuild_line_index_from(0);
    }

    /// Rebuilds the line offsets and refreshes the derived state (filter visibility,
    /// search matches). Lines before `unchanged_lines` are known to be identical to the
    /// previous index, so their derived state is kept instead of being rescanned.
    fn rebuild_line_index_from(&mut self, unchanged_lines: usize) {
        self.line_offsets.clear();
        let mmap = &self.buffer;
        if mmap.is_empty() {
            return;
        }

        let mut max_bytes = 0usize;
        match self.encoding {
            FileEncoding::Utf8 | FileEncoding::Ascii | FileEncoding::Ansi => {
                let start_offset = if self.encoding == FileEncoding::Utf8
                    && mmap.starts_with(&[0xEF, 0xBB, 0xBF])
                {
                    3
                } else {
                    0
                };
                if start_offset < mmap.len() {
                    self.line_offsets.push(start_offset as u64);
                }
                let mut prev_offset = start_offset;
                for (i, &byte) in mmap.iter().enumerate().skip(start_offset) {
                    if byte == b'\n' {
                        let len = i.saturating_sub(prev_offset);
                        if len > max_bytes {
                            max_bytes = len;
                        }
                        if i + 1 < mmap.len() {
                            self.line_offsets.push((i + 1) as u64);
                            prev_offset = i + 1;
                        }
                    }
                }
                let last_len = mmap.len().saturating_sub(prev_offset);
                if last_len > max_bytes {
                    max_bytes = last_len;
                }
            }
            FileEncoding::UnicodeLe => {
                let start_offset = if mmap.starts_with(&[0xFF, 0xFE]) {
                    2
                } else {
                    0
                };
                if start_offset < mmap.len() {
                    self.line_offsets.push(start_offset as u64);
                }
                let mut prev_offset = start_offset;
                let mut i = start_offset;
                while i + 1 < mmap.len() {
                    if mmap[i] == 0x0A && mmap[i + 1] == 0x00 {
                        let len = (i.saturating_sub(prev_offset)) / 2;
                        if len > max_bytes {
                            max_bytes = len;
                        }
                        if i + 2 < mmap.len() {
                            self.line_offsets.push((i + 2) as u64);
                            prev_offset = i + 2;
                        }
                    }
                    i += 2;
                }
                let last_len = (mmap.len().saturating_sub(prev_offset)) / 2;
                if last_len > max_bytes {
                    max_bytes = last_len;
                }
            }
            FileEncoding::UnicodeBe => {
                let start_offset = if mmap.starts_with(&[0xFE, 0xFF]) {
                    2
                } else {
                    0
                };
                if start_offset < mmap.len() {
                    self.line_offsets.push(start_offset as u64);
                }
                let mut prev_offset = start_offset;
                let mut i = start_offset;
                while i + 1 < mmap.len() {
                    if mmap[i] == 0x00 && mmap[i + 1] == 0x0A {
                        let len = (i.saturating_sub(prev_offset)) / 2;
                        if len > max_bytes {
                            max_bytes = len;
                        }
                        if i + 2 < mmap.len() {
                            self.line_offsets.push((i + 2) as u64);
                            prev_offset = i + 2;
                        }
                    }
                    i += 2;
                }
                let last_len = (mmap.len().saturating_sub(prev_offset)) / 2;
                if last_len > max_bytes {
                    max_bytes = last_len;
                }
            }
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
    }

    pub fn poll_updates(&mut self) {
        if !self.is_watching {
            return;
        }
        let mut needs_refresh = false;

        // Drain filesystem watcher events
        while let Ok(event_res) = self.rx.try_recv() {
            if let Ok(event) = event_res {
                if event.kind.is_modify() || event.kind.is_create() {
                    needs_refresh = true;
                }
            }
        }

        // Periodic size check (handles network drives and Windows handle caches)
        if let Ok(metadata) = std::fs::metadata(&self.path) {
            let current_len = metadata.len();
            if current_len != self.file_size {
                needs_refresh = true;
            }
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
        if let Ok(metadata) = std::fs::metadata(&self.path) {
            let new_size = metadata.len();
            let new_modified = metadata.modified().ok();

            if new_size < self.file_size {
                // File was truncated or rotated! Reset completely.
                self.file_size = new_size;
                self.last_modified = new_modified;
                self.max_line_bytes = 0;
                self.max_detected_width = 0.0;
                self.buffer.clear();
                if new_size > 0 {
                    if let Ok(mut file) = open_file_shared(&self.path) {
                        let _ = file.read_to_end(&mut self.buffer);
                        // file is dropped and closed immediately!
                    }
                }
                self.has_new_data = true;
                self.rebuild_line_index();
                return;
            }

            if new_size > self.file_size {
                let prev_lines_count = self.line_offsets.len();
                let added_bytes = new_size - self.file_size;
                self.bytes_read_since_tick += added_bytes;
                self.file_size = new_size;
                self.last_modified = new_modified;

                let mut appended_only = false;
                if let Ok(mut file) = open_file_shared(&self.path) {
                    let mut prefix = [0u8; 64];
                    let check_len = self.buffer.len().min(64);
                    let is_rewrite =
                        if check_len > 0 && file.read_exact(&mut prefix[..check_len]).is_ok() {
                            prefix[..check_len] != self.buffer[..check_len]
                        } else {
                            false
                        };

                    if is_rewrite {
                        self.buffer.clear();
                        let _ = file.seek(SeekFrom::Start(0));
                        let _ = file.read_to_end(&mut self.buffer);
                    } else if file.seek(SeekFrom::Start(self.buffer.len() as u64)).is_ok() {
                        let _ = file.read_to_end(&mut self.buffer);
                        appended_only = true;
                    } else {
                        self.buffer.clear();
                        let _ = file.read_to_end(&mut self.buffer);
                    }
                    // file is dropped and closed immediately!
                }
                self.has_new_data = true;
                // On a pure append every previously complete line is unchanged; the last line
                // may have been partial, so it is re-evaluated together with the new ones.
                let unchanged_lines = if appended_only {
                    prev_lines_count.saturating_sub(1)
                } else {
                    0
                };
                self.rebuild_line_index_from(unchanged_lines);
                self.check_sound_alerts(prev_lines_count);
                return;
            }

            // In-place modification where size remains identical but timestamp changed
            if new_modified != self.last_modified {
                self.last_modified = new_modified;
                if let Ok(mut file) = open_file_shared(&self.path) {
                    self.buffer.clear();
                    let _ = file.read_to_end(&mut self.buffer);
                    // file is dropped and closed immediately!
                }
                self.has_new_data = true;
                self.rebuild_line_index();
            }
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
        let mmap = &self.buffer;
        if mmap.is_empty() {
            return None;
        }
        let start = self.line_offsets[idx] as usize;
        let next_start = if idx + 1 < self.line_offsets.len() {
            self.line_offsets[idx + 1] as usize
        } else {
            mmap.len()
        };

        if start > next_start || next_start > mmap.len() {
            return None;
        }

        match self.encoding {
            FileEncoding::Utf8 => {
                let mut end = next_start;
                if end > start && mmap.get(end - 1) == Some(&b'\n') {
                    end -= 1;
                    if end > start && mmap.get(end - 1) == Some(&b'\r') {
                        end -= 1;
                    }
                }
                let slice = &mmap[start..end];
                match std::str::from_utf8(slice) {
                    Ok(s) => Some(Cow::Borrowed(s)),
                    Err(_) => Some(Cow::Owned(String::from_utf8_lossy(slice).into_owned())),
                }
            }
            FileEncoding::Ascii => {
                let mut end = next_start;
                if end > start && mmap.get(end - 1) == Some(&b'\n') {
                    end -= 1;
                    if end > start && mmap.get(end - 1) == Some(&b'\r') {
                        end -= 1;
                    }
                }
                let slice = &mmap[start..end];
                let mut s = String::with_capacity(slice.len());
                for &b in slice {
                    if b <= 127 {
                        s.push(b as char);
                    } else {
                        s.push('?');
                    }
                }
                Some(Cow::Owned(s))
            }
            FileEncoding::Ansi => {
                let mut end = next_start;
                if end > start && mmap.get(end - 1) == Some(&b'\n') {
                    end -= 1;
                    if end > start && mmap.get(end - 1) == Some(&b'\r') {
                        end -= 1;
                    }
                }
                let slice = &mmap[start..end];
                let mut s = String::with_capacity(slice.len());
                for &b in slice {
                    s.push(b as char);
                }
                Some(Cow::Owned(s))
            }
            FileEncoding::UnicodeLe => {
                let mut end = next_start;
                if end >= start + 2 && mmap[end - 2] == 0x0A && mmap[end - 1] == 0x00 {
                    end -= 2;
                    if end >= start + 2 && mmap[end - 2] == 0x0D && mmap[end - 1] == 0x00 {
                        end -= 2;
                    }
                }
                let slice = &mmap[start..end];
                let u16_iter = slice
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes([c[0], c[1]]));
                let s = char::decode_utf16(u16_iter)
                    .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect::<String>();
                Some(Cow::Owned(s))
            }
            FileEncoding::UnicodeBe => {
                let mut end = next_start;
                if end >= start + 2 && mmap[end - 2] == 0x00 && mmap[end - 1] == 0x0A {
                    end -= 2;
                    if end >= start + 2 && mmap[end - 2] == 0x00 && mmap[end - 1] == 0x0D {
                        end -= 2;
                    }
                }
                let slice = &mmap[start..end];
                let u16_iter = slice
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes([c[0], c[1]]));
                let s = char::decode_utf16(u16_iter)
                    .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect::<String>();
                Some(Cow::Owned(s))
            }
        }
    }

    pub fn matches_filter(&self, line: &str) -> bool {
        // Exclude check first
        if !self.exclude_filter.is_empty() {
            let matches_exclude = if self.filter_is_regex {
                if let Some(ref re) = self.exclude_regex {
                    re.is_match(line)
                } else {
                    false
                }
            } else if self.filter_case_sensitive {
                line.contains(&self.exclude_filter)
            } else {
                contains_case_insensitive(line, &self.exclude_filter_lower)
            };
            if matches_exclude {
                return false;
            }
        }

        // Include check
        if !self.include_filter.is_empty() {
            if self.filter_is_regex {
                if let Some(ref re) = self.include_regex {
                    re.is_match(line)
                } else {
                    false
                }
            } else if self.filter_case_sensitive {
                line.contains(&self.include_filter)
            } else {
                contains_case_insensitive(line, &self.include_filter_lower)
            }
        } else {
            true
        }
    }

    pub fn match_highlight(&self, line: &str) -> Option<HighlightStyle> {
        for (re_opt, pat_lower, rule) in &self.compiled_highlights {
            if !rule.enabled {
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
                let fg = Color32::from_rgb(rule.fg_color[0], rule.fg_color[1], rule.fg_color[2]);
                let bg = Color32::from_rgb(rule.bg_color[0], rule.bg_color[1], rule.bg_color[2]);
                return Some(HighlightStyle {
                    fg,
                    bg,
                    bold: rule.bold,
                    italic: rule.italic,
                });
            }
        }
        None
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
        !self.include_filter.is_empty() || !self.exclude_filter.is_empty()
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
            self.filtered_lines.clear();
            return;
        }
        let total = self.total_lines();
        let start = start.min(total);
        if start == 0 {
            self.filtered_lines.clear();
        } else {
            let keep = self.filtered_lines.partition_point(|&idx| idx < start);
            self.filtered_lines.truncate(keep);
        }
        let mut fresh = Vec::new();
        for idx in start..total {
            if self.is_line_visible(idx) {
                fresh.push(idx);
            }
        }
        self.filtered_lines.extend(fresh);
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
        if let Some(line) = self.get_line(idx) {
            // Must not match exclude filter if set
            if !self.exclude_filter.is_empty() {
                let matches_exclude = if self.filter_is_regex {
                    if let Some(ref re) = self.exclude_regex {
                        re.is_match(&line)
                    } else {
                        false
                    }
                } else if self.filter_case_sensitive {
                    line.contains(&self.exclude_filter)
                } else {
                    contains_case_insensitive(&line, &self.exclude_filter_lower)
                };
                if matches_exclude {
                    return false;
                }
            }

            // Must match the include filter when one is set (highlight rules never bypass it)
            let matches_include = if !self.include_filter.is_empty() {
                if self.filter_is_regex {
                    if let Some(ref re) = self.include_regex {
                        re.is_match(&line)
                    } else {
                        false
                    }
                } else if self.filter_case_sensitive {
                    line.contains(&self.include_filter)
                } else {
                    contains_case_insensitive(&line, &self.include_filter_lower)
                }
            } else {
                true
            };

            if matches_include {
                return true;
            }

            // Also check multiline stack trace continuation
            if Self::is_stacktrace_continuation(&line) {
                let mut curr = idx;
                while curr > 0 {
                    curr -= 1;
                    if let Some(parent) = self.get_line(curr) {
                        if !Self::is_stacktrace_continuation(&parent) {
                            return self.is_line_visible(curr);
                        }
                    }
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

        let check_match = |line: &str| -> bool {
            contains_case_insensitive(line, &q_lower)
        };

        if self.is_filter_active() {
            let first = self.filtered_lines.partition_point(|&idx| idx < start);
            for &idx in &self.filtered_lines[first..] {
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
            for i in start..self.total_lines() {
                if let Some(line) = self.get_line(i) {
                    if check_match(&line) {
                        matches.push(i);
                        if matches.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        matches
    }

    /// Re-runs the active search over lines `>= start` after the buffer or the filter
    /// changed, keeping the current match on the same line whenever it still matches.
    fn refresh_search_from(&mut self, start: usize) {
        if self.last_searched_query.is_empty() {
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
        if trimmed.is_empty() || limit == 0 || start >= self.buffer.len() {
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

        let haystack = &self.buffer[start..];
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
                    matches.push((start + i, pattern.len()));
                    if matches.len() >= limit {
                        break;
                    }
                }
            }
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
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
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
        let raw = String::from_utf8_lossy(&self.buffer);
        let text = if crate::html_converter::contains_html(&raw) {
            crate::html_converter::html_to_markdown(&raw)
        } else {
            raw.into_owned()
        };
        self.markdown_text_cache = Some((generation, text));
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

    pub fn current_search_line(&self) -> Option<usize> {
        self.current_match_idx
            .and_then(|idx| self.search_matches.get(idx).copied())
    }
}
