// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Split file: cuts a stream into pieces every N lines, every N bytes (at a line end) or
//! every time span (from the line timestamps), written next to the file or in a chosen
//! folder as `app.part-001.log`, ... or `app.2026-10-05T14.log`. The job runs a few
//! milliseconds per frame over the stream's lines (all of them, or the visible ones) and
//! writes one piece at a time through a buffered writer, so nothing is held in memory.
//! A growing file is split as far as it was when the job started.

use crate::tail_engine::{TailEngine, NO_TIMESTAMP};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How the pieces are cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitBy {
    /// Every N lines.
    Lines(u64),
    /// Every N bytes, cut at the last line end before the limit (a longer line is a
    /// piece of its own).
    Bytes(u64),
    /// Every span of time, in milliseconds, from the line timestamps (aligned on the
    /// span: whole hours, whole days).
    Time(i64),
}

/// What to split and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPlan {
    pub by: SplitBy,
    /// Only the lines the filters let through, as Export does.
    pub visible_only: bool,
    /// The folder the pieces go to.
    pub dir: PathBuf,
    /// The source file name before and after the dot of its extension (`app`, `.log`).
    pub stem: String,
    pub ext: String,
}

impl SplitPlan {
    /// A plan for the file `source`, the pieces next to it.
    pub fn for_file(source: &Path, by: SplitBy) -> Self {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "split".to_string());
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
            _ => (name.clone(), ".log".to_string()),
        };
        Self {
            by,
            visible_only: false,
            dir: source
                .parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
            stem,
            ext,
        }
    }

    /// The name of piece `n` (from 1) by lines or size, or of the piece starting at
    /// `bucket` (milliseconds) by time.
    pub fn piece_name(&self, n: usize, bucket: Option<i64>) -> String {
        match (self.by, bucket) {
            (SplitBy::Time(span), Some(start)) => {
                format!("{}.{}{}", self.stem, bucket_label(start, span), self.ext)
            }
            _ => format!("{}.part-{n:03}{}", self.stem, self.ext),
        }
    }
}

/// Reads a split typed as text (the terminal's prompt): `100000` lines, `10mb` (or `kb`,
/// `gb`), `1h`, `1d` or `30m` (minutes); a trailing `!` agrees to replace existing files.
pub fn parse_spec(text: &str) -> Option<(SplitBy, bool)> {
    let text = text.trim();
    let (text, replace) = match text.strip_suffix('!') {
        Some(rest) => (rest.trim_end(), true),
        None => (text, false),
    };
    let lower = text.to_ascii_lowercase();
    let digits = lower.chars().take_while(char::is_ascii_digit).count();
    let n: u64 = lower[..digits].parse().ok().filter(|&n| n > 0)?;
    let by = match lower[digits..].trim() {
        "" => SplitBy::Lines(n),
        "kb" => SplitBy::Bytes(n.checked_mul(1024)?),
        "mb" => SplitBy::Bytes(n.checked_mul(1024 * 1024)?),
        "gb" => SplitBy::Bytes(n.checked_mul(1024 * 1024 * 1024)?),
        "m" | "min" => SplitBy::Time(i64::try_from(n).ok()?.checked_mul(60_000)?),
        "h" => SplitBy::Time(i64::try_from(n).ok()?.checked_mul(3_600_000)?),
        "d" => SplitBy::Time(i64::try_from(n).ok()?.checked_mul(86_400_000)?),
        _ => return None,
    };
    Some((by, replace))
}

/// `2026-10-05` for a span of whole days, `2026-10-05T14` for whole hours, else
/// `2026-10-05T14-30` (no `:`, which a Windows file name cannot hold).
pub fn bucket_label(start: i64, span: i64) -> String {
    let text = crate::timestamp::format_millis(start);
    let (date, time) = text.split_once(' ').unwrap_or((&text, "00:00:00"));
    if span % 86_400_000 == 0 {
        date.to_string()
    } else if span % 3_600_000 == 0 {
        format!("{date}T{}", &time[..2])
    } else {
        format!("{date}T{}-{}", &time[..2], &time[3..5])
    }
}

/// The start of the span holding `ts`, aligned on the span.
fn bucket_of(ts: i64, span: i64) -> i64 {
    ts.div_euclid(span) * span
}

/// What the dialog shows before writing: the pieces (at least, for a split by time,
/// which may skip empty spans), the bytes to write, the first names, and the ones that
/// exist already.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SplitPreview {
    pub pieces: usize,
    pub bytes: u64,
    pub names: Vec<String>,
    pub existing: Vec<PathBuf>,
    /// A split by time needs the timestamps, not all read yet.
    pub waiting_for_timestamps: bool,
    /// By time on a stream without timestamps.
    pub no_timestamps: bool,
}

/// Names the preview lists at most (the existing ones are all checked).
const PREVIEW_NAMES: usize = 5;
/// Pieces checked for an existing file at most.
const MAX_CHECKED: usize = 10_000;

/// The preview of `plan` on `engine`.
pub fn preview(engine: &TailEngine, plan: &SplitPlan) -> SplitPreview {
    let (lines, bytes) = if plan.visible_only {
        let n = engine.visible_lines();
        // The bytes of the visible lines would need reading them: in proportion.
        let total = engine.total_lines().max(1);
        (n, engine.file_size * n as u64 / total as u64)
    } else {
        (engine.total_lines(), engine.file_size)
    };
    let mut out = SplitPreview {
        bytes,
        ..SplitPreview::default()
    };
    let buckets: Vec<Option<i64>> = match plan.by {
        SplitBy::Lines(n) => {
            out.pieces = lines.div_ceil(n.max(1) as usize);
            vec![None; out.pieces.min(MAX_CHECKED)]
        }
        SplitBy::Bytes(n) => {
            out.pieces = (bytes.div_ceil(n.max(1))) as usize;
            vec![None; out.pieces.min(MAX_CHECKED)]
        }
        SplitBy::Time(span) => {
            if !engine.timestamps_complete() {
                out.waiting_for_timestamps = true;
            }
            match (engine.first_timestamp(), engine.last_timestamp()) {
                (Some(first), Some(last)) if span > 0 => {
                    let (a, b) = (
                        bucket_of(first.min(last), span),
                        bucket_of(first.max(last), span),
                    );
                    out.pieces = ((b - a) / span + 1) as usize;
                    (0..out.pieces.min(MAX_CHECKED))
                        .map(|i| Some(a + i as i64 * span))
                        .collect()
                }
                _ => {
                    out.no_timestamps = !out.waiting_for_timestamps;
                    Vec::new()
                }
            }
        }
    };
    for (i, bucket) in buckets.iter().enumerate() {
        let name = plan.piece_name(i + 1, *bucket);
        if i < PREVIEW_NAMES {
            out.names.push(name.clone());
        }
        let path = plan.dir.join(&name);
        if path.exists() {
            out.existing.push(path);
        }
    }
    out
}

/// A split being written.
#[derive(Debug)]
pub struct SplitJob {
    pub plan: SplitPlan,
    /// The rows (visible positions, or lines) to write, as far as the stream was when
    /// the job started.
    next: usize,
    end: usize,
    writer: Option<BufWriter<File>>,
    /// The piece being written: its path, lines and bytes, and its time span.
    current: Option<PathBuf>,
    piece_lines: u64,
    piece_bytes: u64,
    bucket: Option<i64>,
    pieces: usize,
    /// The pieces written, in order.
    pub written: Vec<PathBuf>,
    pub lines_written: u64,
    pub error: Option<String>,
    pub done: bool,
    /// Stopped by `cancel` before the end.
    pub cancelled: bool,
}

impl SplitJob {
    /// A job over `engine` as it is now.
    pub fn start(engine: &TailEngine, plan: SplitPlan) -> Self {
        let end = if plan.visible_only {
            engine.visible_lines()
        } else {
            engine.total_lines()
        };
        Self {
            plan,
            next: 0,
            end,
            writer: None,
            current: None,
            piece_lines: 0,
            piece_bytes: 0,
            bucket: None,
            pieces: 0,
            written: Vec::new(),
            lines_written: 0,
            error: None,
            done: end == 0,
            cancelled: false,
        }
    }

    /// The share done, 0 to 1.
    pub fn progress(&self) -> f32 {
        if self.end == 0 {
            1.0
        } else {
            self.next as f32 / self.end as f32
        }
    }

    /// Writes for at most `budget`; true once finished (done, failed or cancelled).
    pub fn step(&mut self, engine: &TailEngine, budget: Duration) -> bool {
        if self.done {
            return true;
        }
        let started = Instant::now();
        while self.next < self.end {
            let idx = if self.plan.visible_only {
                match engine.line_at(self.next) {
                    Some(i) => i,
                    None => {
                        self.next += 1;
                        continue;
                    }
                }
            } else {
                self.next
            };
            let Some(text) = engine.get_line(idx) else {
                self.next += 1;
                continue;
            };
            if let Err(e) = self.write_line(engine, idx, text.as_bytes()) {
                self.fail(e.to_string());
                return true;
            }
            self.next += 1;
            if self.next.is_multiple_of(256) && started.elapsed() >= budget {
                return false;
            }
        }
        if let Err(e) = self.close_piece() {
            self.fail(e.to_string());
        }
        self.done = true;
        true
    }

    fn write_line(&mut self, engine: &TailEngine, idx: usize, text: &[u8]) -> std::io::Result<()> {
        let size = text.len() as u64 + 1;
        let new_piece = match self.plan.by {
            SplitBy::Lines(n) => self.writer.is_none() || self.piece_lines >= n.max(1),
            SplitBy::Bytes(n) => {
                self.writer.is_none()
                    || (self.piece_bytes > 0 && self.piece_bytes + size > n.max(1))
            }
            SplitBy::Time(span) => {
                // Lines before the first timestamp go with the first timed piece.
                let ts = engine.line_timestamp(idx).filter(|&t| t != NO_TIMESTAMP);
                let bucket = ts.map(|t| bucket_of(t, span.max(1)));
                match (bucket, self.bucket) {
                    (Some(b), Some(cur)) if b != cur => {
                        self.bucket = Some(b);
                        true
                    }
                    (Some(b), None) => {
                        self.bucket = Some(b);
                        // The untimed head was written to a piece named after the first
                        // line's time once known: open the first piece now.
                        self.writer.is_none()
                    }
                    _ => self.writer.is_none(),
                }
            }
        };
        if new_piece {
            self.close_piece()?;
            self.open_piece(engine)?;
        }
        let w = self.writer.as_mut().expect("a piece is open");
        w.write_all(text)?;
        w.write_all(b"\n")?;
        self.piece_lines += 1;
        self.piece_bytes += size;
        self.lines_written += 1;
        Ok(())
    }

    fn open_piece(&mut self, engine: &TailEngine) -> std::io::Result<()> {
        self.pieces += 1;
        let bucket = match self.plan.by {
            SplitBy::Time(span) => self.bucket.or_else(|| {
                // An untimed head: named after the first timestamp of the stream.
                engine.first_timestamp().map(|t| bucket_of(t, span.max(1)))
            }),
            _ => None,
        };
        let path = self
            .plan
            .dir
            .join(self.plan.piece_name(self.pieces, bucket));
        let file = File::create(&path)?;
        self.writer = Some(BufWriter::with_capacity(1 << 20, file));
        self.current = Some(path);
        self.piece_lines = 0;
        self.piece_bytes = 0;
        Ok(())
    }

    fn close_piece(&mut self) -> std::io::Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
            if let Some(path) = self.current.take() {
                self.written.push(path);
            }
        }
        Ok(())
    }

    fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.writer = None;
        // The piece being written is incomplete: it goes.
        if let Some(path) = self.current.take() {
            let _ = std::fs::remove_file(path);
        }
        self.done = true;
    }

    /// Stops the job; the pieces already written stay, the incomplete one goes.
    pub fn cancel(&mut self) {
        if !self.done {
            self.cancelled = true;
            self.writer = None;
            if let Some(path) = self.current.take() {
                let _ = std::fs::remove_file(path);
            }
            self.done = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_by_part_and_by_bucket() {
        let plan = SplitPlan::for_file(Path::new("/l/app.log"), SplitBy::Lines(10));
        assert_eq!((plan.stem.as_str(), plan.ext.as_str()), ("app", ".log"));
        assert_eq!(plan.dir, PathBuf::from("/l"));
        assert_eq!(plan.piece_name(7, None), "app.part-007.log");
        let day = crate::timestamp::date_to_days(2026, 10, 5).unwrap() * 86_400_000;
        let at = day + 14 * 3_600_000 + 30 * 60_000;
        assert_eq!(
            bucket_label(bucket_of(at, 3_600_000), 3_600_000),
            "2026-10-05T14"
        );
        assert_eq!(
            bucket_label(bucket_of(at, 86_400_000), 86_400_000),
            "2026-10-05"
        );
        assert_eq!(
            bucket_label(bucket_of(at, 1_800_000), 1_800_000),
            "2026-10-05T14-30"
        );
        let bare = SplitPlan::for_file(Path::new("syslog"), SplitBy::Lines(1));
        assert_eq!(bare.piece_name(1, None), "syslog.part-001.log");
    }

    #[test]
    fn typed_specs() {
        assert_eq!(parse_spec("100000"), Some((SplitBy::Lines(100_000), false)));
        assert_eq!(
            parse_spec(" 10 MB "),
            Some((SplitBy::Bytes(10 << 20), false))
        );
        assert_eq!(parse_spec("1h!"), Some((SplitBy::Time(3_600_000), true)));
        assert_eq!(parse_spec("2d"), Some((SplitBy::Time(172_800_000), false)));
        assert_eq!(parse_spec("30m"), Some((SplitBy::Time(1_800_000), false)));
        assert_eq!(parse_spec("0"), None);
        assert_eq!(parse_spec("ten"), None);
        assert_eq!(parse_spec("5 parsecs"), None);
    }
}
