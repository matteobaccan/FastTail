// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Named sessions: the workspace (open files and patterns, dock layout, per-stream
//! filters, search query, wrap, encoding, ANSI and collapse modes, line-number and time
//! delta columns and bookmarks) saved to and loaded from a `*.fasttail-session.ini` file.
//! Global preferences stay in `fasttail.ini`, which embeds the default session with the
//! same sections so the behaviour of users who never name a session is unchanged.
//!
//! Paths are stored absolute and, when the file lies under the session's directory, also
//! relative to it, so a session saved next to a log bundle still opens after the bundle
//! moves: the relative path wins when it exists, then the absolute one, otherwise the
//! stream is skipped and reported.

use crate::config::{overwrite_regular_file, FastTailConfig};
use crate::filter_preset::ini_value;
use crate::scan_job::MAX_FILTER_TERMS;
use crate::wildcard::{is_pattern_path, split_pattern};
use ini::Ini;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// File name suffix of a session file (`incident.fasttail-session.ini`).
pub const SESSION_SUFFIX: &str = ".fasttail-session.ini";
/// Prefix of the keys holding a column view width (`fields_width.status=6`).
const FIELDS_WIDTH_PREFIX: &str = "fields_width.";
/// A field name as part of an INI key: `%`, `=` and `:` (which end an INI key) as
/// `%25`, `%3D`, `%3A`.
fn encode_ini_key(key: &str) -> String {
    key.replace('%', "%25")
        .replace('=', "%3D")
        .replace(':', "%3A")
}

/// The field name of `encode_ini_key`.
fn decode_ini_key(key: &str) -> String {
    key.replace("%3D", "=")
        .replace("%3d", "=")
        .replace("%3A", ":")
        .replace("%3a", ":")
        .replace("%25", "%")
}

/// Maximum number of remembered session files.
pub const MAX_RECENT_SESSIONS: usize = 10;

/// One stream of a session: what is needed to reopen it exactly as it was.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamEntry {
    /// Absolute path of the file, or a directory pattern such as `C:\logs\app-*.log`.
    pub path: PathBuf,
    /// First include and exclude term, under the keys every version reads.
    pub include_filter: String,
    pub exclude_filter: String,
    /// Further non-empty terms (2 to `MAX_FILTER_TERMS`), stored as `include.2`,
    /// `include.3`… next to `include`: an older build reads the first term and ignores
    /// the rest.
    pub include_extra: Vec<String>,
    pub exclude_extra: Vec<String>,
    pub search_query: String,
    pub wrap: bool,
    /// Encoding name as `FileEncoding::name()`; `None` keeps the detected one.
    pub encoding: Option<String>,
    /// ANSI mode chosen by the user as `AnsiMode::name()` (`render`, `strip`, `raw`);
    /// `None` is auto, and old files without the key read as auto.
    pub ansi: Option<String>,
    /// The timeline histogram is shown above the stream (`timeline=true`, written only
    /// when set).
    pub timeline: bool,
    /// Collapse of repeated lines as `CollapseMode::name()` (`exact`, `numbers`), written
    /// as `collapse=` only when not off; `None` is off, and old files read as off.
    pub collapse: Option<String>,
    /// Lines of context shown around each filter match (`context_lines=N`, written only
    /// when above 0); old files read as 0, off.
    pub context_lines: u8,
    /// The stream's line-number and time delta columns (`line_numbers=`, `time_delta=`).
    /// The app always records both; `None`, written as no key, is what a file from an
    /// older version reads as, and follows the `[general]` defaults.
    pub line_numbers: Option<bool>,
    pub time_delta: Option<bool>,
    /// Time display of the rows (`time_display=utc|local|+HH:MM`) and the zone of the
    /// timestamps without one (`time_source_zone=utc|+HH:MM`), as
    /// `TimeDisplay::to_config` / `SourceZone::to_config`; `None` (no key) is the
    /// default, as written and local time.
    pub time_display: Option<String>,
    pub time_source_zone: Option<String>,
    /// Bookmarked line indices, sorted.
    pub bookmarks: Vec<usize>,
    /// Notes of some of the bookmarks, stored as `bookmark_note.<line>` next to
    /// `bookmarks`: an older build ignores them and keeps the bookmarks.
    pub bookmark_notes: BTreeMap<usize, String>,
    /// Entry name when the stream is a zip entry: `path` is then `archive/entry` (see
    /// `compressed::entry_path`), and the file stores the archive path and the entry.
    pub archive_entry: Option<String>,
    /// The field parser forced for the stream as `ParserChoice::name` (`off`, `json`,
    /// `logfmt`, `regex`, `apache`, `syslog`), written as `fields_parser=` only when not
    /// auto, and the pattern of `regex` (`fields_regex=`). Older builds ignore both.
    pub fields_parser: Option<String>,
    pub fields_regex: Option<String>,
    /// The column view (`fields_view=true`, written only when on), the columns chosen in
    /// order (`fields_columns=ts,level,msg`, none: the defaults; keys holding a comma are
    /// not saved) and their widths in character cells (`fields_width.<key>=N`).
    pub fields_view: bool,
    pub fields_columns: Vec<String>,
    pub fields_widths: BTreeMap<String, u16>,
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
            sec.set("include", ini_value(&s.include_filter));
            sec.set("exclude", ini_value(&s.exclude_filter));
            for (key, extra) in [("include", &s.include_extra), ("exclude", &s.exclude_extra)] {
                for (n, term) in extra.iter().filter(|t| !t.is_empty()).enumerate() {
                    sec.set(format!("{key}.{}", n + 2), ini_value(term));
                }
            }
            sec.set("search", ini_value(&s.search_query));
            sec.set("wrap", s.wrap.to_string());
            sec.set("encoding", s.encoding.clone().unwrap_or_default());
            if let Some(ansi) = &s.ansi {
                sec.set("ansi", ansi);
            }
            if s.timeline {
                sec.set("timeline", "true");
            }
            if let Some(collapse) = &s.collapse {
                sec.set("collapse", collapse);
            }
            if s.context_lines > 0 {
                sec.set("context_lines", s.context_lines.to_string());
            }
            if let Some(show) = s.line_numbers {
                sec.set("line_numbers", show.to_string());
            }
            if let Some(show) = s.time_delta {
                sec.set("time_delta", show.to_string());
            }
            if let Some(display) = &s.time_display {
                sec.set("time_display", display);
            }
            if let Some(zone) = &s.time_source_zone {
                sec.set("time_source_zone", zone);
            }
            if let Some(parser) = &s.fields_parser {
                sec.set("fields_parser", parser);
            }
            if let Some(regex) = &s.fields_regex {
                sec.set("fields_regex", ini_value(regex));
            }
            if s.fields_view {
                sec.set("fields_view", "true");
            }
            let columns: Vec<&str> = s
                .fields_columns
                .iter()
                .map(String::as_str)
                .filter(|c| !c.is_empty() && !c.contains(','))
                .collect();
            if !columns.is_empty() {
                sec.set("fields_columns", ini_value(&columns.join(",")));
            }
            for (key, cells) in &s.fields_widths {
                sec.set(
                    format!("{FIELDS_WIDTH_PREFIX}{}", encode_ini_key(key)),
                    cells.to_string(),
                );
            }
            sec.set(
                "bookmarks",
                s.bookmarks
                    .iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            for (line, text) in &s.bookmark_notes {
                if s.bookmarks.binary_search(line).is_ok() {
                    sec.set(format!("bookmark_note.{line}"), ini_value(text));
                }
            }
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
            let extra = |key: &str| -> Vec<String> {
                (2..=MAX_FILTER_TERMS)
                    .filter_map(|n| sec.get(format!("{key}.{n}")))
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect()
            };
            let bookmarks: Vec<usize> = sec
                .get("bookmarks")
                .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
                .unwrap_or_default();
            let bookmark_notes = crate::config::read_notes(sec, "bookmark_note.", &bookmarks);
            out.session.streams.push(StreamEntry {
                path: resolved,
                include_filter: sec.get("include").unwrap_or("").to_string(),
                exclude_filter: sec.get("exclude").unwrap_or("").to_string(),
                include_extra: extra("include"),
                exclude_extra: extra("exclude"),
                search_query: sec.get("search").unwrap_or("").to_string(),
                wrap: sec
                    .get("wrap")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false),
                encoding: sec
                    .get("encoding")
                    .filter(|e| !e.is_empty())
                    .map(str::to_string),
                ansi: sec
                    .get("ansi")
                    .and_then(crate::ansi::AnsiMode::from_name)
                    .filter(|m| *m != crate::ansi::AnsiMode::Auto)
                    .map(|m| m.name().to_string()),
                timeline: sec
                    .get("timeline")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false),
                collapse: sec
                    .get("collapse")
                    .and_then(crate::collapse::CollapseMode::from_name)
                    .filter(|m| m.is_on())
                    .map(|m| m.name().to_string()),
                context_lines: sec
                    .get("context_lines")
                    .and_then(|v| v.trim().parse::<u8>().ok())
                    .unwrap_or(0)
                    .min(crate::context_lines::MAX_CONTEXT_LINES),
                line_numbers: sec.get("line_numbers").and_then(|v| v.parse().ok()),
                time_delta: sec.get("time_delta").and_then(|v| v.parse().ok()),
                time_display: sec
                    .get("time_display")
                    .and_then(crate::timestamp::TimeDisplay::from_config)
                    .filter(|d| *d != crate::timestamp::TimeDisplay::Written)
                    .map(|d| d.to_config()),
                time_source_zone: sec
                    .get("time_source_zone")
                    .and_then(crate::timestamp::SourceZone::from_config)
                    .filter(|z| *z != crate::timestamp::SourceZone::Local)
                    .map(|z| z.to_config()),
                bookmarks,
                bookmark_notes,
                archive_entry,
                fields_parser: sec
                    .get("fields_parser")
                    .and_then(|p| crate::fields::ParserChoice::from_name(p, ""))
                    .filter(|c| *c != crate::fields::ParserChoice::Auto)
                    .map(|c| c.name().to_string()),
                fields_regex: sec
                    .get("fields_regex")
                    .filter(|r| !r.is_empty())
                    .map(str::to_string),
                fields_view: sec
                    .get("fields_view")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false),
                fields_columns: sec
                    .get("fields_columns")
                    .map(|c| {
                        // Not trimmed: a key may start or end with a space.
                        c.split(',')
                            .filter(|c| !c.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                fields_widths: sec
                    .iter()
                    .filter_map(|(k, v)| {
                        let key = decode_ini_key(k.strip_prefix(FIELDS_WIDTH_PREFIX)?);
                        let cells = v.trim().parse::<u16>().ok()?;
                        (!key.is_empty()).then(|| {
                            (
                                key,
                                cells.clamp(crate::fields::MIN_WIDTH, crate::fields::MAX_WIDTH),
                            )
                        })
                    })
                    .collect(),
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
                (entry.bookmarks, entry.bookmark_notes) = cfg
                    .saved_bookmarks(p)
                    .map(|(_, lines, notes)| (lines.clone(), notes.clone()))
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
            cfg.set_bookmarks_with_notes(&s.path, &s.bookmarks, &s.bookmark_notes);
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

    #[test]
    fn view_columns_are_written_only_when_set_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a.log");
        std::fs::write(&log, "a\n").unwrap();
        let mut entry = StreamEntry::new(log.clone());
        let text = |entry: &StreamEntry| {
            Session {
                streams: vec![entry.clone()],
                dock_layout: None,
            }
            .serialized(None)
        };
        let plain = text(&entry);
        assert!(!plain.contains("line_numbers") && !plain.contains("time_delta"));

        entry.line_numbers = Some(false);
        entry.time_delta = Some(true);
        let written = text(&entry);
        assert!(written.contains("line_numbers=false"), "{written}");
        assert!(written.contains("time_delta=true"), "{written}");
        let conf = Ini::load_from_str(&written).unwrap();
        let read = &Session::read_from(&conf, None).session.streams[0];
        assert_eq!(
            (read.line_numbers, read.time_delta),
            (Some(false), Some(true))
        );

        // A file from before the keys, or with a value we cannot read, follows the
        // defaults.
        let conf =
            Ini::load_from_str(&plain.replace("wrap=", "line_numbers=maybe\nwrap=")).unwrap();
        let read = &Session::read_from(&conf, None).session.streams[0];
        assert_eq!((read.line_numbers, read.time_delta), (None, None));
    }

    #[test]
    fn time_display_is_written_only_when_not_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a.log");
        std::fs::write(&log, "a\n").unwrap();
        let mut entry = StreamEntry::new(log.clone());
        let text = |entry: &StreamEntry| {
            Session {
                streams: vec![entry.clone()],
                dock_layout: None,
            }
            .serialized(None)
        };
        let plain = text(&entry);
        assert!(!plain.contains("time_display"), "{plain}");
        assert!(!plain.contains("time_source_zone"), "{plain}");
        // An old file: as written, local.
        let conf = Ini::load_from_str(&plain).unwrap();
        let read = &Session::read_from(&conf, None).session.streams[0];
        assert_eq!(
            (read.time_display.clone(), read.time_source_zone.clone()),
            (None, None)
        );

        entry.time_display = Some("local".into());
        entry.time_source_zone = Some("+05:30".into());
        let written = text(&entry);
        assert!(written.contains("time_display=local"), "{written}");
        assert!(written.contains("time_source_zone=+05:30"), "{written}");
        let conf = Ini::load_from_str(&written).unwrap();
        let read = &Session::read_from(&conf, None).session.streams[0];
        assert_eq!(read.time_display.as_deref(), Some("local"));
        assert_eq!(read.time_source_zone.as_deref(), Some("+05:30"));

        // Defaults spelled out, and garbage, read as the defaults.
        for (display, zone) in [("written", "local"), ("sideways", "+99:00")] {
            let conf = Ini::load_from_str(&plain.replace(
                "wrap=",
                &format!("time_display={display}\ntime_source_zone={zone}\nwrap="),
            ))
            .unwrap();
            let read = &Session::read_from(&conf, None).session.streams[0];
            assert_eq!(
                (read.time_display.clone(), read.time_source_zone.clone()),
                (None, None)
            );
        }
    }

    #[test]
    fn context_lines_are_written_only_above_zero_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a.log");
        std::fs::write(&log, "a\n").unwrap();
        let mut entry = StreamEntry::new(log.clone());
        let text = |entry: &StreamEntry| {
            Session {
                streams: vec![entry.clone()],
                dock_layout: None,
            }
            .serialized(None)
        };
        let plain = text(&entry);
        assert!(!plain.contains("context_lines"), "{plain}");

        entry.context_lines = 4;
        let written = text(&entry);
        assert!(written.contains("context_lines=4"), "{written}");
        let conf = Ini::load_from_str(&written).unwrap();
        assert_eq!(
            Session::read_from(&conf, None).session.streams[0].context_lines,
            4
        );

        // An old file reads as off; a value out of range is capped, garbage is off.
        let conf = Ini::load_from_str(&plain).unwrap();
        assert_eq!(
            Session::read_from(&conf, None).session.streams[0].context_lines,
            0
        );
        for (value, expected) in [("250", 100), ("-3", 0), ("many", 0)] {
            let conf = Ini::load_from_str(
                &plain.replace("wrap=", &format!("context_lines={value}\nwrap=")),
            )
            .unwrap();
            assert_eq!(
                Session::read_from(&conf, None).session.streams[0].context_lines,
                expected,
                "{value}"
            );
        }
    }
}
