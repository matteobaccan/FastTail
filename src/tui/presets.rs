// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The filter presets of the terminal (`p`): the named filter states of `fasttail.ini`
//! (shared with the GUI's presets drop-down), applied to the focused stream or to all,
//! saved from the focused stream, renamed, reordered and deleted. The dialog edits its
//! own copy of the list; the caller stores it and saves after each change.

use crate::i18n::Language;
use crate::i18n_tui::{en, tx, txf};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::filter_preset::{FilterPreset, FilterState};
use crate::log_level::LogLevel;
use crate::settings_model::{self as model, PresetNameProblem};
use crate::tui::form::{CheckBox, FieldKey, ReorderList, TextField};

/// What the dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    List,
    /// Typing a name: a new preset from the focused stream (`rename` is `None`, with the
    /// time range when `with_time` is ticked) or a new name for preset `rename`.
    Name {
        field: TextField,
        rename: Option<usize>,
        with_time: CheckBox,
        /// The time-range box has the keyboard (`Tab` moves between the two).
        on_time: bool,
    },
    /// `d` asked to delete this preset: `Enter` deletes, `Esc` keeps it.
    ConfirmDelete(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetsDialog {
    pub list: ReorderList<FilterPreset>,
    pub mode: Mode,
    /// First list row shown when the list is taller than the dialog.
    pub top: usize,
    /// Why the last name was refused.
    pub problem: Option<&'static str>,
}

/// What a key asks of the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetsKey {
    /// The list changed (rename, move, delete): store it and save.
    Changed,
    /// Apply preset `index` to the focused stream, or to every stream when `all`.
    Apply {
        index: usize,
        all: bool,
    },
    /// Save the focused stream's filters under `name` (with its time range when
    /// `with_time`); a taken name overwrites that preset, as in the GUI.
    Save {
        name: String,
        with_time: bool,
    },
    /// Only the view changed.
    Moved,
    Close,
    Other,
}

fn problem_text(p: PresetNameProblem) -> &'static str {
    match p {
        PresetNameProblem::Empty => en("type a name"),
        PresetNameProblem::Taken => en("another preset has this name"),
    }
}

/// One line about a filter state: terms, level, toggles and time range.
pub fn summary(s: &FilterState, lang: Language) -> String {
    let mut parts = Vec::new();
    if !s.include.is_empty() {
        parts.push(format!("+{}", s.include.join(" +")));
    }
    if !s.exclude.is_empty() {
        parts.push(format!("-{}", s.exclude.join(" -")));
    }
    if s.min_level != LogLevel::Unknown {
        parts.push(format!(">= {}", s.min_level.name()));
    }
    if s.is_regex {
        parts.push(".*".into());
    }
    if s.case_sensitive {
        parts.push("Aa".into());
    }
    if let Some((from, to)) = &s.time {
        parts.push(txf(lang, "time {0}..{1}", &[&from, &to]));
    }
    if parts.is_empty() {
        tx(lang, "no filter").into()
    } else {
        parts.join("  ")
    }
}

impl PresetsDialog {
    pub fn new(presets: &[FilterPreset]) -> Self {
        Self {
            list: ReorderList::new(presets.to_vec()),
            mode: Mode::List,
            top: 0,
            problem: None,
        }
    }

    pub fn presets(&self) -> Vec<FilterPreset> {
        self.list.items.clone()
    }

    /// `s`: name a new preset from the focused stream.
    pub fn start_save(&mut self) {
        self.problem = None;
        self.mode = Mode::Name {
            field: TextField::default(),
            rename: None,
            with_time: CheckBox { on: false },
            on_time: false,
        };
    }

    /// `r`: a new name for the selected preset.
    pub fn start_rename(&mut self) {
        let Some(p) = self.list.selected_item() else {
            return;
        };
        self.problem = None;
        self.mode = Mode::Name {
            field: TextField::new(&p.name),
            rename: Some(self.list.selected),
            with_time: CheckBox { on: false },
            on_time: false,
        };
    }

    /// `Enter` on the name: the save or the rename, when the name is valid.
    pub fn submit_name(&mut self) -> PresetsKey {
        let Mode::Name {
            field,
            rename,
            with_time,
            ..
        } = &self.mode
        else {
            return PresetsKey::Other;
        };
        let name = field.text().trim().to_string();
        // Saving under a taken name overwrites; a rename must pick a free one.
        let problem = match rename {
            Some(i) => model::preset_name_problem(&self.list.items, &name, Some(*i)),
            None => model::preset_name_problem(&[], &name, None),
        };
        if let Some(p) = problem {
            self.problem = Some(problem_text(p));
            return PresetsKey::Moved;
        }
        let (rename, with_time) = (*rename, with_time.on);
        self.mode = Mode::List;
        self.problem = None;
        match rename {
            Some(i) => {
                if let Some(p) = self.list.items.get_mut(i) {
                    p.name = name;
                }
                PresetsKey::Changed
            }
            None => PresetsKey::Save { name, with_time },
        }
    }

    /// Takes in a preset the caller saved: it replaces the one with its name or is
    /// added at the end, and is selected.
    pub fn saved(&mut self, preset: FilterPreset) {
        let i = crate::filter_preset::upsert(&mut self.list.items, preset);
        self.list.selected = i;
    }

    pub fn on_key(&mut self, key: KeyEvent) -> PresetsKey {
        if key.kind == KeyEventKind::Release {
            return PresetsKey::Other;
        }
        match &mut self.mode {
            Mode::ConfirmDelete(i) => {
                let i = *i;
                self.mode = Mode::List;
                return match key.code {
                    KeyCode::Enter | KeyCode::Char('y') => {
                        if i < self.list.items.len() {
                            self.list.items.remove(i);
                            self.list.selected = i.min(self.list.items.len().saturating_sub(1));
                        }
                        PresetsKey::Changed
                    }
                    _ => PresetsKey::Moved,
                };
            }
            Mode::Name {
                field,
                with_time,
                on_time,
                rename,
            } => {
                return match key.code {
                    KeyCode::Enter => self.submit_name(),
                    KeyCode::Esc => {
                        self.mode = Mode::List;
                        self.problem = None;
                        PresetsKey::Moved
                    }
                    KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down
                        if rename.is_none() =>
                    {
                        *on_time = !*on_time;
                        PresetsKey::Moved
                    }
                    _ if *on_time => match with_time.on_key(key) {
                        FieldKey::Edited => PresetsKey::Moved,
                        _ => PresetsKey::Other,
                    },
                    _ => match field.on_key(key) {
                        FieldKey::Edited => {
                            self.problem = None;
                            PresetsKey::Moved
                        }
                        _ => PresetsKey::Other,
                    },
                };
            }
            Mode::List => {}
        }
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let selected = (!self.list.items.is_empty()).then_some(self.list.selected);
        match key.code {
            KeyCode::Esc => PresetsKey::Close,
            KeyCode::Enter => selected.map_or(PresetsKey::Other, |index| PresetsKey::Apply {
                index,
                all: false,
            }),
            KeyCode::Char('A') if plain => selected.map_or(PresetsKey::Other, |index| {
                PresetsKey::Apply { index, all: true }
            }),
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                selected.map_or(PresetsKey::Other, |index| PresetsKey::Apply {
                    index,
                    all: true,
                })
            }
            KeyCode::Char('s') | KeyCode::Char('n') | KeyCode::Insert if plain => {
                self.start_save();
                PresetsKey::Moved
            }
            KeyCode::Char('r') | KeyCode::F(2) if plain && selected.is_some() => {
                self.start_rename();
                PresetsKey::Moved
            }
            KeyCode::Char('d') | KeyCode::Delete if plain => match selected {
                Some(i) => {
                    self.mode = Mode::ConfirmDelete(i);
                    PresetsKey::Moved
                }
                None => PresetsKey::Other,
            },
            _ => {
                let before = (self.list.selected, self.list.items.len());
                let moves = key.modifiers.contains(KeyModifiers::ALT)
                    || matches!(key.code, KeyCode::Char('K') | KeyCode::Char('J'));
                match self.list.on_key(key) {
                    FieldKey::Edited if moves && before.0 != self.list.selected => {
                        PresetsKey::Changed
                    }
                    FieldKey::Edited => PresetsKey::Moved,
                    _ => PresetsKey::Other,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(d: &mut PresetsDialog, s: &str) {
        for c in s.chars() {
            d.on_key(key(KeyCode::Char(c)));
        }
    }

    fn preset(name: &str, include: &str) -> FilterPreset {
        FilterPreset {
            name: name.into(),
            state: FilterState {
                include: vec![include.into()],
                ..FilterState::default()
            },
        }
    }

    #[test]
    fn presets_are_applied_saved_renamed_moved_and_deleted() {
        let mut d = PresetsDialog::new(&[preset("errors", "ERROR"), preset("db", "sql")]);
        assert_eq!(
            d.on_key(key(KeyCode::Enter)),
            PresetsKey::Apply {
                index: 0,
                all: false
            }
        );
        assert_eq!(
            d.on_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            PresetsKey::Apply {
                index: 0,
                all: true
            }
        );
        // Save: a name and the time-range box.
        d.on_key(key(KeyCode::Char('s')));
        typed(&mut d, " payments ");
        d.on_key(key(KeyCode::Tab));
        d.on_key(key(KeyCode::Char(' ')));
        assert_eq!(
            d.on_key(key(KeyCode::Enter)),
            PresetsKey::Save {
                name: "payments".into(),
                with_time: true
            }
        );
        d.saved(preset("payments", "payment"));
        assert_eq!(d.list.selected, 2);
        // Rename to a taken name is refused; to a free one it changes the list.
        d.on_key(key(KeyCode::Char('r')));
        for _ in 0..8 {
            d.on_key(key(KeyCode::Backspace));
        }
        typed(&mut d, "DB");
        assert_eq!(d.on_key(key(KeyCode::Enter)), PresetsKey::Moved);
        assert!(d.problem.is_some());
        d.on_key(key(KeyCode::Backspace));
        d.on_key(key(KeyCode::Backspace));
        typed(&mut d, "pay");
        assert_eq!(d.on_key(key(KeyCode::Enter)), PresetsKey::Changed);
        assert_eq!(d.presets()[2].name, "pay");
        // Alt+Up moves it; Up alone only selects.
        assert_eq!(
            d.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
            PresetsKey::Changed
        );
        assert_eq!(d.presets()[1].name, "pay");
        assert_eq!(d.on_key(key(KeyCode::Up)), PresetsKey::Moved);
        // Delete asks first: Esc keeps it, Enter deletes it.
        d.on_key(key(KeyCode::Char('d')));
        assert_eq!(d.on_key(key(KeyCode::Esc)), PresetsKey::Moved);
        assert_eq!(d.presets().len(), 3);
        d.on_key(key(KeyCode::Char('d')));
        assert_eq!(d.on_key(key(KeyCode::Enter)), PresetsKey::Changed);
        let names: Vec<String> = d.presets().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["pay", "db"]);
        assert_eq!(d.on_key(key(KeyCode::Esc)), PresetsKey::Close);
    }

    #[test]
    fn an_empty_name_is_refused_and_the_summary_lists_the_filters() {
        let mut d = PresetsDialog::new(&[]);
        assert_eq!(d.on_key(key(KeyCode::Enter)), PresetsKey::Other);
        d.start_save();
        assert_eq!(d.on_key(key(KeyCode::Enter)), PresetsKey::Moved);
        assert_eq!(d.problem, Some("type a name"));
        let s = FilterState {
            include: vec!["payment".into()],
            exclude: vec!["DEBUG".into()],
            min_level: LogLevel::Warn,
            ..FilterState::default()
        };
        assert_eq!(summary(&s, Language::En), "+payment  -DEBUG  >= WARN");
        assert_eq!(summary(&FilterState::default(), Language::En), "no filter");
    }
}
