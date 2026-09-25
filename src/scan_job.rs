//! Background scans over a log file: indexing, filtering, searching and the per-line level
//! and timestamp caches, on a worker thread.
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

use crate::ansi::AnsiMode;
use crate::file_source::FileSource;
use crate::log_level::{detect_level, LogLevel};
use crate::tail_engine::{contains_case_insensitive, decode_line_ansi, FileEncoding, NO_TIMESTAMP};
use crate::timestamp::{detect_timestamp, FormatHint};

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
    /// Detects the timestamp of every line (fills the engine's per-line timestamp cache).
    Timestamps,
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
    /// Emit the effective timestamp of every line, in order, with the rules of the
    /// engine's `fill_timestamps`: `inherited` is the timestamp of the line before the
    /// range (`NO_TIMESTAMP` at the start of the file) and `hint` the format that matched
    /// last, so a job resumed halfway fills the same cache as one that never stopped.
    Timestamps { inherited: i64, hint: FormatHint },
    /// Emit the indices of the visible lines containing the query (case-insensitive),
    /// at most `limit`; past it the scan stops, or with `count_past_limit` goes on
    /// counting the hits it no longer lists (`ScanBatch::Counted`).
    Search {
        query_lower: String,
        filter: Option<FilterSpec>,
        limit: usize,
        count_past_limit: bool,
    },
}

/// Messages from the worker, always tagged with the job generation.
#[derive(Debug)]
pub enum ScanBatch {
    /// Line indices found so far (filter / search), in increasing order.
    Lines(Vec<usize>),
    /// Search hits past the limit since the last batch: counted, not listed, the last of
    /// them on `last_line`.
    Counted { hits: usize, last_line: usize },
    /// Detected levels (`LogLevel as u8`) of the next lines, in order.
    Levels(Vec<u8>),
    /// Effective timestamps of the next lines, in order, plus how many of them carried a
    /// timestamp of their own, whether one went back in time, and the format hint after
    /// the last of them.
    Timestamps {
        values: Vec<i64>,
        parsed: usize,
        unordered: bool,
        hint: FormatHint,
    },
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
    /// Resolved ANSI mode of the stream: in render and strip modes every line is evaluated
    /// without its escape sequences, exactly like the engine's synchronous path.
    pub ansi: AnsiMode,
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
            JobSpec::Timestamps { .. } => ScanKind::Timestamps,
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

/// Running state of a `Timestamps` job: the values of the current batch, what the batch
/// adds to the engine's counters, and what the next line inherits.
struct Timing {
    values: Vec<i64>,
    parsed: usize,
    unordered: bool,
    inherited: i64,
    hint: FormatHint,
}

impl Timing {
    /// One line, exactly as `TailEngine::fill_timestamps` times it.
    fn push(&mut self, line: &str) {
        if let Some((millis, format)) = detect_timestamp(line, self.hint) {
            self.hint = format;
            self.parsed += 1;
            if self.inherited != NO_TIMESTAMP && millis < self.inherited {
                self.unordered = true;
            }
            self.inherited = millis;
        }
        // No timestamp of its own: it belongs to the entry above it.
        self.values.push(self.inherited);
    }

    /// The batch accumulated since the last one, if any.
    fn take_batch(&mut self) -> Option<ScanBatch> {
        if self.values.is_empty() {
            return None;
        }
        let batch = ScanBatch::Timestamps {
            values: std::mem::take(&mut self.values),
            parsed: self.parsed,
            unordered: self.unordered,
            hint: self.hint,
        };
        self.parsed = 0;
        self.unordered = false;
        Some(batch)
    }
}

/// Running state of a `Search` job: hits listed so far, and the hits past the limit
/// counted since the last `Counted` batch with the last of them.
#[derive(Default)]
struct Tally {
    listed: usize,
    counted: usize,
    last_counted: usize,
}

impl Tally {
    fn take_batch(&mut self) -> ScanBatch {
        let batch = ScanBatch::Counted {
            hits: self.counted,
            last_line: self.last_counted,
        };
        self.counted = 0;
        batch
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
    let mut tally = Tally::default();
    let (inherited, hint) = match &spec {
        JobSpec::Timestamps { inherited, hint } => (*inherited, *hint),
        _ => (NO_TIMESTAMP, FormatHint::default()),
    };
    let mut timing = Timing {
        values: Vec::new(),
        parsed: 0,
        unordered: false,
        inherited,
        hint,
    };

    if matches!(spec, JobSpec::Index) && range.start_offset < range.end_offset {
        offsets.push(range.start_offset);
    }

    // Evaluates one complete line (without its newline) at `idx`.
    // Visibility of the previous non-continuation line (stack trace continuation lines
    // follow their parent); a job starting after line 0 receives it from the engine.
    let mut parent_visible = range.parent_visible;
    let strip = range.ansi.strips();
    let mut eval = |idx: usize,
                    bytes: &[u8],
                    hits: &mut Vec<usize>,
                    levels: &mut Vec<u8>,
                    timing: &mut Timing,
                    tally: &mut Tally|
     -> bool {
        match &spec {
            JobSpec::Index => true,
            JobSpec::Levels => {
                line_passes(bytes, range.encoding, strip, |s| {
                    levels.push(detect_level(s) as u8);
                    true
                });
                true
            }
            JobSpec::Timestamps { .. } => {
                line_passes(bytes, range.encoding, strip, |s| {
                    timing.push(s);
                    true
                });
                true
            }
            JobSpec::Filter(filter) => {
                let visible = line_passes(bytes, range.encoding, strip, |s| {
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
                count_past_limit,
            } => {
                let hit = line_passes(bytes, range.encoding, strip, |s| {
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
                    if tally.listed < *limit {
                        hits.push(idx);
                        tally.listed += 1;
                        if tally.listed >= *limit && !*count_past_limit {
                            return false;
                        }
                    } else {
                        tally.counted += 1;
                        tally.last_counted = idx;
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
                if !eval(
                    line_idx,
                    &line_bytes,
                    &mut hits,
                    &mut levels,
                    &mut timing,
                    &mut tally,
                ) {
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
        } else if matches!(spec, JobSpec::Timestamps { .. }) {
            if let Some(batch) = timing.take_batch() {
                if !send(batch) {
                    return;
                }
            }
        } else if tally.counted > 0 {
            // Past the limit: the last listed hits, then the count, in that order.
            if !hits.is_empty() && !send(ScanBatch::Lines(std::mem::take(&mut hits))) {
                return;
            }
            if !send(tally.take_batch()) {
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
        eval(
            line_idx,
            &bytes,
            &mut hits,
            &mut levels,
            &mut timing,
            &mut tally,
        );
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
        } else if matches!(spec, JobSpec::Timestamps { .. }) {
            timing.take_batch()
        } else {
            (!hits.is_empty()).then(|| ScanBatch::Lines(std::mem::take(&mut hits)))
        };
        if let Some(batch) = last {
            if !send(batch) {
                return;
            }
        }
        if tally.counted > 0 && !send(tally.take_batch()) {
            return;
        }
    }
    send(ScanBatch::Done {
        lines: line_idx - range.start_line,
    });
}

/// Decodes a raw line (newline already stripped), removes its escape sequences when
/// `strip` is set, and applies `pred`, borrowing UTF-8 text when it is valid and holds no
/// sequence.
fn line_passes(
    bytes: &[u8],
    encoding: FileEncoding,
    strip: bool,
    pred: impl FnOnce(&str) -> bool,
) -> bool {
    match encoding {
        FileEncoding::Utf8 => {
            let content = &bytes[..trim_cr(bytes)];
            match std::str::from_utf8(content) {
                Ok(s) if strip => pred(&crate::ansi::strip(s)),
                Ok(s) => pred(s),
                Err(_) => {
                    let s = String::from_utf8_lossy(content);
                    if strip {
                        pred(&crate::ansi::strip(&s))
                    } else {
                        pred(&s)
                    }
                }
            }
        }
        enc => pred(&decode_line_ansi(bytes, enc, false, strip)),
    }
}

fn trim_cr(bytes: &[u8]) -> usize {
    if bytes.last() == Some(&b'\r') {
        bytes.len() - 1
    } else {
        bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn millis(text: &str) -> i64 {
        detect_timestamp(text, FormatHint::default())
            .expect("test timestamp")
            .0
    }

    /// Runs a `Timestamps` job over `bytes[start_offset..]` and gathers what the engine
    /// would append to its cache: values, parsed count, out-of-order flag, final hint.
    fn time_file(
        bytes: &[u8],
        start_offset: u64,
        start_line: usize,
        encoding: FileEncoding,
        inherited: i64,
        hint: FormatHint,
    ) -> (Vec<i64>, usize, bool, FormatHint) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timing.log");
        std::fs::write(&path, bytes).unwrap();
        let range = ScanRange {
            start_offset,
            end_offset: bytes.len() as u64,
            start_line,
            encoding,
            ansi: AnsiMode::Raw,
            parent_visible: false,
        };
        let job = ScanJob::spawn(1, &path, range, JobSpec::Timestamps { inherited, hint });
        assert_eq!(job.kind, ScanKind::Timestamps);
        let (mut values, mut parsed, mut unordered, mut last_hint) = (Vec::new(), 0, false, hint);
        let started = Instant::now();
        loop {
            match job.try_recv() {
                Some(ScanBatch::Timestamps {
                    values: v,
                    parsed: p,
                    unordered: u,
                    hint: h,
                }) => {
                    values.extend(v);
                    parsed += p;
                    unordered |= u;
                    last_hint = h;
                }
                Some(ScanBatch::Done { lines }) => {
                    assert_eq!(lines, values.len(), "one value per line");
                    return (values, parsed, unordered, last_hint);
                }
                Some(ScanBatch::Failed) => panic!("timing job failed"),
                Some(_) => {}
                None => {
                    assert!(started.elapsed() < Duration::from_secs(20), "job hangs");
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }

    #[test]
    fn timing_job_reads_every_format_and_inherits_on_continuation_lines() {
        let epoch = millis("2026-09-18T14:04:00.000Z");
        let text = format!(
            "==== banner ====\n\
             2026-09-18T14:02:05.123Z ERROR boom\n\
             \x20   at Foo.bar(Foo.java:10)\n\
             [18/Sep/2026:14:03:00 +0200] \"GET / HTTP/1.1\" 200\n\
             {epoch} epoch millis\n\
             2026-09-18T14:01:00.000Z INFO back in time\n\
             no newline at the end"
        );
        let (values, parsed, unordered, hint) = time_file(
            text.as_bytes(),
            0,
            0,
            FileEncoding::Utf8,
            NO_TIMESTAMP,
            FormatHint::default(),
        );
        let boom = millis("2026-09-18T14:02:05.123Z");
        let back = millis("2026-09-18T14:01:00.000Z");
        assert_eq!(
            values,
            vec![
                NO_TIMESTAMP,
                boom,
                boom,
                millis("[18/Sep/2026:14:03:00 +0200]"),
                epoch,
                back,
                back,
            ]
        );
        assert_eq!(parsed, 4, "banner, stack frame and last line carry none");
        assert!(unordered, "14:01 after 14:04 goes back in time");
        assert_eq!(hint, FormatHint::Iso8601);
    }

    #[test]
    fn timing_job_resumed_mid_file_continues_the_prefix() {
        let text =
            "2026-09-18T14:00:00.000Z first\n    at a\n    at b\n2026-09-18T14:05:00.000Z next\n";
        // Start at line 2 ("    at b"), as a job resumed after line 1 would.
        let start_offset = text.find("    at b").unwrap() as u64;
        let inherited = millis("2026-09-18T14:00:00.000Z");
        let (values, parsed, unordered, hint) = time_file(
            text.as_bytes(),
            start_offset,
            2,
            FileEncoding::Utf8,
            inherited,
            FormatHint::Iso8601,
        );
        assert_eq!(values, vec![inherited, millis("2026-09-18T14:05:00.000Z")]);
        assert_eq!(parsed, 1);
        assert!(!unordered);
        assert_eq!(hint, FormatHint::Iso8601);

        // A value inherited from the prefix still counts for the out-of-order check.
        let later = millis("2026-09-18T15:00:00.000Z");
        let (_, _, unordered, _) = time_file(
            text.as_bytes(),
            start_offset,
            2,
            FileEncoding::Utf8,
            later,
            FormatHint::Iso8601,
        );
        assert!(unordered);
    }

    #[test]
    fn timing_job_decodes_utf16() {
        let text = "2026-09-18T14:02:00.000Z INFO a\r\n    continuation\r\n2026-09-18T14:03:00.000Z INFO b\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let (values, parsed, _, _) = time_file(
            &bytes,
            2,
            0,
            FileEncoding::UnicodeLe,
            NO_TIMESTAMP,
            FormatHint::default(),
        );
        let a = millis("2026-09-18T14:02:00.000Z");
        assert_eq!(values, vec![a, a, millis("2026-09-18T14:03:00.000Z")]);
        assert_eq!(parsed, 2);
    }

    #[test]
    fn cancelled_timing_job_never_reports_done() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let line = "2026-09-18T14:02:00.000Z INFO a line of padding to fill the chunks\n";
        std::fs::write(&path, line.repeat(8 * 1024 * 1024 / line.len())).unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        let range = ScanRange {
            start_offset: 0,
            end_offset: len,
            start_line: 0,
            encoding: FileEncoding::Utf8,
            ansi: AnsiMode::Raw,
            parent_visible: false,
        };
        let job = ScanJob::spawn(
            7,
            &path,
            range,
            JobSpec::Timestamps {
                inherited: NO_TIMESTAMP,
                hint: FormatHint::default(),
            },
        );
        job.cancel();
        // The flag is checked before every 1 MB chunk: at most one of the eight is read.
        let started = Instant::now();
        let mut timed = 0usize;
        while started.elapsed() < Duration::from_millis(500) {
            match job.try_recv() {
                Some(ScanBatch::Done { .. }) => panic!("a cancelled job must not complete"),
                Some(ScanBatch::Timestamps { values, .. }) => timed += values.len(),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        assert!(timed < (len as usize) / line.len());
    }
}
