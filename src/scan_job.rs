//! Background scans over a log file: indexing, filtering and searching on a worker thread.
//!
//! A job owns its own file handle, streams the file in 1 MB chunks, splits lines with the
//! same rules as the engine's index, and sends ordered batches through a channel. Every
//! message carries the job generation so the engine can drop results of a job it has
//! already replaced; the worker also checks a cancel flag after every chunk.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use regex::Regex;

use crate::file_source::FileSource;
use crate::log_level::{detect_level, LogLevel};
use crate::tail_engine::{contains_case_insensitive, decode_line, FileEncoding};

const CHUNK: usize = 1024 * 1024;
/// Hits accumulated before a batch is sent (keeps the channel traffic low).
const BATCH_HITS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanKind {
    Index,
    Filter,
    Search,
    /// Detects the log level of every line (fills the engine's per-line level cache).
    Levels,
}

/// The include/exclude filter, compiled once and shareable with a worker thread.
#[derive(Debug, Clone, Default)]
pub struct FilterSpec {
    pub include: String,
    pub exclude: String,
    pub include_lower: String,
    pub exclude_lower: String,
    pub case_sensitive: bool,
    pub is_regex: bool,
    pub include_regex: Option<Regex>,
    pub exclude_regex: Option<Regex>,
    /// Minimum level a line must have to be visible; `Unknown` = no level filter.
    pub min_level: LogLevel,
    /// With a minimum level set, whether lines without a detectable level are shown.
    pub show_unknown_levels: bool,
}

impl FilterSpec {
    pub fn build(include: &str, exclude: &str, case_sensitive: bool, is_regex: bool) -> Self {
        let compile = |p: &str| {
            if p.is_empty() {
                None
            } else {
                regex::RegexBuilder::new(p)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .ok()
            }
        };
        Self {
            include: include.to_string(),
            exclude: exclude.to_string(),
            include_lower: include.to_lowercase(),
            exclude_lower: exclude.to_lowercase(),
            case_sensitive,
            is_regex,
            include_regex: if is_regex { compile(include) } else { None },
            exclude_regex: if is_regex { compile(exclude) } else { None },
            min_level: LogLevel::Unknown,
            show_unknown_levels: false,
        }
    }

    /// Adds the minimum-level stage (see `level_passes`).
    pub fn with_levels(mut self, min_level: LogLevel, show_unknown_levels: bool) -> Self {
        self.min_level = min_level;
        self.show_unknown_levels = show_unknown_levels;
        self
    }

    pub fn is_active(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty() || self.min_level != LogLevel::Unknown
    }

    /// Third stage after exclude and include: the line's detected level must reach
    /// `min_level`. Lines without a level pass when the threshold is off or `TRACE`, or
    /// when `show_unknown_levels` is set.
    pub fn level_passes(&self, line: &str) -> bool {
        if self.min_level == LogLevel::Unknown {
            return true;
        }
        match detect_level(line) {
            LogLevel::Unknown => self.show_unknown_levels || self.min_level == LogLevel::Trace,
            level => level >= self.min_level,
        }
    }

    /// True when the exclude filter is set and matches `line`.
    pub fn excluded(&self, line: &str) -> bool {
        if self.exclude.is_empty() {
            return false;
        }
        if self.is_regex {
            self.exclude_regex
                .as_ref()
                .map(|re| re.is_match(line))
                .unwrap_or(false)
        } else if self.case_sensitive {
            line.contains(&self.exclude)
        } else {
            contains_case_insensitive(line, &self.exclude_lower)
        }
    }

    /// True when the include filter is empty or matches `line`.
    pub fn included(&self, line: &str) -> bool {
        if self.include.is_empty() {
            return true;
        }
        if self.is_regex {
            self.include_regex
                .as_ref()
                .map(|re| re.is_match(line))
                .unwrap_or(false)
        } else if self.case_sensitive {
            line.contains(&self.include)
        } else {
            contains_case_insensitive(line, &self.include_lower)
        }
    }

    /// Exclude wins over include; an empty include lets every non-excluded line through;
    /// the minimum level is checked last.
    pub fn matches(&self, line: &str) -> bool {
        !self.excluded(line) && self.included(line) && self.level_passes(line)
    }

    /// Visibility of a line in a sequential scan: a stack trace continuation line that is
    /// not excluded follows its parent (`parent_visible`) unless it matches on its own.
    /// Returns the visibility and the parent state to carry to the next line.
    pub fn visible_in_sequence(&self, line: &str, parent_visible: bool) -> (bool, bool) {
        if crate::tail_engine::TailEngine::is_stacktrace_continuation(line) {
            let v = !self.excluded(line)
                && ((self.included(line) && self.level_passes(line)) || parent_visible);
            (v, parent_visible)
        } else {
            let v = self.matches(line);
            (v, v)
        }
    }
}

/// What a job evaluates on every line.
#[derive(Debug, Clone)]
pub enum JobSpec {
    /// Emit the offset of every line start (and the longest line seen).
    Index,
    /// Emit the indices of the lines that pass the filter.
    Filter(FilterSpec),
    /// Emit the detected level of every line, one byte per line, in order.
    Levels,
    /// Emit the indices of the visible lines containing the query (case-insensitive),
    /// at most `limit`.
    Search {
        query_lower: String,
        filter: Option<FilterSpec>,
        limit: usize,
    },
}

/// Messages from the worker, always tagged with the job generation.
#[derive(Debug)]
pub enum ScanBatch {
    /// Line indices found so far (filter / search), in increasing order.
    Lines(Vec<usize>),
    /// Detected levels (`LogLevel as u8`) of the next lines, in order.
    Levels(Vec<u8>),
    /// Line start offsets (index), in increasing order, plus the longest line in the batch.
    Offsets {
        offsets: Vec<u64>,
        max_line_bytes: usize,
    },
    /// Fraction of the byte range scanned so far.
    Progress(f32),
    /// The range was scanned completely (`lines` = line count covered).
    Done { lines: usize },
    /// The file could not be read; the engine reloads on its next poll.
    Failed,
}

/// Parameters of the byte range to scan.
#[derive(Debug, Clone)]
pub struct ScanRange {
    pub start_offset: u64,
    pub end_offset: u64,
    /// Index of the line that starts at `start_offset`.
    pub start_line: usize,
    pub encoding: FileEncoding,
    /// Visibility of the nearest non-continuation line before `start_line` under the filter.
    pub parent_visible: bool,
}

pub struct ScanJob {
    pub kind: ScanKind,
    pub generation: u64,
    pub range: ScanRange,
    pub progress: f32,
    pub hits: usize,
    rx: Receiver<(u64, ScanBatch)>,
    cancel: Arc<AtomicBool>,
}

impl ScanJob {
    /// Starts the worker thread.
    pub fn spawn(generation: u64, path: &std::path::Path, range: ScanRange, spec: JobSpec) -> Self {
        let kind = match spec {
            JobSpec::Index => ScanKind::Index,
            JobSpec::Filter(_) => ScanKind::Filter,
            JobSpec::Search { .. } => ScanKind::Search,
            JobSpec::Levels => ScanKind::Levels,
        };
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker_path = path.to_path_buf();
        let worker_range = range.clone();
        thread::Builder::new()
            .name(format!("fasttail-scan-{generation}"))
            .spawn(move || {
                run(
                    generation,
                    worker_path,
                    worker_range,
                    spec,
                    tx,
                    worker_cancel,
                )
            })
            .ok();
        Self {
            kind,
            generation,
            range,
            progress: 0.0,
            hits: 0,
            rx,
            cancel,
        }
    }

    /// Non-blocking receive of the next batch of this job.
    pub fn try_recv(&self) -> Option<ScanBatch> {
        loop {
            match self.rx.try_recv() {
                Ok((gen, batch)) if gen == self.generation => return Some(batch),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for ScanJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Worker body: streams the range, splits lines, evaluates the spec, sends batches.
fn run(
    generation: u64,
    path: std::path::PathBuf,
    range: ScanRange,
    spec: JobSpec,
    tx: Sender<(u64, ScanBatch)>,
    cancel: Arc<AtomicBool>,
) {
    let send = |b: ScanBatch| tx.send((generation, b)).is_ok();
    let source = match FileSource::open(&path) {
        Ok(s) => s,
        Err(_) => {
            send(ScanBatch::Failed);
            return;
        }
    };
    let total = range.end_offset.saturating_sub(range.start_offset);
    let char_bytes: usize = match range.encoding {
        FileEncoding::UnicodeLe | FileEncoding::UnicodeBe => 2,
        _ => 1,
    };
    let (nl_lo, nl_hi) = match range.encoding {
        FileEncoding::UnicodeLe => (0x0A, 0x00),
        FileEncoding::UnicodeBe => (0x00, 0x0A),
        _ => (b'\n', 0),
    };

    let mut chunk = vec![0u8; CHUNK];
    let mut carry: Vec<u8> = Vec::new(); // bytes of the current line seen in earlier chunks
    let mut pos = range.start_offset;
    let mut line_idx = range.start_line;
    let mut hits: Vec<usize> = Vec::new();
    let mut levels: Vec<u8> = Vec::new();
    let mut offsets: Vec<u64> = Vec::new();
    let mut max_line_bytes = 0usize;
    let mut limit_reached = false;

    if matches!(spec, JobSpec::Index) && range.start_offset < range.end_offset {
        offsets.push(range.start_offset);
    }

    // Evaluates one complete line (without its newline) at `idx`.
    // Visibility of the previous non-continuation line (stack trace continuation lines
    // follow their parent); a job starting after line 0 receives it from the engine.
    let mut parent_visible = range.parent_visible;
    let mut eval =
        |idx: usize, bytes: &[u8], hits: &mut Vec<usize>, levels: &mut Vec<u8>| -> bool {
            match &spec {
                JobSpec::Index => true,
                JobSpec::Levels => {
                    line_passes(bytes, range.encoding, |s| {
                        levels.push(detect_level(s) as u8);
                        true
                    });
                    true
                }
                JobSpec::Filter(filter) => {
                    let visible = line_passes(bytes, range.encoding, |s| {
                        let (v, next) = filter.visible_in_sequence(s, parent_visible);
                        parent_visible = next;
                        v
                    });
                    if visible {
                        hits.push(idx);
                    }
                    true
                }
                JobSpec::Search {
                    query_lower,
                    filter,
                    limit,
                } => {
                    let hit = line_passes(bytes, range.encoding, |s| {
                        let visible = match filter {
                            Some(f) => {
                                let (v, next) = f.visible_in_sequence(s, parent_visible);
                                parent_visible = next;
                                v
                            }
                            None => true,
                        };
                        visible && contains_case_insensitive(s, query_lower)
                    });
                    if hit {
                        hits.push(idx);
                        if hits.len() >= *limit {
                            return false;
                        }
                    }
                    true
                }
            }
        };

    'outer: while pos < range.end_offset {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let want = ((range.end_offset - pos) as usize).min(chunk.len());
        let n = match source.read_direct(pos, &mut chunk[..want]) {
            Ok(n) if n > 0 => n & !(char_bytes - 1),
            _ => {
                send(ScanBatch::Failed);
                return;
            }
        };
        if n == 0 {
            break;
        }
        let data = &chunk[..n];
        let mut line_start = 0usize; // within `data`
        let mut k = 0usize;
        while k + char_bytes <= n {
            let is_newline = if char_bytes == 1 {
                data[k] == nl_lo
            } else {
                data[k] == nl_lo && data[k + 1] == nl_hi
            };
            if is_newline {
                let abs_newline = pos + k as u64;
                let content_len = carry.len() + (k - line_start);
                let line_bytes: std::borrow::Cow<[u8]> = if carry.is_empty() {
                    std::borrow::Cow::Borrowed(&data[line_start..k])
                } else {
                    let mut v = std::mem::take(&mut carry);
                    v.extend_from_slice(&data[line_start..k]);
                    std::borrow::Cow::Owned(v)
                };
                if content_len / char_bytes > max_line_bytes {
                    max_line_bytes = content_len / char_bytes;
                }
                if !eval(line_idx, &line_bytes, &mut hits, &mut levels) {
                    limit_reached = true;
                    break 'outer;
                }
                line_idx += 1;
                if abs_newline + char_bytes as u64 >= range.end_offset {
                    // A newline at the very end does not start another line.
                    line_start = n;
                } else {
                    line_start = k + char_bytes;
                    if matches!(spec, JobSpec::Index) {
                        offsets.push(abs_newline + char_bytes as u64);
                    }
                }
                k += char_bytes;
                continue;
            }
            k += char_bytes;
        }
        if line_start < n {
            carry.extend_from_slice(&data[line_start..n]);
        }
        pos += n as u64;

        if matches!(spec, JobSpec::Index) {
            if !offsets.is_empty()
                && !send(ScanBatch::Offsets {
                    offsets: std::mem::take(&mut offsets),
                    max_line_bytes,
                })
            {
                return;
            }
        } else if matches!(spec, JobSpec::Levels) {
            if !levels.is_empty() && !send(ScanBatch::Levels(std::mem::take(&mut levels))) {
                return;
            }
        } else if hits.len() >= BATCH_HITS && !send(ScanBatch::Lines(std::mem::take(&mut hits))) {
            return;
        }
        let progress = if total == 0 {
            1.0
        } else {
            (pos - range.start_offset) as f32 / total as f32
        };
        if !send(ScanBatch::Progress(progress)) {
            return;
        }
    }

    // The final line without a trailing newline.
    if !limit_reached && !carry.is_empty() {
        let len = carry.len() / char_bytes;
        if len > max_line_bytes {
            max_line_bytes = len;
        }
        let bytes = std::mem::take(&mut carry);
        eval(line_idx, &bytes, &mut hits, &mut levels);
        line_idx += 1;
    }
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    if matches!(spec, JobSpec::Index) {
        if !offsets.is_empty()
            && !send(ScanBatch::Offsets {
                offsets: std::mem::take(&mut offsets),
                max_line_bytes,
            })
        {
            return;
        }
    } else {
        let last = if matches!(spec, JobSpec::Levels) {
            (!levels.is_empty()).then(|| ScanBatch::Levels(std::mem::take(&mut levels)))
        } else {
            (!hits.is_empty()).then(|| ScanBatch::Lines(std::mem::take(&mut hits)))
        };
        if let Some(batch) = last {
            if !send(batch) {
                return;
            }
        }
    }
    send(ScanBatch::Done {
        lines: line_idx - range.start_line,
    });
}

/// Decodes a raw line (newline already stripped) and applies `pred`, borrowing UTF-8 text
/// when it is valid.
fn line_passes(bytes: &[u8], encoding: FileEncoding, pred: impl FnOnce(&str) -> bool) -> bool {
    match encoding {
        FileEncoding::Utf8 => {
            let content = &bytes[..trim_cr(bytes)];
            match std::str::from_utf8(content) {
                Ok(s) => pred(s),
                Err(_) => pred(&String::from_utf8_lossy(content)),
            }
        }
        enc => pred(&decode_line(bytes, enc, false)),
    }
}

fn trim_cr(bytes: &[u8]) -> usize {
    if bytes.last() == Some(&b'\r') {
        bytes.len() - 1
    } else {
        bytes.len()
    }
}
