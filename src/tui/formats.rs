// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Log formats dialog of the terminal (palette): the formats of the library (the
//! built-in ones locked) and the files that did not load, then a form for one format as
//! a draft, with the share of the focused stream's entries it reads as a preview. The
//! state, the save, import, export and delete are the window's (`log_format_edit`).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::i18n::Language;
use crate::i18n_tui::{en, tx, txf};
use crate::log_format::{Library, MAX_PATTERNS};
use crate::log_format_edit::{Draft, Editing, FormatsDialog};
use crate::tui::form::{FieldKey, TextField};

/// What a form row edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Name,
    /// Regex `n` (0-based) of the format.
    Pattern(usize),
    TimestampField,
    TimestampFormat,
    TimestampZone,
    LevelField,
    LevelMap,
    MessageField,
    Units,
    EntryStart,
    Files,
    Columns,
}

impl Part {
    /// The row's label (translated where it is drawn).
    pub fn label(self) -> &'static str {
        match self {
            Part::Name => en("Name"),
            Part::Pattern(_) => en("Regex"),
            Part::TimestampField => en("Time field"),
            Part::TimestampFormat => en("Time format"),
            Part::TimestampZone => en("Time zone"),
            Part::LevelField => en("Level field"),
            Part::LevelMap => en("Level map"),
            Part::MessageField => en("Message field"),
            Part::Units => en("Units"),
            Part::EntryStart => en("Entry start"),
            Part::Files => en("Files"),
            Part::Columns => en("Columns"),
        }
    }
}

/// What a key did to the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatsKey {
    /// Only the view changed.
    Moved,
    /// The formats folder changed: the caller loads it again and refreshes the streams.
    Changed,
    /// `i`: the caller asks for a format file to import.
    Import,
    /// `x`: the caller asks for the file to export the format to.
    Export,
    Close,
    Other,
}

pub struct FormatsView {
    pub state: FormatsDialog,
    /// The row of the list: a format of the library, then a file that did not load.
    pub selected: usize,
    /// The form has the keyboard (else the list does).
    pub editing: bool,
    pub fields: Vec<(Part, TextField)>,
    pub focus: usize,
    /// `d` asked to delete the format file being edited: `y` confirms.
    pub confirm_delete: bool,
    /// The outcome of the last action (an error when `true`).
    pub notice: Option<(String, bool)>,
}

impl FormatsView {
    /// The dialog on the formats folder `dir`, previewing over `stream_lines`.
    pub fn new(dir: std::path::PathBuf, stream_lines: Vec<String>) -> Self {
        Self {
            state: FormatsDialog::new(dir, stream_lines),
            selected: 0,
            editing: false,
            fields: Vec::new(),
            focus: 0,
            confirm_delete: false,
            notice: None,
        }
    }

    /// Whether the draft is a built-in format (read-only).
    pub fn read_only(&self) -> bool {
        self.state.editing == Editing::Builtin
    }

    /// Opens the form on the draft: its patterns one per row, plus a spare row.
    fn edit_draft(&mut self) {
        let d = &self.state.draft;
        let mut fields = vec![(Part::Name, TextField::new(&d.name))];
        let patterns: Vec<&str> = d
            .patterns
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        for (i, p) in patterns.iter().enumerate() {
            fields.push((Part::Pattern(i), TextField::new(p)));
        }
        if patterns.len() < MAX_PATTERNS {
            fields.push((Part::Pattern(patterns.len()), TextField::default()));
        }
        for (part, text) in [
            (Part::TimestampField, &d.timestamp_field),
            (Part::TimestampFormat, &d.timestamp_format),
            (Part::TimestampZone, &d.timestamp_zone),
            (Part::LevelField, &d.level_field),
            (Part::LevelMap, &d.level_map),
            (Part::MessageField, &d.message_field),
            (Part::Units, &d.units),
            (Part::EntryStart, &d.entry_start),
            (Part::Files, &d.files),
            (Part::Columns, &d.columns),
        ] {
            fields.push((part, TextField::new(text)));
        }
        self.fields = fields;
        self.focus = 0;
        self.editing = true;
        self.confirm_delete = false;
    }

    /// The draft as the form holds it (the samples are kept).
    fn draft(&self) -> Draft {
        let mut d = Draft {
            samples: self.state.draft.samples.clone(),
            ..Draft::default()
        };
        let mut patterns = Vec::new();
        for (part, field) in &self.fields {
            let text = field.text().to_string();
            match part {
                Part::Name => d.name = text,
                Part::Pattern(_) if !text.trim().is_empty() => patterns.push(text),
                Part::Pattern(_) => {}
                Part::TimestampField => d.timestamp_field = text,
                Part::TimestampFormat => d.timestamp_format = text,
                Part::TimestampZone => d.timestamp_zone = text,
                Part::LevelField => d.level_field = text,
                Part::LevelMap => d.level_map = text,
                Part::MessageField => d.message_field = text,
                Part::Units => d.units = text,
                Part::EntryStart => d.entry_start = text,
                Part::Files => d.files = text,
                Part::Columns => d.columns = text,
            }
        }
        d.patterns = patterns.join("\n");
        d
    }

    /// The draft of the form into the state, and its preview read again.
    fn sync(&mut self) {
        self.state.draft = self.draft();
        self.state.refresh_preview();
    }

    pub fn on_key(&mut self, key: KeyEvent, lib: &Library, lang: Language) -> FormatsKey {
        if key.kind == KeyEventKind::Release {
            return FormatsKey::Other;
        }
        if self.confirm_delete {
            self.confirm_delete = false;
            if key.code == KeyCode::Char('y') {
                if let Editing::File(path) = self.state.editing.clone() {
                    return match self.state.delete(&path) {
                        Ok(()) => {
                            self.editing = false;
                            self.notice =
                                Some((txf(lang, "Deleted {0}", &[&path.display()]), false));
                            FormatsKey::Changed
                        }
                        Err(e) => {
                            self.notice = Some((e.to_string(), true));
                            FormatsKey::Moved
                        }
                    };
                }
            }
            return FormatsKey::Moved;
        }
        if self.editing {
            return self.form_key(key, lib, lang);
        }
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let rows = lib.formats.len() + lib.errors.len();
        match key.code {
            KeyCode::Esc => FormatsKey::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                FormatsKey::Moved
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(rows.saturating_sub(1));
                FormatsKey::Moved
            }
            KeyCode::Enter => match lib.formats.get(self.selected) {
                Some(format) => {
                    self.state.select(format);
                    self.edit_draft();
                    FormatsKey::Moved
                }
                None => FormatsKey::Other,
            },
            KeyCode::Char('n') if plain => {
                self.state.new_format();
                self.edit_draft();
                FormatsKey::Moved
            }
            KeyCode::Char('c') if plain => match lib.formats.get(self.selected) {
                Some(format) => {
                    self.state.select(format);
                    self.state.duplicate();
                    self.edit_draft();
                    FormatsKey::Moved
                }
                None => FormatsKey::Other,
            },
            KeyCode::Char('i') if plain => FormatsKey::Import,
            KeyCode::Char('x') if plain => match lib.formats.get(self.selected) {
                Some(format) => {
                    self.state.select(format);
                    FormatsKey::Export
                }
                None => FormatsKey::Other,
            },
            KeyCode::Char('d') | KeyCode::Delete => match lib.formats.get(self.selected) {
                Some(format) if !format.builtin && format.file.is_some() => {
                    self.state.select(format);
                    self.confirm_delete = true;
                    FormatsKey::Moved
                }
                Some(_) => {
                    self.notice =
                        Some((tx(lang, "A built-in format cannot be deleted").into(), true));
                    FormatsKey::Moved
                }
                None => FormatsKey::Other,
            },
            _ => FormatsKey::Other,
        }
    }

    /// Keys of the form: `Tab` / arrows move, typing edits (the preview follows), `Enter`
    /// saves, `Esc` goes back to the list without saving.
    fn form_key(&mut self, key: KeyEvent, lib: &Library, lang: Language) -> FormatsKey {
        let n = self.fields.len();
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.focus = (self.focus + 1) % n;
                return FormatsKey::Moved;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + n - 1) % n;
                return FormatsKey::Moved;
            }
            _ => {}
        }
        if key.code == KeyCode::Char('d')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(self.state.editing, Editing::File(_))
        {
            self.confirm_delete = true;
            return FormatsKey::Moved;
        }
        if self.read_only() && !matches!(key.code, KeyCode::Esc | KeyCode::Char('c')) {
            if key.code == KeyCode::Enter || matches!(key.code, KeyCode::Char(_)) {
                self.notice = Some((
                    tx(
                        lang,
                        "A built-in format is read-only: c makes a copy to change",
                    )
                    .into(),
                    true,
                ));
            }
            return FormatsKey::Moved;
        }
        if self.read_only() && key.code == KeyCode::Char('c') {
            self.state.duplicate();
            self.edit_draft();
            return FormatsKey::Moved;
        }
        let Some((part, field)) = self.fields.get_mut(self.focus) else {
            return FormatsKey::Other;
        };
        let part = *part;
        match field.on_key(key) {
            FieldKey::Submit => {
                self.sync();
                match self.state.save(lib) {
                    Ok(path) => {
                        self.notice = Some((txf(lang, "Saved {0}", &[&path.display()]), false));
                        self.editing = false;
                        FormatsKey::Changed
                    }
                    Err(e) => {
                        self.notice = Some((save_error(lang, &e, &self.state.draft.name), true));
                        FormatsKey::Moved
                    }
                }
            }
            FieldKey::Cancel => {
                self.editing = false;
                self.notice = None;
                FormatsKey::Moved
            }
            FieldKey::Edited => {
                // Typing in the spare regex row adds the next spare one.
                if let Part::Pattern(i) = part {
                    let last = !self
                        .fields
                        .iter()
                        .any(|(p, _)| matches!(p, Part::Pattern(j) if *j > i));
                    let typed = !self.fields[self.focus].1.text().is_empty();
                    if last && typed && i + 1 < MAX_PATTERNS {
                        self.fields
                            .insert(self.focus + 1, (Part::Pattern(i + 1), TextField::default()));
                    }
                }
                self.sync();
                FormatsKey::Moved
            }
            FieldKey::Other => FormatsKey::Other,
        }
    }

    /// One line on what the draft reads: the share of the entries it matches, or why it
    /// does not compile (`true`: an error).
    pub fn summary(&self, lang: Language) -> (String, bool) {
        let p = &self.state.preview;
        if let Some(e) = &p.error {
            return (txf(lang, "Does not compile: {0}", &[e]), true);
        }
        let mut text = txf(
            lang,
            "Reads {0}% of {1} entries ({2} lines), {3} µs per line",
            &[
                &p.ratio(),
                &p.entries,
                &p.rows.len(),
                &format!("{:.1}", p.micros_per_line),
            ],
        );
        if p.few_entries() {
            text.push_str("  ");
            text.push_str(tx(lang, "(the entry start pattern starts almost no entry)"));
        }
        (text, false)
    }
}

/// The text of an error of `save` / `import`: a name or a file that is taken, or why the
/// format does not load.
pub fn save_error(lang: Language, error: &str, name: &str) -> String {
    if error.ends_with(": exists") {
        txf(
            lang,
            "A format named {0} or its file exists already",
            &[&name],
        )
    } else {
        txf(lang, "The format does not load: {0}", &[&error])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn a_new_format_is_edited_previewed_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        let lines = vec!["2026-10-29 10:00:00 WARN disk".to_string()];
        let mut v = FormatsView::new(dir.path().to_path_buf(), lines);
        let lib = Library::builtin();
        assert_eq!(
            v.on_key(key(KeyCode::Char('n')), &lib, Language::En),
            FormatsKey::Moved
        );
        assert!(v.editing);
        // The name, typed over the default one.
        v.fields[0].1 = TextField::new("Mine");
        v.sync();
        let (summary, error) = v.summary(Language::En);
        assert!(!error, "{summary}");
        assert!(summary.starts_with("Reads 100% of 1 entries"), "{summary}");
        assert_eq!(
            v.on_key(key(KeyCode::Enter), &lib, Language::En),
            FormatsKey::Changed
        );
        let file = dir.path().join(
            crate::log_format::FormatSpec {
                name: "Mine".into(),
                ..Default::default()
            }
            .file_name(),
        );
        assert!(file.exists(), "{file:?}");
        assert!(!v.editing);
    }

    #[test]
    fn a_built_in_format_is_read_only_until_copied() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = FormatsView::new(dir.path().to_path_buf(), Vec::new());
        let lib = Library::builtin();
        v.on_key(key(KeyCode::Enter), &lib, Language::En);
        assert!(v.read_only());
        v.on_key(key(KeyCode::Char('z')), &lib, Language::En);
        assert!(v.notice.as_ref().is_some_and(|(_, e)| *e));
        v.on_key(key(KeyCode::Char('c')), &lib, Language::En);
        assert!(!v.read_only());
        assert!(v.fields[0].1.text().ends_with(" copy"));
    }
}
