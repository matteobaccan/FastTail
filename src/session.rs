//! Named sessions: the workspace (open files and patterns, dock layout, per-stream
//! filters, search query, wrap, encoding and bookmarks) saved to and loaded from a
//! `*.fasttail-session.ini` file. Global preferences stay in `fasttail.ini`, which embeds
//! the default session with the same sections so the behaviour of users who never name a
//! session is unchanged.
//!
//! Paths are stored absolute and, when the file lies under the session's directory, also
//! relative to it, so a session saved next to a log bundle still opens after the bundle
//! moves: the relative path wins when it exists, then the absolute one, otherwise the
//! stream is skipped and reported.

use crate::config::{overwrite_regular_file, FastTailConfig};
use crate::wildcard::{is_pattern_path, split_pattern};
use ini::Ini;
use std::path::{Path, PathBuf};

/// File name suffix of a session file (`incident.fasttail-session.ini`).
pub const SESSION_SUFFIX: &str = ".fasttail-session.ini";
/// Maximum number of remembered session files.
pub const MAX_RECENT_SESSIONS: usize = 10;

/// One stream of a session: what is needed to reopen it exactly as it was.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamEntry {
    /// Absolute path of the file, or a directory pattern such as `C:\logs\app-*.log`.
    pub path: PathBuf,
    pub include_filter: String,
    pub exclude_filter: String,
    pub search_query: String,
    pub wrap: bool,
    /// Encoding name as `FileEncoding::name()`; `None` keeps the detected one.
    pub encoding: Option<String>,
    /// Bookmarked line indices, sorted.
    pub bookmarks: Vec<usize>,
    /// Entry name when the stream is a zip entry: `path` is then `archive/entry` (see
    /// `compressed::entry_path`), and the file stores the archive path and the entry.
    pub archive_entry: Option<String>,
}

impl StreamEntry {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            ..Default::default()
        }
    }
}

/// The workspace: streams in dock order plus the serialized dock layout.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    pub streams: Vec<StreamEntry>,
    pub dock_layout: Option<String>,
}

/// A session read from disk, with the streams that could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadedSession {
    pub session: Session,
    /// Paths (as written in the file) whose file or pattern directory no longer exists.
    pub missing: Vec<PathBuf>,
    /// True when at least one stream was reopened from a different location than the
    /// absolute path written in the file (a moved bundle): the saved dock layout, which
    /// names the old paths, is then not applicable.
    pub relocated: bool,
}

impl Session {
    /// Display name of a session file: `incident` for `incident.fasttail-session.ini`.
    pub fn name_of(file: &Path) -> String {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        name.strip_suffix(SESSION_SUFFIX)
            .map(str::to_string)
            .or_else(|| file.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or(name)
    }

    /// Adds the session suffix when the chosen file name has no `.ini` extension.
    pub fn with_suffix(file: &Path) -> PathBuf {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.to_ascii_lowercase().ends_with(".ini") {
            file.to_path_buf()
        } else {
            file.with_file_name(format!("{name}{SESSION_SUFFIX}"))
        }
    }

    /// Writes the session sections into `conf`: `[session]` with the layout and one
    /// `[stream_N]` per stream, with the relative path when `base_dir` contains it.
    pub fn write_into(&self, conf: &mut Ini, base_dir: Option<&Path>) {
        let mut sec = conf.with_section(Some("session"));
        sec.set("streams", self.streams.len().to_string());
        if let Some(layout) = &self.dock_layout {
            sec.set("layout", layout);
        }
        for (i, s) in self.streams.iter().enumerate() {
            let mut sec = conf.with_section(Some(format!("stream_{i}")));
            // A zip entry is written as its archive plus the entry name, so the file
            // names a real file and a moved bundle still resolves.
            let file = match &s.archive_entry {
                Some(entry) => crate::compressed::archive_of(&s.path, entry),
                None => s.path.clone(),
            };
            sec.set("path", file.to_string_lossy().to_string());
            if let Some(rel) = base_dir.and_then(|b| relative_under(&file, b)) {
                sec.set("rel", rel);
            }
            if let Some(entry) = &s.archive_entry {
                sec.set("entry", entry);
            }
            sec.set("include", &s.include_filter);
            sec.set("exclude", &s.exclude_filter);
            sec.set("search", &s.search_query);
            sec.set("wrap", s.wrap.to_string());
            sec.set("encoding", s.encoding.clone().unwrap_or_default());
            sec.set(
                "bookmarks",
                s.bookmarks
                    .iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
    }

    /// Reads the session sections of `conf`, resolving paths against `base_dir` (the
    /// directory of the session file) as described in the module documentation.
    pub fn read_from(conf: &Ini, base_dir: Option<&Path>) -> LoadedSession {
        let mut out = LoadedSession::default();
        if let Some(sec) = conf.section(Some("session")) {
            if let Some(layout) = sec.get("layout") {
                if !layout.is_empty() {
                    out.session.dock_layout = Some(layout.to_string());
                }
            }
        }
        let mut i = 0;
        while let Some(sec) = conf.section(Some(format!("stream_{i}"))) {
            i += 1;
            let absolute = sec.get("path").filter(|p| !p.is_empty()).map(PathBuf::from);
            let relative = sec.get("rel").filter(|p| !p.is_empty()).map(PathBuf::from);
            let Some(written) = absolute.clone().or_else(|| relative.clone()) else {
                continue;
            };
            let from_rel = base_dir
                .zip(relative)
                .map(|(b, r)| b.join(r))
                .filter(|p| source_exists(p));
            let resolved = match from_rel {
                Some(p) => {
                    if absolute
                        .as_deref()
                        .map(|a| !same_path(a, &p))
                        .unwrap_or(true)
                    {
                        out.relocated = true;
                    }
                    p
                }
                None => match absolute.filter(|p| source_exists(p)) {
                    Some(p) => p,
                    None => {
                        out.missing.push(written);
                        continue;
                    }
                },
            };
            let archive_entry = sec
                .get("entry")
                .filter(|e| !e.is_empty())
                .map(str::to_string);
            let resolved = match &archive_entry {
                Some(entry) => crate::compressed::entry_path(&resolved, entry),
                None => resolved,
            };
            let bookmarks: Vec<usize> = sec
                .get("bookmarks")
                .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
                .unwrap_or_default();
            out.session.streams.push(StreamEntry {
                path: resolved,
                include_filter: sec.get("include").unwrap_or("").to_string(),
                exclude_filter: sec.get("exclude").unwrap_or("").to_string(),
                search_query: sec.get("search").unwrap_or("").to_string(),
                wrap: sec
                    .get("wrap")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false),
                encoding: sec
                    .get("encoding")
                    .filter(|e| !e.is_empty())
                    .map(str::to_string),
                bookmarks,
                archive_entry,
            });
        }
        if out.relocated {
            out.session.dock_layout = None;
        }
        out
    }

    /// The session as INI text, used to detect unsaved changes: two sessions are the
    /// same when their text is the same.
    pub fn serialized(&self, base_dir: Option<&Path>) -> String {
        let mut conf = Ini::new();
        self.write_into(&mut conf, base_dir);
        let mut buf = Vec::new();
        let _ = conf.write_to(&mut buf);
        String::from_utf8_lossy(&buf).to_string()
    }

    pub fn save_to(&self, file: &Path) -> std::io::Result<()> {
        let mut conf = Ini::new();
        self.write_into(&mut conf, file.parent());
        let mut buf = Vec::new();
        conf.write_to(&mut buf)?;
        overwrite_regular_file(file, &buf)
    }

    pub fn load_from(file: &Path) -> std::io::Result<LoadedSession> {
        let conf = Ini::load_from_file(file).map_err(std::io::Error::other)?;
        Ok(Self::read_from(&conf, file.parent()))
    }

    /// The default session embedded in `fasttail.ini`: open files with their persisted
    /// per-stream state, wrap flag and bookmarks, plus the dock layout.
    pub fn from_config(cfg: &FastTailConfig) -> Self {
        let streams = cfg
            .open_files
            .iter()
            .map(|p| {
                let mut entry = cfg
                    .stream_state_for(p)
                    .cloned()
                    .unwrap_or_else(|| StreamEntry::new(p.clone()));
                entry.path = p.clone();
                if entry.archive_entry.is_none() {
                    entry.archive_entry = crate::compressed::split_entry_path(p).map(|(_, e)| e);
                }
                entry.wrap = cfg.wrap_for(p);
                entry.bookmarks = cfg
                    .bookmarks
                    .iter()
                    .find(|(bp, _)| crate::paths::paths_equal(bp, p))
                    .map(|(_, lines)| lines.clone())
                    .unwrap_or_default();
                entry
            })
            .collect();
        Self {
            streams,
            dock_layout: cfg.dock_layout.clone(),
        }
    }

    /// Makes this session the default workspace of `cfg`.
    pub fn apply_to_config(&self, cfg: &mut FastTailConfig) {
        cfg.open_files = self.streams.iter().map(|s| s.path.clone()).collect();
        cfg.dock_layout = self.dock_layout.clone();
        cfg.streams.clear();
        for s in &self.streams {
            cfg.set_stream_state(s.clone());
            cfg.set_wrap(&s.path, s.wrap);
            cfg.set_bookmarks(&s.path, &s.bookmarks);
        }
    }
}

/// `path` relative to `base`, as a string with forward slashes, when `path` lies under
/// `base` (ASCII-case-insensitive on Windows).
pub fn relative_under(path: &Path, base: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok().map(Path::to_path_buf);
    #[cfg(windows)]
    let rel = rel.or_else(|| {
        let p = path.to_string_lossy().replace('\\', "/");
        let b = base.to_string_lossy().replace('\\', "/");
        let b = b.trim_end_matches('/');
        if p.len() > b.len() + 1
            && p[..b.len()].eq_ignore_ascii_case(b)
            && p.as_bytes()[b.len()] == b'/'
        {
            Some(PathBuf::from(&p[b.len() + 1..]))
        } else {
            None
        }
    });
    let rel = rel?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// A file that exists, or a pattern whose directory exists.
fn source_exists(p: &Path) -> bool {
    if is_pattern_path(p) {
        split_pattern(p)
            .map(|(dir, _)| dir.is_dir())
            .unwrap_or(false)
    } else {
        p.is_file()
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    crate::paths::paths_equal(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_strips_the_session_suffix() {
        assert_eq!(
            Session::name_of(Path::new("C:/x/incident.fasttail-session.ini")),
            "incident"
        );
        assert_eq!(Session::name_of(Path::new("/tmp/dev.ini")), "dev");
    }

    #[test]
    fn suffix_is_added_once() {
        assert_eq!(
            Session::with_suffix(Path::new("/tmp/dev")),
            PathBuf::from("/tmp/dev.fasttail-session.ini")
        );
        assert_eq!(
            Session::with_suffix(Path::new("/tmp/dev.fasttail-session.ini")),
            PathBuf::from("/tmp/dev.fasttail-session.ini")
        );
    }

    #[test]
    fn test_save_to_rejects_non_regular_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir_target = temp_dir.path().join("dir_target");
        std::fs::create_dir(&dir_target).unwrap();

        let session = Session::default();
        let err = session
            .save_to(&dir_target)
            .expect_err("should fail when target is a directory");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn relative_paths_only_under_the_base() {
        let base = if cfg!(windows) {
            PathBuf::from(r"C:\bundle")
        } else {
            PathBuf::from("/bundle")
        };
        let inside = base.join("logs").join("app.log");
        assert_eq!(
            relative_under(&inside, &base).as_deref(),
            Some("logs/app.log")
        );
        let outside = if cfg!(windows) {
            PathBuf::from(r"D:\other\app.log")
        } else {
            PathBuf::from("/other/app.log")
        };
        assert_eq!(relative_under(&outside, &base), None);
        assert_eq!(relative_under(&base, &base), None);
    }
}
