// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The scratchpad: one plain-text pad per session where lines sent from the streams and
//! free text are collected, then pasted into a ticket.
//!
//! Lines are sent with a reference line `── app.log:1204 ──` above them; a
//! `CTRL + click` (or `ALT + Enter`) on a reference line shows that line in its stream,
//! opening the file when needed. The reference is plain text the user may edit; the full
//! path of each name sent is kept in a small map beside the pad, so a reference still
//! resolves after a restart without putting paths into the text.
//!
//! The pad is saved beside its session: `<name>.fasttail-session.scratch.txt` (and
//! `….scratch.paths` for the map), or `scratchpad.txt` beside `fasttail.ini` for the
//! default workspace; at most once a second after a change, and on exit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Largest pad; sending more is refused.
pub const MAX_SCRATCHPAD_BYTES: usize = 4 * 1024 * 1024;
/// Least time between a change and its save.
pub const SAVE_DELAY: Duration = Duration::from_secs(1);

const REF_OPEN: &str = "── ";
const REF_CLOSE: &str = " ──";

/// The reference line written above the lines sent from `name` starting at line
/// `first` (0-based, written 1-based).
pub fn reference_line(name: &str, first: usize) -> String {
    format!("{REF_OPEN}{name}:{}{REF_CLOSE}", first + 1)
}

/// The file name and 0-based line a reference line names, when `line` is one.
pub fn parse_reference(line: &str) -> Option<(String, usize)> {
    let inner = line
        .trim()
        .strip_prefix(REF_OPEN.trim_end())?
        .strip_suffix(REF_CLOSE.trim_start())?
        .trim();
    let (name, number) = inner.rsplit_once(':')?;
    let number: usize = number.trim().replace(',', "").parse().ok()?;
    let name = name.trim();
    (!name.is_empty() && number >= 1).then(|| (name.to_string(), number - 1))
}

/// The text of the line of `text` holding byte `at`.
pub fn line_at(text: &str, at: usize) -> &str {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    &text[start..end]
}

/// Where the pad of a session file is kept: beside it, `….scratch.txt`.
pub fn sidecar_of(session_file: &Path) -> PathBuf {
    let mut name = session_file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(stripped) = name.strip_suffix(".ini") {
        name = stripped.to_string();
    }
    session_file.with_file_name(format!("{name}.scratch.txt"))
}

/// Where the pad of the default workspace is kept: `scratchpad.txt` beside `config`.
pub fn default_pad_of(config: &Path) -> PathBuf {
    config.with_file_name("scratchpad.txt")
}

/// The map file beside a pad file.
fn paths_file_of(pad: &Path) -> PathBuf {
    pad.with_extension("paths")
}

/// Why lines were not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRefused {
    /// The pad would grow past `MAX_SCRATCHPAD_BYTES`.
    TooLarge,
    /// Nothing to send.
    Empty,
}

#[derive(Default)]
pub struct Scratchpad {
    pub text: String,
    /// File name → full path of every stream lines were sent from.
    pub paths: BTreeMap<String, PathBuf>,
    /// The file the pad is saved to; `None` until the app sets one.
    pub file: Option<PathBuf>,
    /// When the text last changed without being saved.
    pub(crate) dirty_at: Option<Instant>,
    /// A reference line picked in the editor: `(file name, line)`, resolved by the app
    /// (`resolve_jump`) against the open streams.
    pub jump_request: Option<(String, usize)>,
    /// A reference line asked to be shown: `(path, line)`, applied by the app.
    pub jump: Option<(PathBuf, usize)>,
    /// Find box text and the last message shown under the editor (text, is a warning).
    pub find: String,
    pub notice: Option<(String, bool)>,
    pub confirm_clear: bool,
    /// The pad file exists but could not be read: it is left alone (never written over
    /// or deleted) until another pad is loaded.
    pub(crate) load_failed: bool,
}

/// Above this size the editor lays out slowly; the tab says so.
pub const LARGE_PAD_BYTES: usize = 512 * 1024;

impl Scratchpad {
    /// The pad saved at `file` (empty when there is none), which it is saved to from now.
    pub fn load(file: PathBuf) -> Self {
        // Not UTF-8 (re-saved by another editor): read as well as can be, invalid bytes
        // replaced. A file that cannot be read at all is left alone.
        let (text, load_failed) = match std::fs::read(&file) {
            Ok(bytes) => (String::from_utf8_lossy(&bytes).into_owned(), false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
            Err(_) => (String::new(), true),
        };
        let paths = std::fs::read_to_string(paths_file_of(&file))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('|'))
            .map(|(name, path)| (name.to_string(), PathBuf::from(path)))
            .collect();
        Self {
            text,
            paths,
            file: Some(file),
            load_failed,
            ..Default::default()
        }
    }

    /// Marks the text changed: it is saved `SAVE_DELAY` later.
    pub fn touch(&mut self) {
        self.dirty_at.get_or_insert_with(Instant::now);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_at.is_some()
    }

    /// Appends `lines` sent from the stream `name` (`path`), which start at line `first`,
    /// preceded by a reference line unless `with_reference` is false.
    pub fn send(
        &mut self,
        name: &str,
        path: &Path,
        first: usize,
        lines: &str,
        with_reference: bool,
    ) -> Result<(), SendRefused> {
        let lines = lines.trim_end_matches(['\n', '\r']);
        if lines.is_empty() {
            return Err(SendRefused::Empty);
        }
        let mut block = String::new();
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            block.push('\n');
        }
        if with_reference {
            block.push_str(&reference_line(name, first));
            block.push('\n');
        }
        block.push_str(lines);
        block.push('\n');
        if self.text.len() + block.len() > MAX_SCRATCHPAD_BYTES {
            return Err(SendRefused::TooLarge);
        }
        self.text.push_str(&block);
        self.paths.insert(name.to_string(), path.to_path_buf());
        self.touch();
        Ok(())
    }

    /// The stream a reference names: an open stream of that file name first (`open` are
    /// the open streams' names and paths), then the map of the names sent.
    pub fn resolve(&self, name: &str, open: &[(String, PathBuf)]) -> Option<PathBuf> {
        open.iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.clone())
            .or_else(|| self.paths.get(name).cloned())
    }

    /// Writes the pad (and its map) now. An empty pad removes its files.
    pub fn save(&mut self) -> std::io::Result<()> {
        if self.load_failed {
            return Err(std::io::Error::other(
                "the scratchpad file could not be read: it is left as it is",
            ));
        }
        self.dirty_at = None;
        let Some(file) = self.file.clone() else {
            return Ok(());
        };
        if self.text.is_empty() {
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(paths_file_of(&file));
            return Ok(());
        }
        crate::config::overwrite_regular_file(&file, self.text.as_bytes())?;
        let map: String = self
            .paths
            .iter()
            .map(|(name, path)| format!("{name}|{}\n", path.display()))
            .collect();
        crate::config::overwrite_regular_file(&paths_file_of(&file), map.as_bytes())
    }

    /// Saves when a change is older than `SAVE_DELAY`.
    pub fn save_if_due(&mut self) {
        if self.dirty_at.is_some_and(|at| at.elapsed() >= SAVE_DELAY) {
            if let Err(e) = self.save() {
                self.notice = Some((e.to_string(), true));
            }
        }
    }

    /// Saves the pad and loads the one of `file` (switching sessions).
    pub fn switch_to(&mut self, file: PathBuf) {
        if self.file.as_ref() == Some(&file) {
            return;
        }
        if self.is_dirty() {
            if let Err(e) = self.save() {
                // The notes stay here (and dirty): nothing is lost, the switch waits.
                self.dirty_at = Some(Instant::now());
                self.notice = Some((e.to_string(), true));
                return;
            }
        }
        *self = Self::load(file);
    }

    /// Saves on exit, only when something changed (another instance may have written
    /// the same pad since).
    pub fn save_on_exit(&mut self) {
        if self.is_dirty() {
            let _ = self.save();
        }
    }

    /// Copies the pad to `file` and keeps it there (Save session as…).
    pub fn move_to(&mut self, file: PathBuf) {
        self.file = Some(file);
        let _ = self.save();
    }
}

impl Scratchpad {
    /// Turns a reference picked in the editor into a jump, against the open streams
    /// (`(name, path)`); says so when the file cannot be found.
    pub fn resolve_jump(&mut self, open: &[(String, PathBuf)], lang: crate::i18n::Language) {
        let Some((name, line)) = self.jump_request.take() else {
            return;
        };
        match self.resolve(&name, open) {
            Some(path) if open.iter().any(|(_, p)| *p == path) || path.exists() => {
                self.jump = Some((path, line));
            }
            _ => {
                self.notice = Some((
                    crate::i18n::t(lang, "scratch_file_not_found").replace("{name}", &name),
                    true,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn reference_lines_round_trip_and_reject_other_text() {
        let line = reference_line("app.log", 1203);
        assert_eq!(line, "── app.log:1204 ──");
        assert_eq!(parse_reference(&line), Some(("app.log".into(), 1203)));
        assert_eq!(
            parse_reference("  ── C:\\logs\\a b.log:7 ──  "),
            Some(("C:\\logs\\a b.log".into(), 6))
        );
        for text in [
            "app.log:12",
            "── app.log ──",
            "── app.log:0 ──",
            "── :3 ──",
            "── a:x ──",
        ] {
            assert_eq!(parse_reference(text), None, "{text}");
        }
        let text = "first\n── a.log:2 ──\nline two\n";
        assert_eq!(line_at(text, 8), "── a.log:2 ──");
        assert_eq!(line_at(text, 0), "first");
        assert_eq!(line_at(text, text.len()), "");
    }

    #[test]
    fn sending_appends_blocks_and_refuses_past_the_cap() {
        let mut pad = Scratchpad {
            text: "my notes".into(),
            ..Default::default()
        };
        pad.send("a.log", Path::new("/x/a.log"), 9, "ten\neleven\n", true)
            .unwrap();
        pad.send("b.log", Path::new("/y/b.log"), 0, "one", false)
            .unwrap();
        assert_eq!(pad.text, "my notes\n── a.log:10 ──\nten\neleven\none\n");
        assert_eq!(pad.paths.get("a.log"), Some(&PathBuf::from("/x/a.log")));
        assert!(pad.is_dirty());
        assert_eq!(
            pad.send("a.log", Path::new("/x/a.log"), 0, "\n", true),
            Err(SendRefused::Empty)
        );
        let huge = "x".repeat(MAX_SCRATCHPAD_BYTES);
        let before = pad.text.clone();
        assert_eq!(
            pad.send("a.log", Path::new("/x/a.log"), 0, &huge, true),
            Err(SendRefused::TooLarge)
        );
        assert_eq!(pad.text, before);
    }

    #[test]
    fn references_resolve_to_open_streams_then_to_the_map() {
        let mut pad = Scratchpad::default();
        pad.paths
            .insert("a.log".into(), PathBuf::from("/old/a.log"));
        let open = vec![("a.log".to_string(), PathBuf::from("/new/a.log"))];
        assert_eq!(
            pad.resolve("a.log", &open),
            Some(PathBuf::from("/new/a.log"))
        );
        assert_eq!(pad.resolve("a.log", &[]), Some(PathBuf::from("/old/a.log")));
        assert_eq!(pad.resolve("b.log", &open), None);

        pad.jump_request = Some(("gone.log".into(), 3));
        pad.resolve_jump(&open, Language::En);
        assert!(pad.jump.is_none());
        assert!(pad.notice.as_ref().is_some_and(|(_, warn)| *warn));
        pad.jump_request = Some(("a.log".into(), 3));
        pad.resolve_jump(&open, Language::En);
        assert_eq!(pad.jump, Some((PathBuf::from("/new/a.log"), 3)));
    }

    #[test]
    fn a_pad_that_is_not_utf8_loads_lossily_and_exit_saves_only_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pad.txt");
        std::fs::write(&file, b"caf\xe9 notes\n").unwrap();
        let mut pad = Scratchpad::load(file.clone());
        assert_eq!(pad.text, "caf\u{fffd} notes\n");
        // Nothing changed: exit leaves the file as the other editor wrote it.
        pad.save_on_exit();
        assert_eq!(std::fs::read(&file).unwrap(), b"caf\xe9 notes\n");
        pad.touch();
        pad.save_on_exit();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "caf\u{fffd} notes\n"
        );
    }

    #[test]
    fn a_failed_save_keeps_the_notes_instead_of_switching() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the pad file should be: the save fails.
        let blocked = dir.path().join("blocked.txt");
        std::fs::create_dir(&blocked).unwrap();
        let mut pad = Scratchpad {
            file: Some(blocked.clone()),
            ..Default::default()
        };
        pad.send("a.log", Path::new("/x/a.log"), 0, "keep me", true)
            .unwrap();
        pad.switch_to(dir.path().join("other.txt"));
        assert_eq!(pad.file.as_deref(), Some(blocked.as_path()));
        assert!(pad.text.contains("keep me"));
        assert!(pad.notice.as_ref().is_some_and(|(_, warn)| *warn));
    }

    #[test]
    fn the_pad_and_its_map_round_trip_and_switch_with_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("incident.fasttail-session.ini");
        let file = sidecar_of(&session);
        assert_eq!(
            file.file_name().unwrap().to_string_lossy(),
            "incident.fasttail-session.scratch.txt"
        );
        let mut pad = Scratchpad::load(file.clone());
        assert!(pad.text.is_empty(), "no file is an empty pad");
        pad.send("a.log", Path::new("/x/a.log"), 0, "hello", true)
            .unwrap();
        pad.save().unwrap();
        let loaded = Scratchpad::load(file.clone());
        assert_eq!(loaded.text, pad.text);
        assert_eq!(loaded.paths, pad.paths);

        // Another session: its own (empty) pad; the first one was saved on the way.
        pad.send("a.log", Path::new("/x/a.log"), 1, "more", false)
            .unwrap();
        let other = sidecar_of(&dir.path().join("other.fasttail-session.ini"));
        pad.switch_to(other.clone());
        assert!(pad.text.is_empty());
        assert!(Scratchpad::load(file).text.ends_with("more\n"));

        // An empty pad leaves no file behind.
        pad.send("a.log", Path::new("/x/a.log"), 0, "x", true)
            .unwrap();
        pad.save().unwrap();
        assert!(other.exists());
        pad.text.clear();
        pad.save().unwrap();
        assert!(!other.exists());
        assert_eq!(
            default_pad_of(&dir.path().join("fasttail.ini")),
            dir.path().join("scratchpad.txt")
        );
    }

    #[test]
    fn test_save_rejects_non_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let dir_target = dir.path().join("dir_target");
        std::fs::create_dir(&dir_target).unwrap();

        let mut pad = Scratchpad {
            file: Some(dir_target.clone()),
            text: "some content".into(),
            ..Default::default()
        };

        let err = pad
            .save()
            .expect_err("save should fail when target is a directory");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
