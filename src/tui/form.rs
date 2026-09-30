// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The form toolkit of the terminal dialogs. A text field edits one line: `←` `→`,
//! `Home` `End`, `Backspace`, `Delete`, `Ctrl+U` to clear and paste, and scrolls sideways
//! to keep its cursor in view. Widths are counted in terminal cells.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use unicode_width::UnicodeWidthChar;

/// What a key did to a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    /// The field used the key (the text or the cursor may have changed).
    Edited,
    /// `Enter`: the dialog confirms.
    Submit,
    /// `Esc` (or `Ctrl+C`): the dialog cancels.
    Cancel,
    /// Not a field key: the dialog may use it (`Tab`, `↑`, `↓`).
    Other,
}

/// One line of editable text with a cursor, counted in characters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    /// Characters before the cursor.
    cursor: usize,
}

impl TextField {
    /// A field holding `text`, the cursor at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.chars().count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Byte offset of the cursor.
    fn at(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(b, _)| b)
    }

    /// Inserts `s` at the cursor; line breaks of a pasted text become spaces.
    pub fn insert(&mut self, s: &str) {
        let at = self.at();
        let clean: String = s
            .chars()
            .map(|c| {
                if c == '\r' || c == '\n' || c == '\t' {
                    ' '
                } else {
                    c
                }
            })
            .filter(|c| !c.is_control())
            .collect();
        self.text.insert_str(at, &clean);
        self.cursor += clean.chars().count();
    }

    /// Applies a key. Only presses count (the Windows console also reports releases).
    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        if key.kind == KeyEventKind::Release {
            return FieldKey::Other;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let len = self.text.chars().count();
        match key.code {
            KeyCode::Enter => return FieldKey::Submit,
            KeyCode::Esc => return FieldKey::Cancel,
            KeyCode::Char('c') if ctrl => return FieldKey::Cancel,
            KeyCode::Char('u') if ctrl => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c) if !ctrl => self.insert(c.encode_utf8(&mut [0; 4])),
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.at();
                self.text.remove(at);
            }
            KeyCode::Delete if self.cursor < len => {
                let at = self.at();
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(len),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = len,
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return FieldKey::Other,
        }
        FieldKey::Edited
    }

    /// The part of the text a box `width` cells wide shows, and the cursor's cell in it.
    /// The text scrolls so the cursor stays inside, one cell kept for it at the end.
    pub fn view(&self, width: usize) -> (String, usize) {
        let width = width.max(1);
        let cells: Vec<(char, usize)> = self
            .text
            .chars()
            .map(|c| (c, c.width().unwrap_or(0)))
            .collect();
        let before: usize = cells[..self.cursor].iter().map(|&(_, w)| w).sum();
        // Drop characters from the left until the cursor fits.
        let mut skip = 0;
        let mut skipped_cells = 0;
        while before - skipped_cells >= width && skip < self.cursor {
            skipped_cells += cells[skip].1;
            skip += 1;
        }
        let mut shown = String::new();
        let mut used = 0;
        for &(c, w) in &cells[skip..] {
            if used + w > width {
                break;
            }
            shown.push(c);
            used += w;
        }
        (shown, before - skipped_cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(f: &mut TextField, s: &str) {
        for c in s.chars() {
            f.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn editing_keys_move_insert_and_delete_at_the_cursor() {
        let mut f = TextField::new("14:02");
        assert_eq!(f.cursor, 5);
        f.on_key(key(KeyCode::Home));
        typed(&mut f, "2026-09-18 ");
        assert_eq!(f.text(), "2026-09-18 14:02");
        f.on_key(key(KeyCode::End));
        f.on_key(key(KeyCode::Left));
        f.on_key(key(KeyCode::Backspace));
        assert_eq!(f.text(), "2026-09-18 14:2");
        f.on_key(key(KeyCode::Delete));
        assert_eq!(f.text(), "2026-09-18 14:");
        f.on_key(key(KeyCode::Delete));
        assert_eq!(f.text(), "2026-09-18 14:", "Delete at the end does nothing");
        f.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!((f.text(), f.cursor), ("", 0));
        assert_eq!(f.on_key(key(KeyCode::Enter)), FieldKey::Submit);
        assert_eq!(f.on_key(key(KeyCode::Esc)), FieldKey::Cancel);
        assert_eq!(f.on_key(key(KeyCode::Tab)), FieldKey::Other);
    }

    #[test]
    fn non_ascii_text_is_edited_by_character() {
        let mut f = TextField::new("città");
        f.on_key(key(KeyCode::Left));
        f.on_key(key(KeyCode::Backspace));
        assert_eq!(f.text(), "cità");
        f.insert("日本");
        assert_eq!((f.text(), f.cursor), ("cit日本à", 5));
    }

    #[test]
    fn paste_turns_line_breaks_into_spaces() {
        let mut f = TextField::default();
        f.insert("error\r\nwarn\tx\u{7}");
        assert_eq!(f.text(), "error  warn x");
    }

    #[test]
    fn the_view_scrolls_to_keep_the_cursor_inside() {
        let f = TextField::new("abcdefghij");
        assert_eq!(f.view(20), ("abcdefghij".to_string(), 10));
        // Five cells: the cursor at the end takes the last one.
        assert_eq!(f.view(5), ("ghij".to_string(), 4));
        let mut f = TextField::new("abcdefghij");
        f.on_key(key(KeyCode::Home));
        assert_eq!(f.view(5), ("abcde".to_string(), 0));
        // Wide characters count two cells.
        let f = TextField::new("日本語");
        assert_eq!(f.view(10), ("日本語".to_string(), 6));
        assert_eq!(f.view(4), ("語".to_string(), 2));
    }
}
