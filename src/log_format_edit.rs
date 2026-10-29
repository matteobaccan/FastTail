// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The state of the "Log formats" editor, shared by the window and the terminal: the
//! format being edited as a draft, the preview of what it reads from its samples and
//! from the focused stream's first lines, and new, duplicate, save, import, export and
//! delete. Nothing changes for the streams until the draft is saved.

use crate::fields::FieldParser;
use crate::log_format::{self, FormatSpec, Library, LogFormat};
use crate::log_level::LogLevel;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Lines of the focused stream the preview reads.
pub const PREVIEW_LINES: usize = 200;
/// The preview is computed again this long after the last keystroke.
pub const PREVIEW_DELAY: Duration = Duration::from_millis(300);
/// Below this share of the lines starting an entry, the entry start pattern is suspect.
const FEW_ENTRIES_PERCENT: usize = 1;

/// A format as the editor holds it: every field as text, the patterns and the samples one
/// per line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub patterns: String,
    pub timestamp_field: String,
    pub timestamp_format: String,
    pub timestamp_zone: String,
    pub level_field: String,
    pub level_map: String,
    pub message_field: String,
    pub units: String,
    pub entry_start: String,
    pub files: String,
    pub columns: String,
    pub samples: String,
}

impl Draft {
    pub fn from_spec(spec: &FormatSpec) -> Self {
        Self {
            name: spec.name.clone(),
            patterns: spec.patterns.join("\n"),
            timestamp_field: spec.timestamp_field.clone(),
            timestamp_format: spec.timestamp_format.clone(),
            timestamp_zone: spec.timestamp_zone.clone(),
            level_field: spec.level_field.clone(),
            level_map: spec.level_map.clone(),
            message_field: spec.message_field.clone(),
            units: spec.units.clone(),
            entry_start: spec.entry_start.clone(),
            files: spec.files.join("; "),
            columns: spec.columns.join(","),
            samples: spec.samples.join("\n"),
        }
    }

    pub fn to_spec(&self) -> FormatSpec {
        let lines = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect()
        };
        let list = |text: &str, seps: &[char]| -> Vec<String> {
            text.split(seps)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        };
        FormatSpec {
            name: self.name.trim().to_string(),
            patterns: lines(&self.patterns)
                .into_iter()
                .take(log_format::MAX_PATTERNS)
                .collect(),
            timestamp_field: self.timestamp_field.trim().to_string(),
            timestamp_format: self.timestamp_format.trim().to_string(),
            timestamp_zone: self.timestamp_zone.trim().to_string(),
            level_field: self.level_field.trim().to_string(),
            level_map: self.level_map.trim().to_string(),
            message_field: self.message_field.trim().to_string(),
            units: self.units.trim().to_string(),
            entry_start: self.entry_start.clone(),
            files: list(&self.files, &[';']),
            columns: list(&self.columns, &[',', ';']),
            samples: lines(&self.samples)
                .into_iter()
                .take(log_format::MAX_SAMPLES)
                .collect(),
        }
    }
}

/// What the draft is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Editing {
    /// A format not saved yet.
    New,
    /// A user format, saved back to its file.
    File(PathBuf),
    /// A built-in format: read-only, duplicated to be changed.
    Builtin,
}

/// One line of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    pub text: String,
    /// A sample of the format, not a line of the stream.
    pub sample: bool,
    pub starts_entry: bool,
    pub matched: bool,
    pub fields: Vec<(String, String)>,
    pub time: Option<i64>,
    pub level: LogLevel,
}

/// What the draft reads, or why it does not compile.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub error: Option<String>,
    pub rows: Vec<PreviewRow>,
    /// Lines starting an entry, and those of them a pattern matches.
    pub entries: usize,
    pub matched: usize,
    /// Average time to read one line, in microseconds.
    pub micros_per_line: f64,
}

impl Preview {
    /// The share of entries matched, in percent.
    pub fn ratio(&self) -> usize {
        (self.matched * 100).checked_div(self.entries).unwrap_or(0)
    }

    /// Whether so few lines start an entry that the entry start pattern looks wrong.
    pub fn few_entries(&self) -> bool {
        !self.rows.is_empty() && self.entries * 100 < self.rows.len() * FEW_ENTRIES_PERCENT.max(1)
    }
}

/// Reads `samples` and then `stream` with the format of `spec`.
pub fn preview(spec: &FormatSpec, stream: &[String]) -> Preview {
    let format = match LogFormat::compile(spec.clone()) {
        Ok(f) => Arc::new(f),
        Err(e) => {
            return Preview {
                error: Some(e),
                ..Preview::default()
            }
        }
    };
    let parser = FieldParser::Format(format.clone());
    let lines = spec
        .samples
        .iter()
        .map(|l| (l.as_str(), true))
        .chain(stream.iter().map(|l| (l.as_str(), false)));
    let mut out = Preview::default();
    let started = Instant::now();
    let mut count = 0usize;
    for (text, sample) in lines {
        count += 1;
        let starts_entry = format.starts_entry(text);
        let (matched, fields) = crate::fields::with_spans(|spans| {
            let matched = format.scan(text, spans);
            let fields = spans
                .iter(text)
                .map(|f| (f.key.to_string(), f.value().into_owned()))
                .collect::<Vec<_>>();
            (matched, fields)
        });
        if starts_entry {
            out.entries += 1;
            out.matched += usize::from(matched);
        }
        out.rows.push(PreviewRow {
            text: text.to_string(),
            sample,
            starts_entry,
            matched,
            fields,
            time: crate::fields::line_timestamp(Some(&parser), text, Default::default())
                .map(|(t, _)| t),
            level: crate::fields::line_level(Some(&parser), text, 256),
        });
    }
    if count > 0 {
        out.micros_per_line = started.elapsed().as_secs_f64() * 1e6 / count as f64;
    }
    out
}

/// What the dialog did that the app has to follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatsAction {
    /// The formats folder changed: load it again and refresh the streams.
    Changed,
    Close,
}

/// The dialog's state while it is shown.
#[derive(Debug)]
pub struct FormatsDialog {
    /// The formats folder.
    pub dir: PathBuf,
    pub draft: Draft,
    pub editing: Editing,
    /// The focused stream's first lines, read when the dialog opened.
    pub stream_lines: Vec<String>,
    pub preview: Preview,
    pub edited_at: Option<Instant>,
    pub confirm_delete: Option<PathBuf>,
    /// The outcome of the last action (an error in red).
    pub notice: Option<(String, bool)>,
}

impl FormatsDialog {
    /// The dialog on `dir`, with a new empty draft.
    pub fn new(dir: PathBuf, stream_lines: Vec<String>) -> Self {
        let mut d = Self {
            dir,
            draft: Draft::default(),
            editing: Editing::New,
            stream_lines,
            preview: Preview::default(),
            edited_at: None,
            confirm_delete: None,
            notice: None,
        };
        d.new_format();
        d
    }

    /// A new draft whose samples are `lines` ("Create format from these lines").
    pub fn from_lines(dir: PathBuf, stream_lines: Vec<String>, lines: &[String]) -> Self {
        let mut d = Self::new(dir, stream_lines);
        d.draft.samples = lines
            .iter()
            .take(log_format::MAX_SAMPLES)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        d.refresh_preview();
        d
    }

    /// Edits `format` (a built-in one read-only).
    pub fn select(&mut self, format: &LogFormat) {
        self.draft = Draft::from_spec(&format.spec);
        self.editing = match (&format.file, format.builtin) {
            (_, true) => Editing::Builtin,
            (Some(file), false) => Editing::File(file.clone()),
            (None, false) => Editing::New,
        };
        self.confirm_delete = None;
        self.refresh_preview();
    }

    pub fn new_format(&mut self) {
        self.draft = Draft {
            name: "New format".to_string(),
            patterns: r"^(?P<ts>\S+ \S+) (?P<level>[A-Z]+) (?P<msg>.*)$".to_string(),
            timestamp_field: "ts".to_string(),
            level_field: "level".to_string(),
            message_field: "msg".to_string(),
            ..Draft::default()
        };
        self.editing = Editing::New;
        self.confirm_delete = None;
        self.refresh_preview();
    }

    /// A copy of the draft as a new format, `<name> copy`.
    pub fn duplicate(&mut self) {
        // A plugin's format (`acme/server`) becomes the user's own `server copy`.
        let name = self.draft.name.trim();
        let name = name.rsplit_once('/').map_or(name, |(_, n)| n);
        self.draft.name = format!("{name} copy");
        self.editing = Editing::New;
        self.confirm_delete = None;
        self.refresh_preview();
    }

    /// Whether the draft can be changed and saved.
    pub fn editable(&self) -> bool {
        self.editing != Editing::Builtin
    }

    pub fn refresh_preview(&mut self) {
        self.preview = preview(&self.draft.to_spec(), &self.stream_lines);
        self.edited_at = None;
    }

    /// Writes the draft to its file (a new one in the folder for a new format). Refused
    /// when it does not compile, or when another format has its name or its file name.
    pub fn save(&mut self, lib: &Library) -> Result<PathBuf, String> {
        let spec = self.draft.to_spec();
        LogFormat::compile(spec.clone())?;
        let own = match &self.editing {
            Editing::File(p) => Some(p.clone()),
            Editing::New => None,
            Editing::Builtin => return Err("built-in".to_string()),
        };
        if let Some(other) = lib.find(&spec.name) {
            if other.builtin || other.file != own {
                return Err(format!("{}: {}", spec.name, "exists"));
            }
        }
        // A renamed format moves to the file of its new name.
        let path = match &own {
            Some(p)
                if p.file_name()
                    .is_some_and(|n| n.to_string_lossy() == spec.file_name()) =>
            {
                p.clone()
            }
            _ => {
                let p = self.dir.join(spec.file_name());
                let same = own
                    .as_deref()
                    .is_some_and(|o| crate::paths::paths_equal(o, &p));
                if p.exists() && !same {
                    return Err(format!("{}: {}", p.display(), "exists"));
                }
                p
            }
        };
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        crate::config::overwrite_regular_file(&path, spec.to_ini().as_bytes())
            .map_err(|e| e.to_string())?;
        if let Some(old) = own.filter(|o| !crate::paths::paths_equal(o, &path)) {
            std::fs::remove_file(old).map_err(|e| e.to_string())?;
        }
        self.editing = Editing::File(path.clone());
        Ok(path)
    }

    /// Copies the format file `from` into the folder, when it loads and neither its name
    /// nor its file name is taken.
    pub fn import(&mut self, lib: &Library, from: &Path) -> Result<PathBuf, String> {
        let format = log_format::read_file(from)?;
        if lib.find(format.name()).is_some() {
            return Err(format!("{}: exists", format.name()));
        }
        let path = self.dir.join(format.spec.file_name());
        if path.exists() {
            return Err(format!("{}: exists", path.display()));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        crate::config::overwrite_regular_file(&path, format.spec.to_ini().as_bytes())
            .map_err(|e| e.to_string())?;
        self.draft = Draft::from_spec(&format.spec);
        self.editing = Editing::File(path.clone());
        self.confirm_delete = None;
        self.refresh_preview();
        Ok(path)
    }

    /// Writes the draft as a format file at `to`.
    pub fn export(&self, to: &Path) -> std::io::Result<()> {
        crate::config::overwrite_regular_file(to, self.draft.to_spec().to_ini().as_bytes())
    }

    /// Removes the format file `path` and starts a new draft.
    pub fn delete(&mut self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)?;
        self.confirm_delete = None;
        self.new_format();
        Ok(())
    }
}
