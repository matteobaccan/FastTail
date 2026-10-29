// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Settings dialog of the terminal (`,`): the settings that apply to a terminal,
//! in sections, built on the form toolkit. It is filled from `FastTailConfig`, checked
//! with the ranges of `settings_model` (the GUI Settings use the same ones) and written
//! back only when every field is valid. Keys the terminal does not show are left alone.

use crossterm::event::{KeyCode, KeyEvent};

use crate::auto_highlight::TokenKind;
use crate::config::{FastTailConfig, Interface};
use crate::i18n::Language;
use crate::i18n_tui::{en, tx, txf};
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
    /// The automatic highlighting switch (`auto_highlight`) and each of its kinds
    /// (`auto_highlight_kinds`).
    AutoHighlight,
    Token(TokenKind),
    TimeDelta,
    TimeDeltaGap,
    /// `pin_time_column`: the time column pinned in the column view of new streams.
    PinTimeColumn,
    /// `rotations`: a file with rotations next to it offers them, opens with them, or not.
    Rotations,
    /// `level_detect_bytes`: where the level is looked for in streams without their own.
    LevelBytes,
    SizeUnit,
    Telemetry,
    PollInterval,
    SizeCheckInterval,
    SpoolDir,
    CompressedMaxGb,
    StdinSpoolMaxMb,
    SystemBacklog,
    DockerCommand,
    JournalctlCommand,
    SshProgram,
    RemoteInitialMb,
    RemoteSpoolMaxMb,
    ListenerSpoolMaxMb,
    ListenerLinesPerSec,
    ListenerMbPerSec,
    AutoBookmarkMax,
    Sound,
    FlashOnAlert,
    Notifications,
    NotifyAlways,
    /// Opens the external tools editor (`Enter`); the tools are not part of the form.
    Tools,
    /// A preset of `source_links::PRESETS`: picking one fills `EditorCommand`.
    EditorPreset,
    /// `editor_command`: the editor a source location opens in.
    EditorCommand,
    /// `source_roots`: folders separated by `;`.
    SourceRoots,
    /// The idle lock switch (`lock_enabled`), the idle minutes
    /// (`screensaver_timeout_mins`), a new PIN typed twice, and removing the PIN.
    LockEnabled,
    IdleMinutes,
    PinNew,
    PinConfirm,
    RemovePin,
}

/// The input of a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Widget {
    Check(CheckBox),
    Radio(RadioList),
    Number(NumberField),
    Text(TextField),
    /// A row that opens another dialog with `Enter`, showing this text.
    Button(String),
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

    /// Why the field's value cannot be applied (a number out of its range), in `lang`.
    pub fn problem(&self, lang: Language) -> Option<String> {
        match &self.widget {
            Widget::Number(n) => n.problem(lang),
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
    /// A PIN is stored now (the idle lock needs one, set here or before).
    has_pin: bool,
    /// The language of the texts (the one being chosen, while Settings is open).
    pub lang: Language,
}

/// The choice of the editor preset row: "as typed", then each preset.
fn preset_of(command: &str) -> usize {
    crate::source_links::PRESETS
        .iter()
        .position(|(_, t)| *t == command.trim())
        .map_or(0, |i| i + 1)
}

fn editor_presets(command: &str, lang: Language) -> RadioList {
    let mut options = vec![tx(lang, "as typed")];
    options.extend(crate::source_links::PRESETS.iter().map(|(name, _)| *name));
    RadioList::new(&options, preset_of(command))
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

/// One kind of the automatic highlighting, under its switch, as in the GUI's Settings.
fn token_field(c: &FastTailConfig, kind: TokenKind) -> Field {
    let label = match kind {
        TokenKind::Ip => en("  IP addresses"),
        TokenKind::Uuid => en("  Identifiers (UUID)"),
        TokenKind::Url => en("  Web addresses (URL)"),
        TokenKind::Duration => en("  Durations"),
        TokenKind::Path => en("  File paths"),
    };
    Field::new(
        Key::Token(kind),
        label,
        check(c.auto_highlight_kinds.contains(kind)),
    )
}

/// The interface choice. A build without the window (the Linux terminal-only archive)
/// shows the terminal as the only one, and says why; on Windows, where the window is
/// another file, the graphical entry says when that file is missing.
fn interface_widget(selected: usize, lang: Language) -> Widget {
    if !cfg!(feature = "gui") {
        return Widget::Button(
            tx(
                lang,
                "terminal - the graphical interface is not in this build",
            )
            .into(),
        );
    }
    let missing =
        cfg!(windows) && !crate::handoff::sibling_present(&crate::handoff::gui_exe_name());
    let graphical = if missing {
        tx(lang, "graphical (fasttail.exe missing)")
    } else {
        tx(lang, "graphical")
    };
    Widget::Radio(RadioList::new(&[graphical, tx(lang, "terminal")], selected))
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
        let level_bytes = NumberField::new(
            c.level_detect_bytes as u64,
            *model::LEVEL_DETECT_BYTES.start() as u64..=*model::LEVEL_DETECT_BYTES.end() as u64,
        );
        let spool = c
            .spool_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let fields = vec![
            Field::new(
                Key::Interface,
                en("Interface at start"),
                interface_widget(interface, c.language),
            )
            .starts(en("General")),
            Field::new(
                Key::Theme,
                en("Theme"),
                Widget::Radio(RadioList::new(&themes, theme)),
            ),
            Field::new(
                Key::Language,
                en("Language"),
                Widget::Radio(RadioList::new(&languages, language)),
            ),
            Field::new(
                Key::LanguageAuto,
                en("Follow the system language"),
                check(c.language_auto),
            ),
            Field::new(
                Key::LevelColors,
                en("Colour rows by level"),
                check(c.level_colors),
            )
            .starts(en("View")),
            Field::new(
                Key::AutoHighlight,
                en("Automatic token highlighting"),
                check(c.auto_highlight),
            ),
            token_field(c, TokenKind::Ip),
            token_field(c, TokenKind::Uuid),
            token_field(c, TokenKind::Url),
            token_field(c, TokenKind::Duration),
            token_field(c, TokenKind::Path),
            Field::new(
                Key::TimeDeltaGap,
                en("Time delta gap (ms)"),
                number(c.time_delta_gap_ms, &model::TIME_DELTA_GAP_MS),
            ),
            Field::new(
                Key::SizeUnit,
                en("File size in"),
                Widget::Radio(RadioList::new(
                    &[tx(c.language, "bytes"), "MB", "GB", tx(c.language, "hex")],
                    unit,
                )),
            ),
            Field::new(
                Key::Telemetry,
                en("CPU and RAM in the top bar"),
                check(c.telemetry_enabled),
            ),
            Field::new(
                Key::PollInterval,
                en("Poll interval (ms)"),
                number(c.poll_interval_ms, &model::POLL_INTERVAL_MS),
            )
            .starts(en("Performance and refresh")),
            Field::new(
                Key::SizeCheckInterval,
                en("Size check interval (ms)"),
                number(c.size_check_interval_ms, &model::SIZE_CHECK_INTERVAL_MS),
            ),
            Field::new(
                Key::SpoolDir,
                en("Spool directory (empty: default)"),
                Widget::Text(TextField::new(&spool)),
            ),
            Field::new(
                Key::CompressedMaxGb,
                en("Largest decompressed file (GB)"),
                number(c.compressed_max_gb, &model::COMPRESSED_MAX_GB),
            ),
            Field::new(
                Key::StdinSpoolMaxMb,
                en("Standard input spool (MB)"),
                number(c.stdin_spool_max_mb, &model::STDIN_SPOOL_MAX_MB),
            ),
            Field::new(
                Key::SystemBacklog,
                en("System source backlog (lines)"),
                number(
                    c.system_source_backlog_lines,
                    &model::SYSTEM_SOURCE_BACKLOG_LINES,
                ),
            ),
            Field::new(
                Key::DockerCommand,
                en("Docker command"),
                Widget::Text(TextField::new(&c.docker_command)),
            ),
            Field::new(
                Key::JournalctlCommand,
                en("journalctl command"),
                Widget::Text(TextField::new(&c.journalctl_command)),
            ),
            Field::new(
                Key::SshProgram,
                en("SSH client"),
                Widget::Text(TextField::new(&c.ssh_program)),
            ),
            Field::new(
                Key::RemoteInitialMb,
                en("Remote files: first fetch (MB, 0: only new lines)"),
                number(c.remote_initial_mb, &model::REMOTE_INITIAL_MB),
            ),
            Field::new(
                Key::RemoteSpoolMaxMb,
                en("Remote files: spool (MB)"),
                number(c.remote_spool_max_mb, &model::STDIN_SPOOL_MAX_MB),
            ),
            Field::new(
                Key::ListenerSpoolMaxMb,
                en("Listeners: spool (MB)"),
                number(c.listener_spool_max_mb, &model::STDIN_SPOOL_MAX_MB),
            ),
            Field::new(
                Key::ListenerLinesPerSec,
                en("Listeners: lines per second at most"),
                number(
                    c.listener_max_lines_per_sec,
                    &model::LISTENER_MAX_LINES_PER_SEC,
                ),
            ),
            Field::new(
                Key::ListenerMbPerSec,
                en("Listeners: MB per second at most"),
                number(c.listener_max_mb_per_sec, &model::LISTENER_MAX_MB_PER_SEC),
            ),
            Field::new(
                Key::LineNumbers,
                en("Line numbers in new streams"),
                check(c.show_line_numbers),
            )
            .starts(en("New streams and bookmarks - each stream keeps its own")),
            Field::new(
                Key::TimeDelta,
                en("Time delta column in new streams"),
                check(c.show_time_delta),
            ),
            Field::new(
                Key::PinTimeColumn,
                en("Pin the time column in the column view"),
                check(c.pin_time_column),
            ),
            Field::new(
                Key::Rotations,
                en("Rotations of a file"),
                Widget::Radio(RadioList::new(
                    &[
                        tx(c.language, "ask"),
                        tx(c.language, "always"),
                        tx(c.language, "never"),
                    ],
                    match c.rotations {
                        crate::rotation::Policy::Ask => 0,
                        crate::rotation::Policy::Always => 1,
                        crate::rotation::Policy::Never => 2,
                    },
                )),
            ),
            Field::new(
                Key::LevelBytes,
                en("Level searched in the first N bytes"),
                Widget::Number(level_bytes),
            ),
            Field::new(
                Key::AutoBookmarkMax,
                en("Automatic bookmarks per file"),
                Widget::Number(auto_max),
            ),
            Field::new(Key::Sound, en("Sound alerts"), check(c.sound_enabled)).starts(en("Sound")),
            Field::new(
                Key::FlashOnAlert,
                en("Bell when a hidden stream has an alert"),
                check(c.flash_on_alert),
            ),
            Field::new(
                Key::Notifications,
                en("Desktop notifications"),
                check(c.notifications_enabled),
            ),
            Field::new(
                Key::NotifyAlways,
                en("Also when the stream is on screen"),
                check(c.notify_always),
            ),
            Field::new(
                Key::Tools,
                en("External tools"),
                Widget::Button(txf(
                    c.language,
                    en("{0} defined - Enter edits them"),
                    &[&c.external_tools.len()],
                )),
            )
            .starts(en("External tools")),
            Field::new(
                Key::EditorPreset,
                en("Editor preset"),
                Widget::Radio(editor_presets(&c.editor_command, c.language)),
            ),
            Field::new(
                Key::EditorCommand,
                en("Editor for source locations ({file} {line} {col})"),
                Widget::Text(TextField::new(&c.editor_command)),
            ),
            Field::new(
                Key::SourceRoots,
                en("Source folders (separated by ;)"),
                Widget::Text(TextField::new(&crate::source_links::roots_text(
                    &c.source_roots,
                ))),
            ),
            Field::new(
                Key::LockEnabled,
                en("Lock when idle"),
                check(c.lock_enabled),
            )
            .starts(en("PIN lock - deters onlookers, it is not security")),
            Field::new(
                Key::IdleMinutes,
                en("Idle minutes before the lock"),
                number(c.screensaver_timeout_mins, &model::SCREENSAVER_TIMEOUT_MINS),
            ),
            Field::new(
                Key::PinNew,
                if c.lock_pin.is_empty() {
                    en("New PIN (4-12 digits)")
                } else {
                    en("Change the PIN (4-12 digits)")
                },
                Widget::Text(TextField::default()),
            ),
            Field::new(
                Key::PinConfirm,
                en("The new PIN again"),
                Widget::Text(TextField::default()),
            ),
            Field::new(Key::RemovePin, en("Remove the PIN"), check(false)),
        ];
        Self {
            fields,
            focus: 0,
            top: 0,
            rejected: false,
            lang: c.language,
            has_pin: !c.lock_pin.is_empty(),
        }
    }

    /// The text of field `key` (empty for a field that is not text).
    fn text_of(&self, key: Key) -> &str {
        match self.fields.iter().find(|f| f.key == key).map(|f| &f.widget) {
            Some(Widget::Text(t)) => t.text(),
            _ => "",
        }
    }

    fn checked(&self, key: Key) -> bool {
        matches!(
            self.fields.iter().find(|f| f.key == key).map(|f| &f.widget),
            Some(Widget::Check(c)) if c.on
        )
    }

    /// Sends a key to the focused field; `Tab` / `Shift+Tab` and `↑` / `↓` move between
    /// fields, on every kind of field, and `←` / `→` change the value (a radio list's
    /// choice, a number by one). `Enter` and `Esc` come back to the dialog.
    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        match key.code {
            KeyCode::Tab | KeyCode::Down => return self.move_focus(1),
            KeyCode::BackTab | KeyCode::Up => return self.move_focus(-1),
            KeyCode::Enter => return FieldKey::Submit,
            KeyCode::Esc => return FieldKey::Cancel,
            _ => {}
        }
        let field = &mut self.fields[self.focus];
        let done = match &mut field.widget {
            Widget::Check(c) => c.on_key(key),
            Widget::Radio(r) if matches!(key.code, KeyCode::Left | KeyCode::Right) => r.on_key(key),
            Widget::Radio(_) => FieldKey::Other,
            Widget::Number(n) => n.on_key(key),
            Widget::Text(t) => t.on_key(key),
            Widget::Button(_) => FieldKey::Other,
        };
        if done == FieldKey::Edited {
            self.rejected = false;
            self.follow_editor_preset();
            // Picking a language stops following the system's, as in the GUI.
            if self.fields[self.focus].key == Key::Language {
                if let Some(Widget::Check(c)) = self
                    .fields
                    .iter_mut()
                    .find(|f| f.key == Key::LanguageAuto)
                    .map(|f| &mut f.widget)
                {
                    c.on = false;
                }
            }
        }
        done
    }

    /// A preset picked fills the editor command; a command typed picks its preset, or
    /// "as typed".
    fn follow_editor_preset(&mut self) {
        let key = self.fields[self.focus].key;
        let at = |fields: &[Field], k: Key| fields.iter().position(|f| f.key == k);
        let (Some(p), Some(c)) = (
            at(&self.fields, Key::EditorPreset),
            at(&self.fields, Key::EditorCommand),
        ) else {
            return;
        };
        match key {
            Key::EditorPreset => {
                let choice = match &self.fields[p].widget {
                    Widget::Radio(r) => r.selected,
                    _ => 0,
                };
                if let Some((_, template)) = choice
                    .checked_sub(1)
                    .and_then(|i| crate::source_links::PRESETS.get(i))
                {
                    self.fields[c].widget = Widget::Text(TextField::new(template));
                }
            }
            Key::EditorCommand => {
                let text = match &self.fields[c].widget {
                    Widget::Text(t) => t.text().trim().to_string(),
                    _ => return,
                };
                if let Widget::Radio(r) = &mut self.fields[p].widget {
                    r.selected = preset_of(&text);
                }
            }
            _ => {}
        }
    }

    fn move_focus(&mut self, delta: isize) -> FieldKey {
        let n = self.fields.len() as isize;
        self.focus = (self.focus as isize + delta).rem_euclid(n) as usize;
        FieldKey::Edited
    }

    /// The fields that cannot be applied, with why.
    pub fn problems(&self) -> Vec<(usize, String)> {
        let new_pin = self.text_of(Key::PinNew).trim().to_string();
        let remove = self.checked(Key::RemovePin);
        let will_have_pin = !remove && (self.has_pin || !new_pin.is_empty());
        self.fields
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let pin = match f.key {
                    Key::PinNew if !new_pin.is_empty() && !crate::lock::is_valid_pin(&new_pin) => {
                        Some(tx(self.lang, "4 to 12 digits").to_string())
                    }
                    Key::PinConfirm if self.text_of(Key::PinConfirm).trim() != new_pin => {
                        Some(tx(self.lang, "does not match the new PIN").to_string())
                    }
                    // Removing the PIN switches the idle lock off too, as in the GUI.
                    Key::LockEnabled
                        if self.checked(Key::LockEnabled) && !remove && !will_have_pin =>
                    {
                        Some(tx(self.lang, "set a PIN first").to_string())
                    }
                    _ => None,
                };
                pin.or_else(|| f.problem(self.lang)).map(|p| (i, p))
            })
            .collect()
    }

    /// Writes every field into `c` when all of them are valid; otherwise leaves `c`
    /// as it is and returns the fields to fix.
    pub fn apply(&self, c: &mut FastTailConfig) -> Result<(), Vec<(usize, String)>> {
        let problems = self.problems();
        if !problems.is_empty() {
            return Err(problems);
        }
        self.write(c, &[], false);
        Ok(())
    }

    /// Writes the fields that can apply while the dialog is open, as the GUI's Settings
    /// do: every valid field but the PIN ones and the interface (which wait for OK).
    pub fn apply_live(&self, c: &mut FastTailConfig) {
        let skip: Vec<usize> = self.problems().into_iter().map(|(i, _)| i).collect();
        self.write(c, &skip, true);
    }

    /// Writes the fields into `c`, but those at the positions in `skip`; `live` also
    /// leaves out the PIN fields, the idle lock and the interface.
    fn write(&self, c: &mut FastTailConfig, skip: &[usize], live: bool) {
        for (i, f) in self.fields.iter().enumerate() {
            let waits = matches!(
                f.key,
                Key::PinNew | Key::PinConfirm | Key::RemovePin | Key::LockEnabled | Key::Interface
            );
            if skip.contains(&i) || (live && waits) {
                continue;
            }
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
                // Only a choice changes it (not the terminal-only build's note).
                Key::Interface if matches!(f.widget, Widget::Radio(_)) => {
                    c.interface = if choice == 1 {
                        Interface::Tui
                    } else {
                        Interface::Gui
                    }
                }
                Key::Interface => {}
                Key::Theme => c.theme = THEMES[choice],
                Key::Language => c.language = Language::ALL[choice],
                Key::LanguageAuto => c.language_auto = on,
                Key::LineNumbers => c.show_line_numbers = on,
                Key::LevelColors => c.level_colors = on,
                Key::AutoHighlight => c.auto_highlight = on,
                Key::Token(kind) => c.auto_highlight_kinds.set(kind, on),
                Key::TimeDelta => c.show_time_delta = on,
                Key::PinTimeColumn => c.pin_time_column = on,
                Key::Rotations => {
                    c.rotations = match choice {
                        1 => crate::rotation::Policy::Always,
                        2 => crate::rotation::Policy::Never,
                        _ => crate::rotation::Policy::Ask,
                    }
                }
                Key::TimeDeltaGap => c.time_delta_gap_ms = num(),
                Key::LevelBytes => c.level_detect_bytes = num() as usize,
                Key::SizeUnit => c.size_unit = SIZE_UNITS[choice],
                Key::Telemetry => c.telemetry_enabled = on,
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
                Key::SystemBacklog => c.system_source_backlog_lines = num() as u32,
                Key::RemoteInitialMb => c.remote_initial_mb = num() as u32,
                Key::RemoteSpoolMaxMb => c.remote_spool_max_mb = num() as u32,
                Key::ListenerSpoolMaxMb => c.listener_spool_max_mb = num() as u32,
                Key::ListenerLinesPerSec => c.listener_max_lines_per_sec = num() as u32,
                Key::ListenerMbPerSec => c.listener_max_mb_per_sec = num() as u32,
                Key::DockerCommand | Key::JournalctlCommand | Key::SshProgram => {
                    let text = match &f.widget {
                        Widget::Text(t) => t.text().trim().to_string(),
                        _ => String::new(),
                    };
                    let (value, default) = match f.key {
                        Key::DockerCommand => (
                            &mut c.docker_command,
                            crate::system_source::DEFAULT_DOCKER_COMMAND,
                        ),
                        Key::SshProgram => {
                            (&mut c.ssh_program, crate::ssh_source::DEFAULT_SSH_PROGRAM)
                        }
                        _ => (
                            &mut c.journalctl_command,
                            crate::system_source::DEFAULT_JOURNALCTL_COMMAND,
                        ),
                    };
                    *value = if text.is_empty() {
                        default.to_string()
                    } else {
                        text
                    };
                }
                Key::AutoBookmarkMax => c.auto_bookmark_max = num() as usize,
                Key::Sound => c.sound_enabled = on,
                Key::FlashOnAlert => c.flash_on_alert = on,
                Key::Notifications => c.notifications_enabled = on,
                Key::NotifyAlways => c.notify_always = on,
                Key::EditorCommand | Key::SourceRoots => {
                    let text = match &f.widget {
                        Widget::Text(t) => t.text().trim().to_string(),
                        _ => String::new(),
                    };
                    if f.key == Key::EditorCommand {
                        c.editor_command = text;
                    } else {
                        c.source_roots = crate::source_links::parse_roots(&text);
                    }
                }
                Key::Tools | Key::PinConfirm | Key::EditorPreset => {}
                Key::LockEnabled => c.lock_enabled = on,
                Key::IdleMinutes => c.screensaver_timeout_mins = num() as u32,
                Key::PinNew => {
                    let pin = match &f.widget {
                        Widget::Text(t) => t.text().trim().to_string(),
                        _ => String::new(),
                    };
                    if !pin.is_empty() {
                        c.lock_pin = crate::lock::scramble_pin(&pin);
                    }
                }
                // Last in the form, so it wins over a new PIN typed with it.
                Key::RemovePin if on => {
                    c.lock_pin.clear();
                    c.lock_enabled = false;
                }
                Key::RemovePin => {}
            }
        }
        if c.language_auto {
            c.language = Language::detect();
        }
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
                out.push((tx(self.lang, s).to_string(), None));
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
                // A PIN never shows: one `*` per character typed.
                Widget::Text(t) if matches!(f.key, Key::PinNew | Key::PinConfirm) => {
                    format!("[{}]", "*".repeat(t.text().chars().count()))
                }
                Widget::Text(t) => format!("[{}]", t.text()),
                Widget::Button(text) => format!("> {text}"),
            };
            let label = tx(self.lang, f.label);
            let pad = 34usize.saturating_sub(unicode_width::UnicodeWidthStr::width(label));
            out.push((format!("  {label}{}{value}", " ".repeat(pad)), Some(i)));
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
            rotations: crate::rotation::Policy::Never,
            pin_time_column: false,
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
        assert_eq!(c.rotations, crate::rotation::Policy::Never);
        assert!(!c.pin_time_column);
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
    fn an_editor_preset_fills_the_command_and_a_typed_one_picks_its_preset() {
        let mut c = FastTailConfig::default();
        let mut form = SettingsForm::from_config(&c);
        focus_on(&mut form, Key::EditorPreset);
        form.on_key(key(KeyCode::Right));
        form.apply(&mut c).unwrap();
        assert_eq!(c.editor_command, crate::source_links::PRESETS[0].1);
        // Typing past the preset's text turns the row back to "as typed".
        focus_on(&mut form, Key::EditorCommand);
        form.on_key(key(KeyCode::Char('x')));
        let preset = form
            .fields
            .iter()
            .find(|f| f.key == Key::EditorPreset)
            .unwrap();
        assert!(matches!(&preset.widget, Widget::Radio(r) if r.selected == 0));
    }

    #[test]
    fn the_level_window_field_takes_16_to_4096_bytes() {
        let mut c = FastTailConfig::default();
        let mut form = SettingsForm::from_config(&c);
        focus_on(&mut form, Key::LevelBytes);
        form.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        form.on_key(key(KeyCode::Char('8')));
        let problems = form.apply(&mut c).unwrap_err();
        assert_eq!(problems[0].1, "16 to 4096");
        assert_eq!(c.level_detect_bytes, 96, "nothing applied");
        form.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for ch in "256".chars() {
            form.on_key(key(KeyCode::Char(ch)));
        }
        form.apply(&mut c).unwrap();
        assert_eq!(c.level_detect_bytes, 256);
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
        // On a number Up / Down still walk the fields; Right / Left step the value.
        focus_on(&mut form, Key::PollInterval);
        let at = form.focus;
        form.on_key(key(KeyCode::Down));
        assert_eq!(form.focus, at + 1);
        form.on_key(key(KeyCode::Up));
        assert_eq!(form.focus, at);
        let value = |form: &SettingsForm| match &form.fields[form.focus].widget {
            Widget::Number(n) => n.value().unwrap(),
            other => panic!("{other:?}"),
        };
        let before = value(&form);
        form.on_key(key(KeyCode::Right));
        assert_eq!(value(&form), before + 1);
        form.on_key(key(KeyCode::Left));
        form.on_key(key(KeyCode::Left));
        assert_eq!(value(&form), before - 1);
        assert_eq!(form.fields[form.focus].key, Key::PollInterval);
        // Digits are still typed at the end.
        form.on_key(key(KeyCode::Backspace));
        form.on_key(key(KeyCode::Char('7')));
        assert_eq!(value(&form), (before - 1) / 10 * 10 + 7);
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

    #[test]
    fn automatic_highlighting_and_its_kinds_are_settings() {
        use crate::auto_highlight::TokenKinds;
        let mut c = FastTailConfig {
            auto_highlight: false,
            auto_highlight_kinds: TokenKinds::ALL,
            ..Default::default()
        };
        let mut form = SettingsForm::from_config(&c);
        focus_on(&mut form, Key::AutoHighlight);
        form.on_key(key(KeyCode::Char(' ')));
        focus_on(&mut form, Key::Token(TokenKind::Path));
        form.on_key(key(KeyCode::Char(' ')));
        form.apply(&mut c).unwrap();
        assert!(c.auto_highlight);
        assert!(!c.auto_highlight_kinds.contains(TokenKind::Path));
        assert!(c.auto_highlight_kinds.contains(TokenKind::Ip));
        assert_eq!(c.auto_tokens(), c.auto_highlight_kinds);
        let lines: Vec<String> = form.lines().into_iter().map(|(l, _)| l).collect();
        assert!(lines
            .iter()
            .any(|l| l.contains("Automatic token highlighting")));
        assert!(lines.iter().any(|l| l.contains("File paths")));
    }

    #[test]
    fn the_pin_is_set_twice_checked_scrambled_and_removed() {
        let mut c = FastTailConfig::default();
        let mut form = SettingsForm::from_config(&c);
        let type_in = |form: &mut SettingsForm, k: Key, text: &str| {
            focus_on(form, k);
            for ch in text.chars() {
                form.on_key(key(KeyCode::Char(ch)));
            }
        };
        // The idle lock needs a PIN.
        focus_on(&mut form, Key::LockEnabled);
        form.on_key(key(KeyCode::Char(' ')));
        assert!(form.apply(&mut c).is_err());
        // Too short, then not matching, then right.
        type_in(&mut form, Key::PinNew, "48");
        assert!(form.apply(&mut c).is_err());
        type_in(&mut form, Key::PinNew, "21");
        type_in(&mut form, Key::PinConfirm, "4822");
        assert!(form.apply(&mut c).is_err());
        focus_on(&mut form, Key::PinConfirm);
        form.on_key(key(KeyCode::Backspace));
        form.on_key(key(KeyCode::Char('1')));
        // The PIN never shows in the form.
        assert!(form.lines().iter().all(|(l, _)| !l.contains("4821")));
        form.apply(&mut c).unwrap();
        assert!(c.lock_enabled);
        assert_ne!(c.lock_pin, "4821");
        assert!(crate::lock::pin_matches(&c.lock_pin, "4821"));
        // A form opened again leaves the PIN as it is; Remove clears it and the switch.
        let untouched = SettingsForm::from_config(&c);
        let mut again = c.clone();
        untouched.apply(&mut again).unwrap();
        assert_eq!(again.lock_pin, c.lock_pin);
        let mut form = SettingsForm::from_config(&c);
        focus_on(&mut form, Key::RemovePin);
        form.on_key(key(KeyCode::Char(' ')));
        form.apply(&mut c).unwrap();
        assert!(c.lock_pin.is_empty() && !c.lock_enabled);
    }
}
