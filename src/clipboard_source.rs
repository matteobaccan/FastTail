// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The clipboard as a stream: pasted text is copied into a spool by the standard-input
//! copier (same limits, same encoding detection), so every stream feature works on it.
//! A later paste is appended to the same spool (`TailEngine::append_clipboard`), so
//! several pastes build one log. The stream is `<clipboard N>`: the workspace, the
//! recent files and sessions skip it, as they skip `<stdin>`; its text is not kept.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::stdin_source::{Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};

/// The start of a clipboard stream's path: `<clipboard 1>`, `<clipboard 2>`...
pub const PREFIX: &str = "<clipboard";

static NEXT: AtomicUsize = AtomicUsize::new(1);

/// Why a paste did not become a stream.
#[derive(Debug)]
pub enum PasteError {
    /// The clipboard holds no text.
    Empty,
    /// More than the spool cap (`stdin_spool_max_mb`), in bytes.
    TooLarge(u64),
    /// The previous paste is still being copied.
    Busy,
    Io(std::io::Error),
}

impl std::fmt::Display for PasteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteError::Empty => write!(f, "the clipboard holds no text"),
            PasteError::TooLarge(cap) => write!(f, "the text is larger than {cap} bytes"),
            PasteError::Busy => write!(f, "the previous paste is still being read"),
            PasteError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for PasteError {
    fn from(e: std::io::Error) -> Self {
        PasteError::Io(e)
    }
}

/// The copier of a clipboard stream, its title (`clipboard 14:02`), and whether the text
/// so far ended its last line.
pub struct ClipboardStream {
    pub copier: StdinStream,
    pub title: String,
    ended_line: bool,
    /// The stream bar's Append was clicked: the app pastes into this stream.
    pub append_requested: bool,
}

/// The local time as `14:02`, for the title of a new clipboard stream.
pub fn clock_now() -> String {
    let minutes = crate::timestamp::local_now_millis()
        .div_euclid(60_000)
        .rem_euclid(1440);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Whether `path` names a clipboard stream.
pub fn is_clipboard_path(path: &Path) -> bool {
    path.to_str().is_some_and(|p| p.starts_with(PREFIX))
}

/// The bytes the spool may hold.
fn cap(settings: &Settings) -> u64 {
    settings.limits.max_bytes
}

/// Opens `text` as a new stream titled after the local time `clock` (`14:02`).
pub fn open_engine(
    text: &str,
    clock: &str,
    settings: &Settings,
    wake: Option<WakeFn>,
) -> Result<TailEngine, PasteError> {
    if text.is_empty() {
        return Err(PasteError::Empty);
    }
    if text.len() as u64 > cap(settings) {
        return Err(PasteError::TooLarge(cap(settings)));
    }
    let bytes = text.as_bytes().to_vec();
    let copier = StdinStream::start_named(Cursor::new(bytes), settings, wake.clone(), "clipboard")?;
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(copier.spool_path(), wake)?,
        None => TailEngine::open(copier.spool_path())?,
    };
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    engine.path = PathBuf::from(format!("{PREFIX} {n}>"));
    engine.follow_tail = true;
    engine.clipboard = Some(ClipboardStream {
        copier,
        title: format!("clipboard {clock}"),
        ended_line: text.ends_with('\n'),
        append_requested: false,
    });
    Ok(engine)
}

impl TailEngine {
    /// Appends `text` to a clipboard stream, on a line of its own.
    pub fn append_clipboard(
        &mut self,
        text: &str,
        settings: &Settings,
        wake: Option<WakeFn>,
    ) -> Result<(), PasteError> {
        let Some(c) = self.clipboard.as_mut() else {
            return Err(PasteError::Io(std::io::Error::other(
                "not a clipboard stream",
            )));
        };
        if text.is_empty() {
            return Err(PasteError::Empty);
        }
        if c.copier.is_reading() {
            return Err(PasteError::Busy);
        }
        if c.copier.written() + text.len() as u64 > cap(settings) {
            return Err(PasteError::TooLarge(cap(settings)));
        }
        let mut bytes = Vec::with_capacity(text.len() + 1);
        if !c.ended_line {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(text.as_bytes());
        c.copier.resume(Cursor::new(bytes), settings, wake)?;
        c.ended_line = text.ends_with('\n');
        self.follow_tail = true;
        Ok(())
    }

    /// Whether this is a clipboard stream.
    pub fn is_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_paths_are_recognised() {
        assert!(is_clipboard_path(Path::new("<clipboard 3>")));
        assert!(!is_clipboard_path(Path::new("<stdin>")));
        assert!(!is_clipboard_path(Path::new("clipboard.log")));
    }
}
