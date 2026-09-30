// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Settings dialog of the terminal (`,`): the settings that apply to a terminal,
//! in sections, built on the form toolkit. It is filled from `FastTailConfig`, checked
//! with the ranges of `settings_model` (the GUI Settings use the same ones) and written
//! back only when every field is valid. Keys the terminal does not show are left alone.

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::{FastTailConfig, Interface};
use crate::i18n::Language;
use crate::settings_model as model;
use crate::tail_engine::SizeUnit;
use crate::theme::CyberTheme;
use crate::tui::form::{CheckBox, FieldKey, NumberField, RadioList, TextField};

const THEMES: [CyberTheme; 5] = [
    CyberTheme::Tron,
    CyberTheme::Matrix,
    CyberTheme::Blade,
    CyberTheme::Light,
    CyberTheme::Commander,
];
const SIZE_UNITS: [SizeUnit; 4] = [SizeUnit::Bytes, SizeUnit::MB, SizeUnit::GB, SizeUnit::Hex];

/// What a field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Interface,
    Theme,
    Language,
    LanguageAuto,
    LineNumbers,
    LevelColors,
    TimeDelta,
    TimeDeltaGap,
    SizeUnit,
    PollInterval,
    SizeCheckInterval,
    SpoolDir,
    CompressedMaxGb,
    StdinSpoolMaxMb,
    AutoBookmarkMax,
    Sound,
}

/// The input of a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Widget {
    Check(CheckBox),
    Radio(RadioList),
    Number(NumberField),
    Text(TextField),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: Key,
    pub label: &'static str,
    /// The section title drawn above this field, when it starts one.
    pub section: Option<&'static str>,
    pub widget: Widget,
}

impl Field {
    fn new(key: Key, label: &'static str, widget: Widget) -> Self {
        Self {
            key,
            label,
            section: None,
            widget,
        }
    }

    fn starts(mut self, section: &'static str) -> Self {
        self.section = Some(section);
        self
    }

    /// Why the field's value cannot be applied (a number out of its range).
    pub fn problem(&self) -> Option<String> {
        match &self.widget {
            Widget::Number(n) => n.value().err(),
            _ => None,
        }
    }
}

/// The dialog's fields and which one has the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsForm {
    pub fields: Vec<Field>,
    pub focus: usize,
    /// First line shown when the form is taller than the dialog.
    pub top: usize,
    /// `[ OK ]` found fields that cannot be applied: they are marked until edited.
    pub rejected: bool,
}

fn number<T: Copy + Into<u64>>(value: T, range: &std::ops::RangeInclusive<T>) -> Widget {
    Widget::Number(NumberField::new(
        value.into(),
        (*range.start()).into()..=(*range.end()).into(),
    ))
}

fn check(on: bool) -> Widget {
    Widget::Check(CheckBox { on })
}

impl SettingsForm {
    pub fn from_config(c: &FastTailConfig) -> Self {
        let theme = THEMES.iter().position(|t| *t == c.theme).unwrap_or(0);
        let themes: Vec<&str> = ["Tron", "Matrix", "Blade", "Light", "Commander"].to_vec();
        let languages: Vec<&str> = Language::ALL.iter().map(|l| l.name()).collect();
        let language = Language::ALL
            .iter()
            .position(|l| *l == c.language)
            .unwrap_or(0);
        let unit = SIZE_UNITS
            .iter()
            .position(|u| *u == c.size_unit)
            .unwrap_or(0);
        let interface = usize::from(c.interface == Interface::Tui);
        let auto_max = NumberField::new(
            c.auto_bookmark_max as u64,
            *model::AUTO_BOOKMARK_MAX.start() as u64..=*model::AUTO_BOOKMARK_MAX.end() as u64,
        );
        let spool = c
            .spool_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let fields = vec![
            Field::new(
                Key::Interface,
                "Interface at start",
                Widget::Radio(RadioList::new(&["graphical", "terminal"], interface)),
            )
            .starts("General"),
            Field::new(
                Key::Theme,
                "Theme",
                Widget::Radio(RadioList::new(&themes, theme)),
            ),
            Field::new(
                Key::Language,
                "Language",
                Widget::Radio(RadioList::new(&languages, language)),
            ),
            Field::new(
                Key::LanguageAuto,
                "Follow the system language",
                check(c.language_auto),
            ),
            Field::new(Key::LineNumbers, "Line numbers", check(c.show_line_numbers)).starts("View"),
            Field::new(
                Key::LevelColors,
                "Colour rows by level",
                check(c.level_colors),
            ),
            Field::new(
                Key::TimeDelta,
                "Time delta column",
                check(c.show_time_delta),
            ),
            Field::new(
                Key::TimeDeltaGap,
                "Time delta gap (ms)",
                number(c.time_delta_gap_ms, &model::TIME_DELTA_GAP_MS),
            ),
            Field::new(
                Key::SizeUnit,
                "File size in",
                Widget::Radio(RadioList::new(&["bytes", "MB", "GB", "hex"], unit)),
            ),
            Field::new(
                Key::PollInterval,
                "Poll interval (ms)",
                number(c.poll_interval_ms, &model::POLL_INTERVAL_MS),
            )
            .starts("Performance and refresh"),
            Field::new(
                Key::SizeCheckInterval,
                "Size check interval (ms)",
                number(c.size_check_interval_ms, &model::SIZE_CHECK_INTERVAL_MS),
            ),
            Field::new(
                Key::SpoolDir,
                "Spool directory (empty: default)",
                Widget::Text(TextField::new(&spool)),
            ),
            Field::new(
                Key::CompressedMaxGb,
                "Largest decompressed file (GB)",
                number(c.compressed_max_gb, &model::COMPRESSED_MAX_GB),
            ),
            Field::new(
                Key::StdinSpoolMaxMb,
                "Standard input spool (MB)",
                number(c.stdin_spool_max_mb, &model::STDIN_SPOOL_MAX_MB),
            ),
            Field::new(
                Key::AutoBookmarkMax,
                "Automatic bookmarks per file",
                Widget::Number(auto_max),
            )
            .starts("New streams"),
            Field::new(Key::Sound, "Sound alerts", check(c.sound_enabled)).starts("Sound"),
        ];
        Self {
            fields,
            focus: 0,
            top: 0,
            rejected: false,
        }
    }

    /// Sends a key to the focused field; `Tab` / `Shift+Tab` and `↑` / `↓` (outside a
    /// radio list) move between fields. `Enter` and `Esc` come back to the dialog.
    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        match key.code {
            KeyCode::Tab => return self.move_focus(1),
            KeyCode::BackTab => return self.move_focus(-1),
            KeyCode::Enter => return FieldKey::Submit,
            KeyCode::Esc => return FieldKey::Cancel,
            _ => {}
        }
        let is_number = matches!(self.fields[self.focus].widget, Widget::Number(_));
        // Up / Down step a number and move a radio list's choice only with Left /
        // Right; otherwise they walk the fields.
        match key.code {
            KeyCode::Up if !is_number => return self.move_focus(-1),
            KeyCode::Down if !is_number => return self.move_focus(1),
            _ => {}
        }
        let field = &mut self.fields[self.focus];
        let done = match &mut field.widget {
            Widget::Check(c) => c.on_key(key),
            Widget::Radio(r) if matches!(key.code, KeyCode::Left | KeyCode::Right) => r.on_key(key),
            Widget::Radio(_) => FieldKey::Other,
            Widget::Number(n) => n.on_key(key),
            Widget::Text(t) => t.on_key(key),
        };
        if done == FieldKey::Edited {
            self.rejected = false;
        }
        done
    }

    fn move_focus(&mut self, delta: isize) -> FieldKey {
        let n = self.fields.len() as isize;
        self.focus = (self.focus as isize + delta).rem_euclid(n) as usize;
        FieldKey::Edited
    }

    /// The fields that cannot be applied, with why.
    pub fn problems(&self) -> Vec<(usize, String)> {
        self.fields
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.problem().map(|p| (i, p)))
            .collect()
    }

    /// Writes every field into `c` when all of them are valid; otherwise leaves `c`
    /// as it is and returns the fields to fix.
    pub fn apply(&self, c: &mut FastTailConfig) -> Result<(), Vec<(usize, String)>> {
        let problems = self.problems();
        if !problems.is_empty() {
            return Err(problems);
        }
        for f in &self.fields {
            let num = || match &f.widget {
                Widget::Number(n) => n.value().unwrap_or_default(),
                _ => 0,
            };
            let on = matches!(&f.widget, Widget::Check(cb) if cb.on);
            let choice = match &f.widget {
                Widget::Radio(r) => r.selected,
                _ => 0,
            };
            match f.key {
                Key::Interface => {
                    c.interface = if choice == 1 {
                        Interface::Tui
                    } else {
                        Interface::Gui
                    }
                }
                Key::Theme => c.theme = THEMES[choice],
                Key::Language => c.language = Language::ALL[choice],
                Key::LanguageAuto => c.language_auto = on,
                Key::LineNumbers => c.show_line_numbers = on,
                Key::LevelColors => c.level_colors = on,
                Key::TimeDelta => c.show_time_delta = on,
                Key::TimeDeltaGap => c.time_delta_gap_ms = num(),
                Key::SizeUnit => c.size_unit = SIZE_UNITS[choice],
                Key::PollInterval => c.poll_interval_ms = num() as u32,
                Key::SizeCheckInterval => c.size_check_interval_ms = num() as u32,
                Key::SpoolDir => {
                    let text = match &f.widget {
                        Widget::Text(t) => t.text().trim().to_string(),
                        _ => String::new(),
                    };
                    c.spool_dir = (!text.is_empty()).then(|| text.into());
                }
                Key::CompressedMaxGb => c.compressed_max_gb = num() as u32,
                Key::StdinSpoolMaxMb => c.stdin_spool_max_mb = num() as u32,
                Key::AutoBookmarkMax => c.auto_bookmark_max = num() as usize,
                Key::Sound => c.sound_enabled = on,
            }
        }
        if c.language_auto {
            c.language = Language::detect();
        }
        Ok(())
    }

    /// The form as lines: section titles, then `label  value` per field. Returns the
    /// lines and, for each, the field it shows (`None` for a title or a gap).
    pub fn lines(&self) -> Vec<(String, Option<usize>)> {
        let mut out = Vec::new();
        for (i, f) in self.fields.iter().enumerate() {
            if let Some(s) = f.section {
                if !out.is_empty() {
                    out.push((String::new(), None));
                }
                out.push((s.to_string(), None));
            }
            let value = match &f.widget {
                Widget::Check(c) => c.text(""),
                // A short list shows every choice, a long one (languages) the current.
                Widget::Radio(r) if r.text().chars().count() <= 36 => r.text(),
                Widget::Radio(r) => format!(
                    "< {} >",
                    r.options.get(r.selected).map(String::as_str).unwrap_or("")
                ),
                Widget::Number(n) => format!("[{}]", n.field.text()),
                Widget::Text(t) => format!("[{}]", t.text()),
            };
            out.push((format!("  {:<34}{value}", f.label), Some(i)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn focus_on(form: &mut SettingsForm, k: Key) {
        form.focus = form.fields.iter().position(|f| f.key == k).unwrap();
    }

    #[test]
    fn an_untouched_form_writes_back_what_it_read() {
        let mut c = FastTailConfig {
            theme: CyberTheme::Blade,
            poll_interval_ms: 300,
            sound_enabled: true,
            spool_dir: Some("/var/spool/ft".into()),
            size_unit: SizeUnit::GB,
            interface: Interface::Tui,
            ..Default::default()
        };
        let before = c.clone();
        SettingsForm::from_config(&c).apply(&mut c).unwrap();
        assert_eq!(c.theme, before.theme);
        assert_eq!(c.poll_interval_ms, 300);
        assert!(c.sound_enabled);
        assert_eq!(c.spool_dir, before.spool_dir);
        assert_eq!(c.size_unit, SizeUnit::GB);
        assert_eq!(c.interface, Interface::Tui);
        let ini = |c: &FastTailConfig| {
            let mut buf = Vec::new();
            c.to_ini().write_to(&mut buf).unwrap();
            buf
        };
        assert_eq!(ini(&c), ini(&before), "every key as it was");
    }

    #[test]
    fn edits_apply_and_an_out_of_range_number_blocks_ok() {
        let mut c = FastTailConfig::default();
        let mut form = SettingsForm::from_config(&c);
        focus_on(&mut form, Key::Theme);
        form.on_key(key(KeyCode::Right));
        focus_on(&mut form, Key::Sound);
        form.on_key(key(KeyCode::Char(' ')));
        focus_on(&mut form, Key::PollInterval);
        form.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for ch in "9".chars() {
            form.on_key(key(KeyCode::Char(ch)));
        }
        let problems = form.apply(&mut c).unwrap_err();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].1, "50 to 5000", "the GUI's range");
        assert_eq!(c.theme, CyberTheme::Tron, "nothing applied");

        form.on_key(key(KeyCode::Char('0')));
        form.on_key(key(KeyCode::Char('0')));
        form.apply(&mut c).unwrap();
        assert_eq!(c.poll_interval_ms, 900);
        assert_eq!(c.theme, CyberTheme::Matrix);
        assert_eq!(c.sound_enabled, !FastTailConfig::default().sound_enabled);
    }

    #[test]
    fn tab_and_arrows_walk_the_fields_and_wrap() {
        let mut form = SettingsForm::from_config(&FastTailConfig::default());
        let n = form.fields.len();
        form.on_key(key(KeyCode::BackTab));
        assert_eq!(form.focus, n - 1);
        form.on_key(key(KeyCode::Down));
        assert_eq!(form.focus, 0);
        form.on_key(key(KeyCode::Tab));
        assert_eq!(form.fields[form.focus].key, Key::Theme);
        // On a number, Up / Down step the value instead.
        focus_on(&mut form, Key::PollInterval);
        let before = form.fields[form.focus].clone();
        form.on_key(key(KeyCode::Up));
        assert_eq!(form.fields[form.focus].key, Key::PollInterval);
        assert_ne!(form.fields[form.focus], before);
        assert_eq!(form.on_key(key(KeyCode::Enter)), FieldKey::Submit);
        assert_eq!(form.on_key(key(KeyCode::Esc)), FieldKey::Cancel);
    }

    #[test]
    fn the_lines_show_sections_and_values() {
        let form = SettingsForm::from_config(&FastTailConfig::default());
        let lines = form.lines();
        let text: Vec<&str> = lines.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(text[0], "General");
        assert!(text
            .iter()
            .any(|l| l.contains("Theme") && l.contains("< Tron >")));
        assert!(text.iter().any(|l| l.contains("Poll interval (ms)")));
        assert!(text.contains(&"Performance and refresh"));
        let fields = lines.iter().filter(|(_, f)| f.is_some()).count();
        assert_eq!(fields, form.fields.len());
    }
}
