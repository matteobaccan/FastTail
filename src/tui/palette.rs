// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The command palette of the terminal (`:`): the actions of the registry shared with
//! the GUI (`crate::actions`) that the terminal can run, with their names in the
//! interface language and the terminal's keys, filtered as the user types. A number (or
//! `+N`, `-N`, a time) followed by `Enter` goes to that line instead, as `:` did.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::actions::{ActionId, ACTIONS};
use crate::i18n::{t, Language};
use crate::tui::form::{FieldKey, TextField};
use crate::tui::keys::Action;

/// A registry action the terminal runs: its id, name, the terminal key and what runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: ActionId,
    pub name: String,
    pub key: &'static str,
    pub action: Action,
}

/// The terminal action and key of a registry action, `None` for the GUI-only ones.
fn terminal_action(id: ActionId) -> Option<(Action, &'static str)> {
    Some(match id {
        ActionId::Follow => (Action::ToggleFollow, "Space"),
        ActionId::GoToLine => (Action::GoTo, "Ctrl+G"),
        ActionId::Copy => (Action::Copy, "y"),
        ActionId::ViewText | ActionId::ViewHex => (Action::ToggleHex, "h"),
        ActionId::ViewAsm => (Action::ToggleAsm, "d"),
        ActionId::AsmArch => (Action::CycleArch, "D"),
        ActionId::Collapse => (Action::CycleCollapse, "c"),
        ActionId::SearchFocus => (Action::StartSearch, "/"),
        ActionId::SearchNext => (Action::SearchNext, "n"),
        ActionId::SearchPrev => (Action::SearchPrev, "N"),
        ActionId::SearchClear => (Action::ClearSearch, "Esc"),
        ActionId::GlobalFilterBar => (Action::EditGlobal, "F"),
        ActionId::ShowContext | ActionId::LeaveContext => (Action::ToggleContext, "Ctrl+K"),
        ActionId::BookmarkToggle => (Action::ToggleBookmark, "b"),
        ActionId::BookmarkNext => (Action::NextBookmark, "]"),
        ActionId::BookmarkPrev => (Action::PrevBookmark, "["),
        ActionId::BookmarkNote => (Action::EditNote, "m"),
        ActionId::PresetSave | ActionId::PresetManage => (Action::Presets, "p"),
        ActionId::OpenFile => (Action::OpenFile, "o"),
        ActionId::Settings => (Action::Settings, ","),
        ActionId::ColorFilters => (Action::EditRules, "r"),
        ActionId::Help => (Action::ToggleHelp, "?"),
        ActionId::About => (Action::About, ""),
        ActionId::LockNow => (Action::Lock, "Ctrl+L"),
        ActionId::SessionSaveAs => (Action::SaveSession, "S"),
        ActionId::SessionLoad => (Action::OpenSession, "O"),
        _ => return None,
    })
}

/// The palette's entries in the registry's order, named in `lang`.
pub fn entries(lang: Language) -> Vec<Entry> {
    ACTIONS
        .iter()
        .filter_map(|a| {
            let (action, key) = terminal_action(a.id)?;
            Some(Entry {
                id: a.id,
                name: t(lang, a.name).to_string(),
                key,
                action,
            })
        })
        .collect()
}

/// Whether `query` is a go-to target rather than a command: it starts with a digit, or
/// with `+` / `-` and a digit.
pub fn is_goto(query: &str) -> bool {
    let q = query.trim();
    let mut chars = q.chars();
    match chars.next() {
        Some(c) if c.is_ascii_digit() => true,
        Some('+' | '-') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// The entries whose name or key holds every word of `query`, without regard to case.
pub fn matching(entries: &[Entry], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let name = e.name.to_lowercase();
            let key = e.key.to_lowercase();
            words.iter().all(|w| name.contains(w) || key == *w)
        })
        .map(|(i, _)| i)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPalette {
    pub entries: Vec<Entry>,
    pub field: TextField,
    /// The selected row among the matching entries.
    pub selected: usize,
}

/// What a key asks of the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteKey {
    /// Run this registry action.
    Run(ActionId, Action),
    /// Go to this line (or time) of the focused stream.
    GoTo(String),
    Moved,
    Close,
    Other,
}

impl CommandPalette {
    pub fn new(lang: Language) -> Self {
        Self {
            entries: entries(lang),
            field: TextField::default(),
            selected: 0,
        }
    }

    /// The matching entries, in order.
    pub fn shown(&self) -> Vec<usize> {
        if is_goto(self.field.text()) {
            return Vec::new();
        }
        matching(&self.entries, self.field.text())
    }

    pub fn on_key(&mut self, key: KeyEvent) -> PaletteKey {
        if key.kind == KeyEventKind::Release {
            return PaletteKey::Other;
        }
        let shown = self.shown();
        match key.code {
            KeyCode::Esc => PaletteKey::Close,
            KeyCode::Enter if is_goto(self.field.text()) => {
                PaletteKey::GoTo(self.field.text().trim().to_string())
            }
            KeyCode::Enter => match shown.get(self.selected).map(|&i| &self.entries[i]) {
                Some(e) => PaletteKey::Run(e.id, e.action),
                None => PaletteKey::Other,
            },
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                PaletteKey::Moved
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(shown.len().saturating_sub(1));
                PaletteKey::Moved
            }
            _ => match self.field.on_key(key) {
                FieldKey::Edited => {
                    self.selected = 0;
                    PaletteKey::Moved
                }
                _ => PaletteKey::Other,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(p: &mut CommandPalette, s: &str) {
        for c in s.chars() {
            p.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn the_palette_lists_the_registry_with_terminal_keys_and_goes_to_lines() {
        let mut p = CommandPalette::new(Language::En);
        assert!(p
            .entries
            .iter()
            .any(|e| e.id == ActionId::LockNow && e.key == "Ctrl+L"));
        // GUI-only actions are not listed.
        assert!(!p.entries.iter().any(|e| e.id == ActionId::ZoomIn));
        // Words filter by name; Enter runs the selected one.
        typed(&mut p, "bookmark next");
        let shown = p.shown();
        assert_eq!(
            shown.len(),
            1,
            "{:?}",
            shown
                .iter()
                .map(|&i| &p.entries[i].name)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            p.on_key(key(KeyCode::Enter)),
            PaletteKey::Run(ActionId::BookmarkNext, Action::NextBookmark)
        );
        // A number is a line to go to.
        let mut p = CommandPalette::new(Language::En);
        typed(&mut p, "1200");
        assert!(p.shown().is_empty());
        assert_eq!(
            p.on_key(key(KeyCode::Enter)),
            PaletteKey::GoTo("1200".into())
        );
        assert!(is_goto("+50") && is_goto("-3") && is_goto("14:02"));
        assert!(!is_goto("go") && !is_goto("-x"));
        assert_eq!(p.on_key(key(KeyCode::Esc)), PaletteKey::Close);
    }

    #[test]
    fn names_follow_the_interface_language() {
        let en = entries(Language::En);
        let it = entries(Language::It);
        assert_eq!(en.len(), it.len());
        assert!(en.iter().zip(&it).any(|(a, b)| a.name != b.name));
    }
}
