//! Key bindings: terminal key events mapped to actions, kept apart from the app so the
//! table can be tested without a terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a key does in the log view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextTab,
    PrevTab,
    /// Zero-based tab index (ALT + 1..9).
    GotoTab(usize),
    ToggleFollow,
    /// Jump to the last row and follow.
    Bottom,
    Top,
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    ScrollLeft,
    ScrollRight,
    ScrollHome,
    StartSearch,
    SearchNext,
    SearchPrev,
    ClearSearch,
    EditInclude,
    EditExclude,
    CycleCollapse,
    CycleLevel,
    /// One window, two side by side, two stacked.
    CycleSplit,
    ToggleHelp,
    /// `y`: copies the selected rows.
    Copy,
    /// CTRL + C: copies the selection when there is one, else quits.
    CopyOrQuit,
}

/// Maps a key of the log view. Only presses count: the Windows console also reports
/// releases (and repeats as presses), which would otherwise act twice.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if ctrl {
        return match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => Some(Action::CopyOrQuit),
            KeyCode::Char('b') => Some(Action::PageUp),
            KeyCode::Char('f') => Some(Action::PageDown),
            _ => None,
        };
    }
    if alt {
        return match key.code {
            KeyCode::Char(c @ '1'..='9') => Some(Action::GotoTab(c as usize - '1' as usize)),
            _ => None,
        };
    }
    Some(match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Tab => Action::NextTab,
        KeyCode::BackTab => Action::PrevTab,
        KeyCode::Char(' ') => Action::ToggleFollow,
        KeyCode::End | KeyCode::Char('G') => Action::Bottom,
        KeyCode::Home | KeyCode::Char('g') => Action::Top,
        KeyCode::Up | KeyCode::Char('k') => Action::LineUp,
        KeyCode::Down | KeyCode::Char('j') => Action::LineDown,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Left => Action::ScrollLeft,
        KeyCode::Right => Action::ScrollRight,
        KeyCode::Char('0') => Action::ScrollHome,
        KeyCode::Char('/') => Action::StartSearch,
        KeyCode::Char('n') => Action::SearchNext,
        KeyCode::Char('N') => Action::SearchPrev,
        KeyCode::Esc => Action::ClearSearch,
        KeyCode::Char('i') => Action::EditInclude,
        KeyCode::Char('x') => Action::EditExclude,
        KeyCode::Char('c') => Action::CycleCollapse,
        KeyCode::Char('l') => Action::CycleLevel,
        KeyCode::Char('s') => Action::CycleSplit,
        KeyCode::Char('y') => Action::Copy,
        KeyCode::Char('?') | KeyCode::F(1) => Action::ToggleHelp,
        _ => return None,
    })
}

/// What a key does while the prompt line (search, include, exclude) is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKey {
    Insert(char),
    Backspace,
    /// CTRL + U: empties the field.
    Clear,
    Submit,
    Cancel,
}

pub fn map_prompt_key(key: KeyEvent) -> Option<PromptKey> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some(PromptKey::Cancel),
        KeyCode::Char('u') if ctrl => Some(PromptKey::Clear),
        KeyCode::Char(c) if !ctrl => Some(PromptKey::Insert(c)),
        KeyCode::Backspace => Some(PromptKey::Backspace),
        KeyCode::Enter => Some(PromptKey::Submit),
        KeyCode::Esc => Some(PromptKey::Cancel),
        _ => None,
    }
}

/// Applies a prompt key to `text`; returns the key back for `Submit` / `Cancel`, which
/// the caller acts on.
pub fn edit_prompt(text: &mut String, key: PromptKey) -> Option<PromptKey> {
    match key {
        PromptKey::Insert(c) => text.push(c),
        PromptKey::Backspace => {
            text.pop();
        }
        PromptKey::Clear => text.clear(),
        PromptKey::Submit | PromptKey::Cancel => return Some(key),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn quit_keys() {
        assert_eq!(
            map_key(press(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(Action::Quit)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::CopyOrQuit)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('y'), KeyModifiers::NONE)),
            Some(Action::Copy)
        );
        // A plain `c` cycles the collapse mode instead.
        assert_eq!(
            map_key(press(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(Action::CycleCollapse)
        );
    }

    #[test]
    fn tabs_and_alt_digits() {
        assert_eq!(
            map_key(press(KeyCode::Tab, KeyModifiers::NONE)),
            Some(Action::NextTab)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('1'), KeyModifiers::ALT)),
            Some(Action::GotoTab(0))
        );
        assert_eq!(
            map_key(press(KeyCode::Char('9'), KeyModifiers::ALT)),
            Some(Action::GotoTab(8))
        );
        // Without ALT a digit is not a tab switch.
        assert_eq!(map_key(press(KeyCode::Char('3'), KeyModifiers::NONE)), None);
    }

    #[test]
    fn shifted_letters_arrive_as_capitals() {
        // Terminals report SHIFT + g as `G` with the SHIFT modifier set.
        assert_eq!(
            map_key(press(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(Action::Bottom)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('N'), KeyModifiers::SHIFT)),
            Some(Action::SearchPrev)
        );
    }

    #[test]
    fn releases_are_ignored() {
        let mut key = press(KeyCode::Char('q'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(map_key(key), None);
        assert_eq!(map_prompt_key(key), None);
    }

    #[test]
    fn prompt_editing() {
        let mut text = String::from("err");
        for key in [
            PromptKey::Insert('o'),
            PromptKey::Insert('r'),
            PromptKey::Backspace,
        ] {
            assert_eq!(edit_prompt(&mut text, key), None);
        }
        assert_eq!(text, "erro");
        assert_eq!(
            edit_prompt(&mut text, PromptKey::Submit),
            Some(PromptKey::Submit)
        );
        edit_prompt(&mut text, PromptKey::Clear);
        assert!(text.is_empty());
        assert_eq!(
            map_prompt_key(press(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(PromptKey::Insert('q'))
        );
        assert_eq!(
            map_prompt_key(press(KeyCode::Esc, KeyModifiers::NONE)),
            Some(PromptKey::Cancel)
        );
    }
}
