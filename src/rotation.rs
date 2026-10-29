// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! A file with its rotations as one stream: `app.log.2.gz`, `app.log.1`, `app.log` read
//! in order, oldest first, the live file last and followed (`rotated:<path>`).
//!
//! The rotations next to a file are recognised by name (`<name>.<n>`, the same
//! compressed, `<name>-<YYYYMMDD>`, `<stem>.<YYYY-MM-DD>.<ext>`). A producer thread writes
//! the pieces, decompressed, then the live file as it grows, into a pipe; the
//! standard-input copier puts the pipe into a spool the engine tails, so every stream
//! feature works on it. A piece without a final line break gets one, so no line spans two
//! pieces, and where each piece starts in the spool is recorded for the stream bar. When
//! the live file is renamed while it is followed (the application rotates it), the
//! producer reads what is left of the old file, then goes on with the new one.

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::compressed::{Codec, Format};
use crate::stdin_source::{Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};

/// The identity prefix of a stream with its rotations.
pub const PREFIX: &str = "rotated:";
/// Pieces read at most (the newest ones are kept).
pub const MAX_PIECES: usize = 256;
/// How often the followed file is looked at once its end is reached.
const FOLLOW_POLL: Duration = Duration::from_millis(200);

/// What a stream does with a file's rotations when it opens (`rotations=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Policy {
    /// The stream bar offers to include them.
    #[default]
    Ask,
    Always,
    Never,
}

impl Policy {
    pub fn name(self) -> &'static str {
        match self {
            Policy::Ask => "ask",
            Policy::Always => "always",
            Policy::Never => "never",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "ask" => Some(Policy::Ask),
            "always" => Some(Policy::Always),
            "never" => Some(Policy::Never),
            _ => None,
        }
    }
}

/// A rotation of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub path: PathBuf,
    pub codec: Option<Codec>,
    /// Bytes on disk.
    pub size: u64,
}

/// The identity of `live` with its rotations.
pub fn identity(live: &Path) -> PathBuf {
    PathBuf::from(format!("{PREFIX}{}", live.display()))
}

/// The live file of a rotated identity.
pub fn parse(path: &Path) -> Option<PathBuf> {
    path.to_str()?
        .strip_prefix(PREFIX)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

pub fn is_rotated_path(path: &Path) -> bool {
    parse(path).is_some()
}

/// How old a sibling is, as its name tells: dates sort before numbers (`app.log.1` is the
/// newest rotation), a date by its text, a number from the largest down.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Age {
    Dated(String),
    Numbered(std::cmp::Reverse<u32>),
}

const COMPRESSED_EXT: [&str; 5] = [".gz", ".xz", ".zst", ".bz2", ".zstd"];

/// Strips a compression extension.
fn without_compression(name: &str) -> &str {
    COMPRESSED_EXT
        .iter()
        .find_map(|ext| name.strip_suffix(ext))
        .unwrap_or(name)
}

/// `YYYYMMDD` or `YYYY-MM-DD` (with an optional `-HH` / `_HHMMSS` tail) at the start of
/// `text`, as a sortable string.
fn date_prefix(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        b.get(r.clone())
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
    };
    if digits(0..8) && b.get(8).is_none_or(|c| !c.is_ascii_digit()) {
        return Some(text.to_string());
    }
    if digits(0..4)
        && b.get(4) == Some(&b'-')
        && digits(5..7)
        && b.get(7) == Some(&b'-')
        && digits(8..10)
    {
        return Some(text.replace('-', ""));
    }
    None
}

/// How old the file named `name` is as a rotation of the file named `live`, `None` when
/// it is not one.
fn age_of(live: &str, name: &str) -> Option<Age> {
    if name == live {
        return None;
    }
    let bare = without_compression(name);
    // `app.log.3`, `app.log.3.gz`
    if let Some(n) = bare.strip_prefix(live).and_then(|r| r.strip_prefix('.')) {
        if let Ok(n) = n.parse::<u32>() {
            return Some(Age::Numbered(std::cmp::Reverse(n)));
        }
    }
    // `app.log-20261027`, `app.log-20261027.gz`, `app.log.2026-10-27`
    if let Some(rest) = bare
        .strip_prefix(live)
        .and_then(|r| r.strip_prefix(['-', '.', '_']))
    {
        if let Some(d) = date_prefix(rest) {
            return Some(Age::Dated(d));
        }
    }
    // `app.2026-10-27.log`, `app-2026-10-27.log` (logback, log4j)
    if let Some((stem, ext)) = live.rsplit_once('.') {
        if let Some(mid) = bare
            .strip_prefix(stem)
            .and_then(|r| r.strip_suffix(&format!(".{ext}")))
            .and_then(|r| r.strip_prefix(['-', '.', '_']))
        {
            if let Some(d) = date_prefix(mid) {
                // `app.2026-10-27.1.log`: the counter after the date orders the day's pieces.
                return Some(Age::Dated(d));
            }
        }
    }
    None
}

/// The rotations next to `live`, oldest first (at most `MAX_PIECES`, the newest kept).
pub fn siblings(live: &Path) -> Vec<Piece> {
    let (Some(dir), Some(name)) = (live.parent(), live.file_name().and_then(|n| n.to_str())) else {
        return Vec::new();
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(Age, PathBuf, u64)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file = e.file_name();
            let age = age_of(name, file.to_str()?)?;
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            Some((age, e.path(), meta.len()))
        })
        .collect();
    found.sort();
    if found.len() > MAX_PIECES {
        found.drain(..found.len() - MAX_PIECES);
    }
    found
        .into_iter()
        .map(|(_, path, size)| {
            let codec = match crate::compressed::sniff(&path) {
                Format::Compressed(c) => Some(c),
                _ => None,
            };
            Piece { path, codec, size }
        })
        .collect()
}

/// What the stream bar offers for `live`: the rotations found and their bytes on disk.
pub fn offer(live: &Path) -> Option<(usize, u64)> {
    let pieces = siblings(live);
    (!pieces.is_empty()).then(|| (pieces.len(), pieces.iter().map(|p| p.size).sum()))
}

/// Where each piece starts in the spool, with its file name.
type Starts = Arc<Mutex<Vec<(u64, String)>>>;

/// A running stream with its rotations, owned by its engine (`TailEngine::rotation`).
pub struct RotationStream {
    pub live: PathBuf,
    /// The rotations read before the live file.
    pub pieces: usize,
    starts: Starts,
    stop: Arc<AtomicBool>,
    /// The copier; declared last so the producer stops before the spool goes.
    pub copier: StdinStream,
}

impl RotationStream {
    /// Where each piece starts in the spool (the live file last), as known so far.
    pub fn starts(&self) -> Vec<(u64, String)> {
        self.starts.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for RotationStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The producer: every piece, then the live file followed, into `out`.
struct Producer<W: Write> {
    out: W,
    written: u64,
    last: u8,
    starts: Starts,
    stop: Arc<AtomicBool>,
    wake: Option<WakeFn>,
}

impl<W: Write> Producer<W> {
    fn begin(&mut self, name: String) -> std::io::Result<()> {
        // A piece that did not end its last line: ended, so the next piece starts a line.
        if self.written > 0 && self.last != b'\n' {
            self.put(b"\n")?;
        }
        if let Ok(mut s) = self.starts.lock() {
            s.push((self.written, name));
        }
        Ok(())
    }

    fn put(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.out.write_all(bytes)?;
        self.written += bytes.len() as u64;
        self.last = bytes[bytes.len() - 1];
        Ok(())
    }

    /// Copies `reader` to its end; `false` when the stream was closed meanwhile.
    fn copy(&mut self, reader: &mut dyn Read) -> std::io::Result<bool> {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            match reader.read(&mut buf) {
                Ok(0) => return Ok(true),
                Ok(n) => self.put(&buf[..n])?,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                // A damaged rotation: what it gave is kept, the next piece follows.
                Err(_) => return Ok(true),
            }
        }
    }

    fn run(mut self, pieces: Vec<Piece>, live: PathBuf) -> std::io::Result<()> {
        for piece in pieces {
            let Ok(file) = crate::file_source::open_file_shared(&piece.path) else {
                continue;
            };
            self.begin(file_name(&piece.path))?;
            let input = BufReader::with_capacity(64 * 1024, file);
            let mut reader: Box<dyn Read> = match piece.codec {
                Some(codec) => crate::compressed::open_decoder(codec, input),
                None => Box::new(input),
            };
            if !self.copy(&mut *reader)? {
                return Ok(());
            }
        }
        self.follow(&live)
    }

    /// The live file, then whatever replaces it after a rotation, until the stream closes.
    fn follow(&mut self, live: &Path) -> std::io::Result<()> {
        let mut first = true;
        loop {
            let file = loop {
                if self.stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                match crate::file_source::open_file_shared(live) {
                    Ok(f) => break f,
                    Err(_) => std::thread::sleep(FOLLOW_POLL),
                }
            };
            self.begin(file_name(live))?;
            if !first {
                if let Some(wake) = &self.wake {
                    wake();
                }
            }
            first = false;
            let identity = file_identity(&file);
            let mut file = file;
            let mut pos = 0u64;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                if self.stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                match file.read(&mut buf) {
                    Ok(0) => {
                        // At the end: has the path moved on to another file?
                        let replaced = match std::fs::metadata(live) {
                            Ok(meta) => meta.len() < pos || path_identity(live) != identity,
                            Err(_) => false,
                        };
                        if replaced {
                            // What the old file still got after the last read is in it.
                            let mut rest = Vec::new();
                            let _ = file.read_to_end(&mut rest);
                            self.put(&rest)?;
                            break;
                        }
                        self.out.flush()?;
                        std::thread::sleep(FOLLOW_POLL);
                    }
                    Ok(n) => {
                        pos += n as u64;
                        self.put(&buf[..n])?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => std::thread::sleep(FOLLOW_POLL),
                }
            }
        }
    }
}

/// The identity of an open file (device and inode on Unix; nothing to compare elsewhere,
/// where the length tells a replaced file).
#[cfg(unix)]
fn file_identity(file: &std::fs::File) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    file.metadata().ok().map(|m| (m.dev(), m.ino()))
}

#[cfg(not(unix))]
fn file_identity(_file: &std::fs::File) -> Option<(u64, u64)> {
    None
}

#[cfg(unix)]
fn path_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

#[cfg(not(unix))]
fn path_identity(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// Opens `live` with its rotations as one stream: `path` is the rotated identity, follow
/// is on.
pub fn open_engine(
    live: &Path,
    settings: &Settings,
    wake: Option<WakeFn>,
) -> std::io::Result<TailEngine> {
    let pieces = siblings(live);
    let count = pieces.len();
    let (reader, writer) = std::io::pipe()?;
    let starts: Starts = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let producer = Producer {
        out: std::io::BufWriter::with_capacity(64 * 1024, writer),
        written: 0,
        last: b'\n',
        starts: Arc::clone(&starts),
        stop: Arc::clone(&stop),
        wake: wake.clone(),
    };
    let live_path = live.to_path_buf();
    std::thread::Builder::new()
        .name("fasttail-rotation".into())
        .spawn(move || {
            let _ = producer.run(pieces, live_path);
        })?;
    let copier = StdinStream::start_named(reader, settings, wake.clone(), "rotated")?;
    let stream = RotationStream {
        live: live.to_path_buf(),
        pieces: count,
        starts,
        stop,
        copier,
    };
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake)?,
        None => TailEngine::open(stream.copier.spool_path())?,
    };
    engine.path = identity(live);
    engine.follow_tail = true;
    engine.rotation = Some(stream);
    Ok(engine)
}

impl TailEngine {
    /// The first line of each piece of a stream with rotations (those indexed so far),
    /// with its file name.
    pub fn rotation_pieces(&self) -> Vec<(usize, String)> {
        let Some(r) = self.rotation.as_ref() else {
            return Vec::new();
        };
        // A piece always starts a line: listed once that line is indexed (the file size
        // can run ahead of or behind the index while the spool grows).
        r.starts()
            .into_iter()
            .filter_map(|(offset, name)| {
                let line = self.line_of_offset(offset);
                (offset == 0 || self.line_offsets.get(line) == Some(&offset))
                    .then_some((line, name))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_names_are_recognised_and_ordered() {
        let age = |n| age_of("app.log", n);
        assert_eq!(age("app.log.1"), Some(Age::Numbered(std::cmp::Reverse(1))));
        assert_eq!(
            age("app.log.12.gz"),
            Some(Age::Numbered(std::cmp::Reverse(12)))
        );
        assert_eq!(age("app.log-20261027"), Some(Age::Dated("20261027".into())));
        assert_eq!(
            age("app.log-20261027.gz"),
            Some(Age::Dated("20261027".into()))
        );
        assert_eq!(
            age("app.log.2026-10-27"),
            Some(Age::Dated("20261027".into()))
        );
        assert_eq!(
            age("app.2026-10-27.log"),
            Some(Age::Dated("20261027".into()))
        );
        assert_eq!(
            age("app-2026-10-27.log"),
            Some(Age::Dated("20261027".into()))
        );
        assert_eq!(age("app.log"), None);
        assert_eq!(age("app.log.bak"), None);
        assert_eq!(age("other.log.1"), None);
        assert_eq!(age("app.logger.1"), None);
        let mut ages = vec![
            age("app.log.1").unwrap(),
            age("app.log.3.gz").unwrap(),
            age("app.log-20261020").unwrap(),
            age("app.log.2").unwrap(),
        ];
        ages.sort();
        assert_eq!(
            ages,
            vec![
                Age::Dated("20261020".into()),
                Age::Numbered(std::cmp::Reverse(3)),
                Age::Numbered(std::cmp::Reverse(2)),
                Age::Numbered(std::cmp::Reverse(1)),
            ]
        );
    }

    #[test]
    fn identities_and_policies() {
        let id = identity(Path::new("/var/log/app.log"));
        assert_eq!(id, PathBuf::from("rotated:/var/log/app.log"));
        assert_eq!(parse(&id), Some(PathBuf::from("/var/log/app.log")));
        assert!(parse(Path::new("/var/log/app.log")).is_none());
        assert!(parse(Path::new("rotated:")).is_none());
        for p in [Policy::Ask, Policy::Always, Policy::Never] {
            assert_eq!(Policy::from_name(p.name()), Some(p));
        }
    }
}
