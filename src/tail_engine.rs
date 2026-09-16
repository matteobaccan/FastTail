use crate::audio::SoundAlertPreset;
use egui::Color32;
use memmap2::Mmap;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    Text,
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
        bold: bool,
        italic: bool,
        sound_alert: SoundAlertPreset,
    ) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex,
            fg_color: fg,
            bg_color: bg,
            bold,
            italic,
            sound_alert,
            enabled: true,
        }
    }
}

pub struct TailEngine {
    pub path: PathBuf,
    file: File,
    mmap: Option<Mmap>,
    pub line_offsets: Vec<u64>,
    pub file_size: u64,
    pub last_modified: Option<std::time::SystemTime>,
    pub follow_tail: bool,
    pub is_watching: bool,
    pub view_mode: ViewMode,
    pub encoding: FileEncoding,
    pub include_filter: String,
    pub exclude_filter: String,
    include_regex: Option<Regex>,
    exclude_regex: Option<Regex>,
    pub highlight_rules: Vec<HighlightRule>,
    compiled_highlights: Vec<(Option<Regex>, HighlightRule)>,
    pub expanded_json_lines: HashSet<usize>,
    pub requested_scroll_x: Option<f32>,
    pub requested_scroll_y: Option<f32>,
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
        let file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();
        let last_modified = metadata.modified().ok();

        let mmap = if file_size > 0 {
            Some(unsafe { Mmap::map(&file)? })
        } else {
            None
        };

        let (detected_encoding, is_binary) = if let Some(ref m) = mmap {
            if m.starts_with(&[0xEF, 0xBB, 0xBF]) {
                (FileEncoding::Utf8, false)
            } else if m.starts_with(&[0xFF, 0xFE]) {
                (FileEncoding::UnicodeLe, false)
            } else if m.starts_with(&[0xFE, 0xFF]) {
                (FileEncoding::UnicodeBe, false)
            } else {
                let sample_len = m.len().min(512);
                let sample = &m[..sample_len];
                if sample.len() >= 4 && sample.iter().step_by(2).all(|&b| b != 0) && sample.iter().skip(1).step_by(2).all(|&b| b == 0) {
                    (FileEncoding::UnicodeLe, false)
                } else if sample.len() >= 4 && sample.iter().step_by(2).all(|&b| b == 0) && sample.iter().skip(1).step_by(2).all(|&b| b != 0) {
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

        let view_mode = if is_binary {
            ViewMode::Hex
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
            file,
            mmap,
            line_offsets: Vec::new(),
            file_size,
            last_modified,
            follow_tail: true,
            is_watching: true,
            view_mode,
            encoding: detected_encoding,
            include_filter: String::new(),
            exclude_filter: String::new(),
            include_regex: None,
            exclude_regex: None,
            highlight_rules: Vec::new(),
            compiled_highlights: Vec::new(),
            expanded_json_lines: HashSet::new(),
            requested_scroll_x: None,
            requested_scroll_y: None,
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
        let mmap = self.mmap.as_ref()?;
        if offset >= mmap.len() {
            return None;
        }
        let end = (offset + len).min(mmap.len());
        Some(&mmap[offset..end])
    }

    pub fn total_hex_rows(&self, bytes_per_row: usize) -> usize {
        if bytes_per_row == 0 || self.file_size == 0 {
            return 0;
        }
        ((self.file_size as usize) + bytes_per_row - 1) / bytes_per_row
    }

    pub fn release_mmap(&mut self) {
        self.mmap = None;
    }

    pub fn set_highlight_rules(&mut self, rules: Vec<HighlightRule>) {
        self.compiled_highlights = rules
            .iter()
            .map(|r| {
                let re = if r.is_regex {
                    Regex::new(&r.pattern).ok()
                } else {
                    None
                };
                (re, r.clone())
            })
            .collect();
        self.highlight_rules = rules;
    }

    pub fn set_include_filter(&mut self, filter: &str) {
        self.include_filter = filter.to_string();
        self.include_regex = if filter.is_empty() {
            None
        } else {
            Regex::new(filter).ok()
        };
    }

    pub fn set_exclude_filter(&mut self, filter: &str) {
        self.exclude_filter = filter.to_string();
        self.exclude_regex = if filter.is_empty() {
            None
        } else {
            Regex::new(filter).ok()
        };
    }

    pub fn set_encoding(&mut self, encoding: FileEncoding) {
        self.encoding = encoding;
        self.rebuild_line_index();
    }

    pub fn rebuild_line_index(&mut self) {
        self.line_offsets.clear();
        let mmap = match self.mmap {
            Some(ref m) if !m.is_empty() => m,
            _ => return,
        };

        match self.encoding {
            FileEncoding::Utf8 | FileEncoding::Ascii | FileEncoding::Ansi => {
                let start_offset = if self.encoding == FileEncoding::Utf8 && mmap.starts_with(&[0xEF, 0xBB, 0xBF]) {
                    3
                } else {
                    0
                };
                if start_offset < mmap.len() {
                    self.line_offsets.push(start_offset as u64);
                }
                for (i, &byte) in mmap.iter().enumerate().skip(start_offset) {
                    if byte == b'\n' && i + 1 < mmap.len() {
                        self.line_offsets.push((i + 1) as u64);
                    }
                }
            }
            FileEncoding::UnicodeLe => {
                let start_offset = if mmap.starts_with(&[0xFF, 0xFE]) { 2 } else { 0 };
                if start_offset < mmap.len() {
                    self.line_offsets.push(start_offset as u64);
                }
                let mut i = start_offset;
                while i + 1 < mmap.len() {
                    if mmap[i] == 0x0A && mmap[i + 1] == 0x00 {
                        if i + 2 < mmap.len() {
                            self.line_offsets.push((i + 2) as u64);
                        }
                    }
                    i += 2;
                }
            }
            FileEncoding::UnicodeBe => {
                let start_offset = if mmap.starts_with(&[0xFE, 0xFF]) { 2 } else { 0 };
                if start_offset < mmap.len() {
                    self.line_offsets.push(start_offset as u64);
                }
                let mut i = start_offset;
                while i + 1 < mmap.len() {
                    if mmap[i] == 0x00 && mmap[i + 1] == 0x0A {
                        if i + 2 < mmap.len() {
                            self.line_offsets.push((i + 2) as u64);
                        }
                    }
                    i += 2;
                }
            }
        }
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
            self.last_modified = metadata.modified().ok();

            if new_size != self.file_size {
                if let Ok(new_file) = File::open(&self.path) {
                    self.file = new_file;
                }
            }

            if new_size < self.file_size {
                // File was truncated or rotated! Reset completely.
                self.file_size = new_size;
                self.mmap = if new_size > 0 {
                    unsafe { Mmap::map(&self.file).ok() }
                } else {
                    None
                };
                self.rebuild_line_index();
                return;
            }

            if new_size > self.file_size {
                let prev_lines_count = self.line_offsets.len();
                let added_bytes = new_size - self.file_size;
                self.bytes_read_since_tick += added_bytes;
                self.file_size = new_size;

                // Remap to include new bytes
                if let Ok(new_mmap) = unsafe { Mmap::map(&self.file) } {
                    self.mmap = Some(new_mmap);
                    self.rebuild_line_index();
                    self.check_sound_alerts(prev_lines_count);
                }
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
                for (re_opt, rule) in &self.compiled_highlights {
                    if rule.enabled && rule.sound_alert != SoundAlertPreset::None {
                        let is_match = if let Some(re) = re_opt {
                            re.is_match(&line)
                        } else {
                            line.contains(&rule.pattern)
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
        let mmap = self.mmap.as_ref()?;
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
                let u16_iter = slice.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
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
                let u16_iter = slice.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]]));
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
            if let Some(ref re) = self.exclude_regex {
                if re.is_match(line) {
                    return false;
                }
            } else if line.contains(&self.exclude_filter) {
                return false;
            }
        }

        // Include check
        if !self.include_filter.is_empty() {
            if let Some(ref re) = self.include_regex {
                re.is_match(line)
            } else {
                line.contains(&self.include_filter)
            }
        } else {
            true
        }
    }

    pub fn match_highlight(&self, line: &str) -> Option<HighlightStyle> {
        for (re_opt, rule) in &self.compiled_highlights {
            if !rule.enabled {
                continue;
            }
            let is_match = if let Some(re) = re_opt {
                re.is_match(line)
            } else {
                line.contains(&rule.pattern)
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

    pub fn is_line_visible(&self, idx: usize) -> bool {
        if self.include_filter.is_empty() && self.exclude_filter.is_empty() {
            return true;
        }
        if let Some(line) = self.get_line(idx) {
            if self.matches_filter(&line) {
                return true;
            }
            // Multiline stack trace grouping: check if parent line matched
            if Self::is_stacktrace_continuation(&line) {
                let mut curr = idx;
                while curr > 0 {
                    curr -= 1;
                    if let Some(parent) = self.get_line(curr) {
                        if !Self::is_stacktrace_continuation(&parent) {
                            return self.matches_filter(&parent);
                        }
                    }
                }
            }
        }
        false
    }

    pub fn find_matches(&self, query: &str) -> Vec<usize> {
        let mut matches = Vec::new();
        if query.is_empty() {
            return matches;
        }
        let q_lower = query.to_lowercase();
        for i in 0..self.total_lines() {
            if let Some(line) = self.get_line(i) {
                if line.to_lowercase().contains(&q_lower) {
                    matches.push(i);
                }
            }
        }
        matches
    }
}
