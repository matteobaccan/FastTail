// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Log formats" dialog: the built-in formats (locked), the user's format files and
//! the files that did not load with their error; new, duplicate, delete, import and
//! export; an editor of the format as a draft with a live preview of what it reads from
//! its samples and from the focused stream's first lines. Nothing changes for the streams
//! until the draft is saved.

use crate::fields::FieldParser;
use crate::i18n::{t, Language};
use crate::log_format::{self, FormatSpec, Library, LogFormat};
use crate::log_level::LogLevel;
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Lines of the focused stream the preview reads.
pub const PREVIEW_LINES: usize = 200;
/// The preview is computed again this long after the last keystroke.
const PREVIEW_DELAY: Duration = Duration::from_millis(300);
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

/// The text of an error of `save` / `import`: a name or file that is taken, or why the
/// format does not load, after a translated prefix.
fn error_text(lang: Language, error: &str, name: &str) -> String {
    if error.ends_with(": exists") {
        t(lang, "fmt_exists").replace("{0}", name)
    } else {
        format!("{}: {error}", t(lang, "fmt_invalid"))
    }
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
    edited_at: Option<Instant>,
    confirm_delete: Option<PathBuf>,
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
        self.draft.name = format!("{} copy", self.draft.name.trim());
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

    /// Draws the dialog; `Some` when the formats changed or it was closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lib: &Library,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FormatsAction> {
        let mut is_open = true;
        let mut action = None;
        if self
            .edited_at
            .is_some_and(|at| at.elapsed() >= PREVIEW_DELAY)
        {
            self.refresh_preview();
        } else if let Some(at) = self.edited_at {
            ctx.request_repaint_after(PREVIEW_DELAY.saturating_sub(at.elapsed()));
        }
        egui::Window::new(
            RichText::new(format!("◇ {}", t(lang, "fmt_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_log_formats"))
        .open(&mut is_open)
        .default_size([920.0, 620.0])
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    if let Some(a) = self.list(ui, lib, lang, theme) {
                        action = Some(a);
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    if let Some(a) = self.editor(ui, lib, lang, theme) {
                        action = Some(a);
                    }
                });
            });
        });
        if !is_open {
            action = Some(FormatsAction::Close);
        }
        action
    }

    /// The formats, the files that did not load, and the list actions.
    fn list(
        &mut self,
        ui: &mut egui::Ui,
        lib: &Library,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FormatsAction> {
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("fmt_list")
            .max_height(360.0)
            .show(ui, |ui| {
                for format in &lib.formats {
                    let current = match (&self.editing, &format.file) {
                        (Editing::File(p), Some(f)) => p == f,
                        (Editing::Builtin, None) => self.draft.name == format.name(),
                        _ => false,
                    };
                    let label = if format.builtin {
                        format!("🔒 {}", format.name())
                    } else {
                        format!("◇ {}", format.name())
                    };
                    let mut resp = ui.selectable_label(current, RichText::new(label).monospace());
                    resp = match (&format.file, format.builtin) {
                        (_, true) => resp.on_hover_text(t(lang, "fmt_builtin_tip")),
                        (Some(f), _) => resp.on_hover_text(f.display().to_string()),
                        _ => resp,
                    };
                    if resp.clicked() {
                        self.select(format);
                    }
                }
                for (path, error) in &lib.errors {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!("⚠ {name}"))
                            .monospace()
                            .color(theme.error_color()),
                    )
                    .on_hover_text(format!("{}: {error}", t(lang, "fmt_invalid")));
                }
                if !lib.formats.iter().any(|f| !f.builtin) {
                    ui.label(
                        RichText::new(
                            t(lang, "fmt_none").replace("{0}", &self.dir.display().to_string()),
                        )
                        .small()
                        .color(theme.text_dim()),
                    );
                }
            });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(t(lang, "fmt_new")).clicked() {
                self.new_format();
            }
            if ui.button(t(lang, "fmt_duplicate")).clicked() {
                self.duplicate();
            }
            let file = match &self.editing {
                Editing::File(p) => Some(p.clone()),
                _ => None,
            };
            if ui
                .add_enabled(file.is_some(), egui::Button::new(t(lang, "fmt_delete")))
                .on_disabled_hover_text(t(lang, "fmt_builtin_tip"))
                .clicked()
            {
                self.confirm_delete = file;
            }
            if ui.button(t(lang, "fmt_import")).clicked() {
                if let Some(from) = rfd::FileDialog::new()
                    .add_filter("FastTail format", &["ini"])
                    .pick_file()
                {
                    match self.import(lib, &from) {
                        Ok(path) => {
                            self.notice = Some((
                                t(lang, "fmt_saved").replace("{0}", &path.display().to_string()),
                                false,
                            ));
                            action = Some(FormatsAction::Changed);
                        }
                        Err(e) => {
                            let name = e.strip_suffix(": exists").unwrap_or("").to_string();
                            self.notice = Some((error_text(lang, &e, &name), true));
                        }
                    }
                }
            }
            if ui.button(t(lang, "fmt_export")).clicked() {
                let name = self.draft.to_spec().file_name();
                if let Some(to) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                    if let Err(e) = self.export(&to) {
                        self.notice = Some((e.to_string(), true));
                    }
                }
            }
        });
        if let Some(path) = self.confirm_delete.clone() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    t(lang, "fmt_delete_confirm").replace("{0}", &path.display().to_string()),
                )
                .color(theme.warn_color()),
            );
            ui.horizontal(|ui| {
                if ui.button(t(lang, "fmt_delete")).clicked() {
                    match self.delete(&path) {
                        Ok(()) => action = Some(FormatsAction::Changed),
                        Err(e) => self.notice = Some((e.to_string(), true)),
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() {
                    self.confirm_delete = None;
                }
            });
        }
        action
    }

    /// The draft's fields, Save, and the preview.
    fn editor(
        &mut self,
        ui: &mut egui::Ui,
        lib: &Library,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FormatsAction> {
        let mut action = None;
        let editable = self.editable();
        let mut changed = false;
        egui::Grid::new("fmt_fields")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                let mut row =
                    |ui: &mut egui::Ui, key: &str, text: &mut String, multi: bool, hint: &str| {
                        ui.label(RichText::new(t(lang, key)).monospace());
                        let edit = if multi {
                            egui::TextEdit::multiline(text).desired_rows(2)
                        } else {
                            egui::TextEdit::singleline(text)
                        };
                        let resp = ui.add_enabled(
                            editable,
                            edit.font(egui::TextStyle::Monospace)
                                .hint_text(hint)
                                .desired_width(560.0),
                        );
                        changed |= resp.changed();
                        ui.end_row();
                    };
                let d = &mut self.draft;
                row(ui, "fmt_name", &mut d.name, false, "");
                row(
                    ui,
                    "fmt_patterns",
                    &mut d.patterns,
                    true,
                    r"^(?P<ts>\S+) (?P<msg>.*)$",
                );
                row(ui, "fmt_ts_field", &mut d.timestamp_field, false, "ts");
                row(
                    ui,
                    "fmt_ts_format",
                    &mut d.timestamp_format,
                    false,
                    "%d.%m.%y %H:%M:%S,%3f",
                );
                row(ui, "fmt_ts_zone", &mut d.timestamp_zone, false, "local");
                row(ui, "fmt_level_field", &mut d.level_field, false, "level");
                row(
                    ui,
                    "fmt_level_map",
                    &mut d.level_map,
                    false,
                    "W=warn,E=error",
                );
                row(ui, "fmt_msg_field", &mut d.message_field, false, "msg");
                row(
                    ui,
                    "fmt_entry_start",
                    &mut d.entry_start,
                    false,
                    r"^\d{4}-\d\d-\d\d ",
                );
                row(ui, "fmt_files", &mut d.files, false, "payments-*.log");
                row(ui, "fmt_columns", &mut d.columns, false, "ts,level");
                row(ui, "fmt_samples", &mut d.samples, true, "");
            });
        if changed {
            self.edited_at = Some(Instant::now());
            ui.ctx().request_repaint_after(PREVIEW_DELAY);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let valid = self.preview.error.is_none();
            if ui
                .add_enabled(editable && valid, egui::Button::new(t(lang, "fmt_save")))
                .on_disabled_hover_text(if editable {
                    t(lang, "fmt_invalid")
                } else {
                    t(lang, "fmt_builtin_tip")
                })
                .clicked()
            {
                match self.save(lib) {
                    Ok(path) => {
                        self.notice = Some((
                            t(lang, "fmt_saved").replace("{0}", &path.display().to_string()),
                            false,
                        ));
                        action = Some(FormatsAction::Changed);
                    }
                    Err(e) => {
                        let name = self.draft.name.trim().to_string();
                        self.notice = Some((error_text(lang, &e, &name), true));
                    }
                }
            }
            if let Some((text, error)) = &self.notice {
                let color = if *error {
                    theme.error_color()
                } else {
                    theme.text_dim()
                };
                ui.label(RichText::new(text).small().color(color));
            }
        });
        ui.separator();
        self.preview_ui(ui, lang, theme);
        action
    }

    /// The compile error, or the match ratio, the cost and the table of lines read.
    fn preview_ui(&self, ui: &mut egui::Ui, lang: Language, theme: CyberTheme) {
        let p = &self.preview;
        if let Some(error) = &p.error {
            ui.label(
                RichText::new(format!("{}: {error}", t(lang, "fmt_invalid")))
                    .monospace()
                    .color(theme.error_color()),
            );
            return;
        }
        let summary = t(lang, "fmt_match_ratio")
            .replace("{0}", &p.matched.to_string())
            .replace("{1}", &p.entries.to_string())
            .replace("{2}", &p.ratio().to_string());
        let cost = t(lang, "fmt_cost").replace("{0}", &format!("{:.1}", p.micros_per_line));
        ui.label(
            RichText::new(format!("{summary} · {cost}"))
                .monospace()
                .small(),
        );
        if p.few_entries() {
            ui.label(
                RichText::new(t(lang, "fmt_entries_warn"))
                    .small()
                    .color(theme.warn_color()),
            );
        }
        egui::ScrollArea::both()
            .id_salt("fmt_preview")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for row in &p.rows {
                    let color = if !row.starts_entry || row.matched {
                        theme.text_primary()
                    } else {
                        theme.text_dim()
                    };
                    let time = row
                        .time
                        .map(crate::timestamp::format_millis)
                        .unwrap_or_else(|| "-".to_string());
                    let level = if row.level == LogLevel::Unknown {
                        "-".to_string()
                    } else {
                        format!("{:?}", row.level).to_uppercase()
                    };
                    let mark = match (row.sample, row.starts_entry, row.matched) {
                        (_, false, _) => "  ",
                        (_, true, true) => "✔ ",
                        (_, true, false) => "✘ ",
                    };
                    let head = format!("{mark}{time:<19} {level:<5} ");
                    let resp = ui.label(
                        RichText::new(format!("{head}{}", row.text))
                            .monospace()
                            .size(11.0)
                            .color(if row.sample {
                                theme.accent_color()
                            } else {
                                color
                            }),
                    );
                    if !row.fields.is_empty() {
                        let fields: Vec<String> = row
                            .fields
                            .iter()
                            .map(|(k, v)| format!("{k} = {v}"))
                            .collect();
                        resp.on_hover_text(fields.join("\n"));
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_save_reject_non_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let sub_dir = dir.path().join("sub");
        std::fs::create_dir(&sub_dir).unwrap();

        let mut dialog = FormatsDialog::new(dir.path().to_path_buf(), Vec::new());
        dialog.draft.name = "TestFormat".to_string();
        dialog.draft.patterns = r"^(?P<msg>.*)$".to_string();

        // Exporting to a directory target must fail
        let res = dialog.export(&sub_dir);
        assert!(res.is_err(), "exporting to a directory must fail");

        // Saving when the target path is a directory must fail
        dialog.editing = Editing::File(sub_dir.clone());
        let save_res = dialog.save(&Library::default());
        assert!(save_res.is_err(), "saving to a directory must fail");
    }
}
