// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

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
    /// SHIFT + UP / DOWN: extends the selection from the cursor row.
    SelectUp,
    SelectDown,
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
    /// `b` / CTRL + F2: bookmark on the cursor row, on or off.
    ToggleBookmark,
    /// `]` / F2 and `[` / SHIFT + F2: next / previous visible bookmark, wrapping around.
    NextBookmark,
    PrevBookmark,
    /// `m`: the note of the cursor row's bookmark.
    EditNote,
    /// CTRL + K: the cursor row in context (filters suspended), and back.
    ToggleContext,
    /// CTRL + G, `:`: go to a line (`N`, `+N`, `-N`) or a time (`14:02`).
    GoTo,
    /// `h`: the HEX view of the bytes, and back to the lines.
    ToggleHex,
    /// `a`: how escape sequences show: auto, render, strip, raw.
    CycleAnsi,
    /// `t`: the time range dialog.
    TimeRange,
    /// `o`: open a file by path.
    OpenFile,
    /// `O`: load a session; `S`: save the open streams as a session.
    OpenSession,
    SaveSession,
    /// `T`: the next theme.
    CycleTheme,
    /// `,`: the Settings dialog.
    Settings,
}

/// Maps a key of the log view. Only presses count: the Windows console also reports
/// releases (and repeats as presses), which would otherwise act twice.
pub fn map_key(mut key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    // The Windows console may report SHIFT + t as a lowercase `t` with the SHIFT
    // modifier, where other terminals send `T`: both mean the capital.
    if let KeyCode::Char(c) = key.code {
        if key.modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_lowercase() {
            key.code = KeyCode::Char(c.to_ascii_uppercase());
        }
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if ctrl {
        return match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => Some(Action::CopyOrQuit),
            KeyCode::Char('b') => Some(Action::PageUp),
            KeyCode::Char('f') => Some(Action::PageDown),
            KeyCode::F(2) => Some(Action::ToggleBookmark),
            KeyCode::Char('k') => Some(Action::ToggleContext),
            KeyCode::Char('g') => Some(Action::GoTo),
            _ => None,
        };
    }
    if alt {
        return match key.code {
            KeyCode::Char(c @ '1'..='9') => Some(Action::GotoTab(c as usize - '1' as usize)),
            _ => None,
        };
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Up => return Some(Action::SelectUp),
            KeyCode::Down => return Some(Action::SelectDown),
            KeyCode::F(2) => return Some(Action::PrevBookmark),
            KeyCode::F(3) => return Some(Action::SearchPrev),
            _ => {}
        }
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
        KeyCode::Char('n') | KeyCode::F(3) => Action::SearchNext,
        KeyCode::Char('N') => Action::SearchPrev,
        KeyCode::Esc => Action::ClearSearch,
        KeyCode::Char('i') => Action::EditInclude,
        KeyCode::Char('x') => Action::EditExclude,
        KeyCode::Char('c') => Action::CycleCollapse,
        KeyCode::Char('l') => Action::CycleLevel,
        KeyCode::Char('s') => Action::CycleSplit,
        KeyCode::Char('y') => Action::Copy,
        KeyCode::Char('b') => Action::ToggleBookmark,
        KeyCode::Char(']') | KeyCode::F(2) => Action::NextBookmark,
        KeyCode::Char('[') => Action::PrevBookmark,
        KeyCode::Char('m') => Action::EditNote,
        KeyCode::Char(':') => Action::GoTo,
        KeyCode::Char('h') => Action::ToggleHex,
        KeyCode::Char('a') => Action::CycleAnsi,
        KeyCode::Char('t') => Action::TimeRange,
        KeyCode::Char('o') => Action::OpenFile,
        KeyCode::Char('O') => Action::OpenSession,
        KeyCode::Char('S') => Action::SaveSession,
        KeyCode::Char('T') => Action::CycleTheme,
        KeyCode::Char(',') => Action::Settings,
        KeyCode::Char('?') | KeyCode::F(1) => Action::ToggleHelp,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn bookmark_keys() {
        let k = |code, m| map_key(press(code, m));
        let none = KeyModifiers::NONE;
        assert_eq!(k(KeyCode::Char('b'), none), Some(Action::ToggleBookmark));
        assert_eq!(
            k(KeyCode::F(2), KeyModifiers::CONTROL),
            Some(Action::ToggleBookmark)
        );
        assert_eq!(k(KeyCode::Char(']'), none), Some(Action::NextBookmark));
        assert_eq!(k(KeyCode::F(2), none), Some(Action::NextBookmark));
        assert_eq!(k(KeyCode::Char('['), none), Some(Action::PrevBookmark));
        assert_eq!(
            k(KeyCode::F(2), KeyModifiers::SHIFT),
            Some(Action::PrevBookmark)
        );
        assert_eq!(k(KeyCode::Char('m'), none), Some(Action::EditNote));
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(k(KeyCode::Char('k'), ctrl), Some(Action::ToggleContext));
        assert_eq!(k(KeyCode::Char('g'), ctrl), Some(Action::GoTo));
        assert_eq!(k(KeyCode::Char(':'), none), Some(Action::GoTo));
        assert_eq!(k(KeyCode::Char('h'), none), Some(Action::ToggleHex));
        assert_eq!(k(KeyCode::Char('a'), none), Some(Action::CycleAnsi));
        assert_eq!(k(KeyCode::Char('t'), none), Some(Action::TimeRange));
        assert_eq!(k(KeyCode::Char('o'), none), Some(Action::OpenFile));
        assert_eq!(k(KeyCode::F(3), none), Some(Action::SearchNext));
        assert_eq!(
            k(KeyCode::F(3), KeyModifiers::SHIFT),
            Some(Action::SearchPrev)
        );
        let shift = KeyModifiers::SHIFT;
        assert_eq!(k(KeyCode::Char('O'), shift), Some(Action::OpenSession));
        assert_eq!(k(KeyCode::Char('S'), shift), Some(Action::SaveSession));
        assert_eq!(k(KeyCode::Char('T'), shift), Some(Action::CycleTheme));
        assert_eq!(k(KeyCode::Char(','), none), Some(Action::Settings));
        assert_eq!(
            k(KeyCode::Char('g'), none),
            Some(Action::Top),
            "plain g is still top"
        );
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
    fn shift_with_a_lowercase_letter_is_the_capital() {
        let shift = KeyModifiers::SHIFT;
        let k = |c| map_key(press(KeyCode::Char(c), shift));
        assert_eq!(k('t'), Some(Action::CycleTheme), "not the time range");
        assert_eq!(k('g'), Some(Action::Bottom));
        assert_eq!(k('n'), Some(Action::SearchPrev));
        assert_eq!(k('o'), Some(Action::OpenSession));
        assert_eq!(k('s'), Some(Action::SaveSession));
        assert_eq!(
            map_key(press(KeyCode::Char('t'), KeyModifiers::NONE)),
            Some(Action::TimeRange)
        );
    }

    #[test]
    fn releases_are_ignored() {
        let mut key = press(KeyCode::Char('q'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(map_key(key), None);
    }
}
