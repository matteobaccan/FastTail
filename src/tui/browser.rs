// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Open dialog: a folder listed as a Windows file dialog lists it (the parent, the
//! folders, then the files with size and date), the drives on Windows above the root,
//! and a name field that filters the list or takes a path, a pattern (`*.log`) or an
//! archive entry to open directly.

use crate::i18n::Language;
use crate::i18n_tui::{tx, txf};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::tui::form::TextField;

/// Entries read from one folder at most; a larger one is listed in part.
const MAX_ENTRIES: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `..`: the folder above (or the drives, above a Windows root).
    Parent,
    Drive,
    Dir,
    File,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub struct FileBrowser {
    /// The folder listed; `None` is the list of drives (Windows).
    pub dir: Option<PathBuf>,
    pub entries: Vec<Entry>,
    /// The name field: filters the list, or a path to open.
    pub field: TextField,
    /// Position in `shown()`.
    pub selected: usize,
    /// First position of `shown()` on screen.
    pub top: usize,
    /// Why the folder is listed in part or not at all.
    pub note: Option<String>,
    /// The language of the notes.
    pub lang: Language,
}

impl FileBrowser {
    /// The browser on `dir` (the drives when `None`).
    pub fn at(dir: Option<PathBuf>, lang: Language) -> Self {
        let mut b = Self {
            dir: None,
            entries: Vec::new(),
            field: TextField::default(),
            selected: 0,
            top: 0,
            note: None,
            lang,
        };
        b.go_to(dir);
        b
    }

    /// Lists `dir`, the field emptied and the first entry selected.
    pub fn go_to(&mut self, dir: Option<PathBuf>) {
        self.field = TextField::default();
        self.selected = 0;
        self.top = 0;
        self.note = None;
        self.entries = match &dir {
            Some(d) => match read_folder(d) {
                Ok((entries, cut)) => {
                    if cut {
                        self.note = Some(txf(
                            self.lang,
                            "Only the first {0} entries",
                            &[&MAX_ENTRIES],
                        ));
                    }
                    entries
                }
                Err(e) => {
                    self.note = Some(txf(self.lang, "Cannot read {0}: {1}", &[&d.display(), &e]));
                    Vec::new()
                }
            },
            None => drives(),
        };
        if dir.is_some() && (cfg!(windows) || dir.as_deref().and_then(Path::parent).is_some()) {
            self.entries.insert(
                0,
                Entry {
                    name: "..".into(),
                    kind: Kind::Parent,
                    size: 0,
                    modified: None,
                },
            );
        }
        self.dir = dir;
    }

    /// The folder above, with the folder left selected; above a Windows root, the
    /// drives.
    pub fn up(&mut self) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        let (target, child) = match dir.parent() {
            Some(p) => (
                Some(p.to_path_buf()),
                dir.file_name().map(|n| n.to_string_lossy().into_owned()),
            ),
            None if cfg!(windows) => (None, Some(dir.display().to_string())),
            None => return,
        };
        self.go_to(target);
        if let Some(name) = child {
            if let Some(k) = self.shown().iter().position(|&i| {
                let e = &self.entries[i];
                e.name.eq_ignore_ascii_case(&name)
                    || e.name.trim_end_matches(['\\', '/']) == name.trim_end_matches(['\\', '/'])
            }) {
                self.selected = k;
            }
        }
    }

    /// Indices of the entries the field lets through: a pattern with `*` or `?`, else
    /// the names holding the text (case-insensitive). `..` always stays; a text with a
    /// path separator filters nothing.
    pub fn shown(&self) -> Vec<usize> {
        let text = self.field.text().trim();
        let pattern = text.contains(['*', '?']);
        let needle = text.to_lowercase();
        let path_like = text.contains(['/', '\\']);
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                text.is_empty()
                    || path_like
                    || e.kind == Kind::Parent
                    || if pattern {
                        // Folders stay reachable while a pattern picks the files.
                        e.kind != Kind::File || crate::wildcard::wildcard_match(text, &e.name)
                    } else {
                        e.name.to_lowercase().contains(&needle)
                    }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Moves the selection by `delta` rows within the shown entries.
    pub fn move_by(&mut self, delta: isize) {
        let n = self.shown().len();
        self.selected = if n == 0 {
            0
        } else {
            self.selected.saturating_add_signed(delta).min(n - 1)
        };
    }

    /// The field changed: back to the first entry after `..`.
    pub fn filter_changed(&mut self) {
        let shown = self.shown();
        self.selected = usize::from(shown.len() > 1 && self.entries[shown[0]].kind == Kind::Parent);
        self.top = 0;
    }

    /// The selected entry.
    pub fn chosen(&self) -> Option<&Entry> {
        self.shown().get(self.selected).map(|&i| &self.entries[i])
    }

    /// The path of `entry`.
    pub fn path_of(&self, entry: &Entry) -> PathBuf {
        match (&self.dir, entry.kind) {
            (_, Kind::Drive) | (None, _) => PathBuf::from(&entry.name),
            (Some(d), _) => d.join(&entry.name),
        }
    }

    /// The typed text as a path: absolute as it is, else in the listed folder.
    fn typed_path(&self, text: &str) -> PathBuf {
        let p = PathBuf::from(text);
        match &self.dir {
            Some(d) if !p.is_absolute() && !has_drive(text) => d.join(p),
            _ => p,
        }
    }

    /// `Enter`: a typed folder is listed; a typed path, pattern or archive entry (or a
    /// name nothing matches) is returned to open; otherwise the selected entry is
    /// entered (a folder, a drive, `..`) or returned (a file).
    pub fn activate(&mut self) -> Option<PathBuf> {
        let text = self.field.text().trim().to_string();
        if !text.is_empty() {
            let typed = self.typed_path(&text);
            if typed.is_dir() {
                self.go_to(Some(typed));
                return None;
            }
            let direct = text.contains(['/', '\\', '*', '?'])
                || !self
                    .shown()
                    .iter()
                    .any(|&i| self.entries[i].kind != Kind::Parent);
            if direct {
                return Some(typed);
            }
        }
        let entry = self.chosen()?.clone();
        match entry.kind {
            Kind::Parent => self.up(),
            Kind::Drive | Kind::Dir => {
                let path = self.path_of(&entry);
                self.go_to(Some(path));
            }
            Kind::File => return Some(self.path_of(&entry)),
        }
        None
    }

    /// The listed folder as the dialog's title shows it.
    pub fn location(&self) -> String {
        match &self.dir {
            Some(d) => d.display().to_string(),
            None => tx(self.lang, "Drives").into(),
        }
    }
}

/// `C:` or `C:\...`: a Windows path from its drive.
fn has_drive(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// The folders, then the files of `dir`, each sorted by name (case-insensitive); true
/// when the listing stopped at `MAX_ENTRIES`.
fn read_folder(dir: &Path) -> std::io::Result<(Vec<Entry>, bool)> {
    let mut entries = Vec::new();
    let mut cut = false;
    for item in std::fs::read_dir(dir)? {
        if entries.len() >= MAX_ENTRIES {
            cut = true;
            break;
        }
        let Ok(item) = item else {
            continue;
        };
        // Links are followed, so a link to a folder opens like one.
        let meta = std::fs::metadata(item.path()).or_else(|_| item.metadata());
        let Ok(meta) = meta else {
            continue;
        };
        entries.push(Entry {
            name: item.file_name().to_string_lossy().into_owned(),
            kind: if meta.is_dir() { Kind::Dir } else { Kind::File },
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok(),
        });
    }
    entries.sort_by(|a, b| {
        (a.kind != Kind::Dir)
            .cmp(&(b.kind != Kind::Dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok((entries, cut))
}

/// The drives that exist (Windows); none elsewhere.
fn drives() -> Vec<Entry> {
    if !cfg!(windows) {
        return Vec::new();
    }
    (b'A'..=b'Z')
        .map(|c| format!("{}:\\", c as char))
        .filter(|root| Path::new(root).exists())
        .map(|name| Entry {
            name,
            kind: Kind::Drive,
            size: 0,
            modified: None,
        })
        .collect()
}

/// A modification time as the list shows it: `2026-09-30 18:05`, local time.
pub fn date_text(t: SystemTime) -> String {
    let Ok(since) = t.duration_since(SystemTime::UNIX_EPOCH) else {
        return String::new();
    };
    let utc = since.as_millis() as i64;
    let local = utc + crate::timestamp::local_offset_millis(utc);
    let (y, m, d) = crate::timestamp::days_to_date(local.div_euclid(86_400_000));
    let minutes = local.rem_euclid(86_400_000) / 60_000;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        minutes / 60,
        minutes % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("logs")).unwrap();
        std::fs::create_dir(dir.path().join("Archive")).unwrap();
        std::fs::write(dir.path().join("b.log"), "b\n").unwrap();
        std::fs::write(dir.path().join("a.txt"), "aaaa").unwrap();
        std::fs::write(dir.path().join("logs").join("app.log"), "x\n").unwrap();
        dir
    }

    fn names(b: &FileBrowser) -> Vec<&str> {
        b.shown()
            .iter()
            .map(|&i| b.entries[i].name.as_str())
            .collect()
    }

    #[test]
    fn folders_come_first_then_files_by_name() {
        let dir = tree();
        let b = FileBrowser::at(Some(dir.path().to_path_buf()), Language::En);
        assert_eq!(names(&b), ["..", "Archive", "logs", "a.txt", "b.log"]);
        assert_eq!(b.entries[3].size, 4);
        assert!(b.entries[3].modified.is_some());
    }

    #[test]
    fn enter_walks_folders_and_returns_a_file() {
        let dir = tree();
        let mut b = FileBrowser::at(Some(dir.path().to_path_buf()), Language::En);
        b.move_by(2);
        assert_eq!(b.chosen().unwrap().name, "logs");
        assert_eq!(b.activate(), None);
        assert_eq!(b.dir.as_deref(), Some(dir.path().join("logs").as_path()));
        b.move_by(1);
        assert_eq!(b.activate(), Some(dir.path().join("logs").join("app.log")));
        // `..` goes back up, with the folder just left selected.
        b.up();
        assert_eq!(b.dir.as_deref(), Some(dir.path()));
        assert_eq!(b.chosen().unwrap().name, "logs");
    }

    #[test]
    fn the_field_filters_or_names_a_path_to_open() {
        let dir = tree();
        let mut b = FileBrowser::at(Some(dir.path().to_path_buf()), Language::En);
        b.field.insert("LOG");
        b.filter_changed();
        assert_eq!(names(&b), ["..", "logs", "b.log"]);
        assert_eq!(b.chosen().unwrap().name, "logs", "the first match after ..");
        // A pattern keeps the folders and picks the files; Enter opens the pattern.
        b.field = TextField::new("*.log");
        b.filter_changed();
        assert_eq!(names(&b), ["..", "Archive", "logs", "b.log"]);
        assert_eq!(b.activate(), Some(dir.path().join("*.log")));
        // A typed folder is listed.
        b.field = TextField::new("logs");
        assert_eq!(b.activate(), None);
        assert_eq!(b.dir.as_deref(), Some(dir.path().join("logs").as_path()));
        // A name nothing matches is opened as typed (it may be missing).
        b.field = TextField::new("new.log");
        assert_eq!(b.activate(), Some(dir.path().join("logs").join("new.log")));
        // An absolute path is opened as it is.
        let abs = dir.path().join("b.log").display().to_string();
        b.field = TextField::new(&abs);
        assert_eq!(b.activate(), Some(PathBuf::from(abs)));
    }

    #[test]
    fn an_unreadable_folder_says_why() {
        let dir = tree();
        let b = FileBrowser::at(Some(dir.path().join("none")), Language::En);
        assert!(b.note.as_deref().unwrap().starts_with("Cannot read"));
        assert_eq!(names(&b), [".."]);
    }

    #[test]
    fn dates_read_as_local_minutes() {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(86_400 * 365);
        let text = date_text(t);
        assert_eq!(text.len(), 16, "{text}");
        assert!(
            text.starts_with("1970-12-3") || text.starts_with("1971-01-0"),
            "{text}"
        );
    }
}
