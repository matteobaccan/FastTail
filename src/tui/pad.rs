// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The scratchpad's editor in the terminal: the session's pad (`crate::scratchpad`, the
//! same file the window edits) as lines with a cursor. Typing, `Enter`, `Backspace`,
//! `Delete`, the arrows, `Home` `End`, `PgUp` `PgDn`; `Ctrl+Z` undoes the last change;
//! `Ctrl+G` on (or under) a reference line `── app.log:1204 ──` goes to that line.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PadKey {
    /// The text changed: the caller writes it back to the pad.
    Edited,
    Moved,
    /// `Ctrl+G`: the reference line at or above the cursor, `(name, 0-based line)`.
    Jump(String, usize),
    Close,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PadEditor {
    pub lines: Vec<String>,
    /// Cursor line, and characters before the cursor in it.
    pub row: usize,
    pub col: usize,
    /// First line drawn, and the lines the last frame drew (for the page keys).
    pub top: usize,
    pub page: usize,
    /// The text and cursor before the last change (one level of undo).
    undo: Option<(Vec<String>, usize, usize)>,
}

impl PadEditor {
    /// An editor over `text`, the cursor at its end.
    pub fn new(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        // A pad ends with a line break: the cursor goes on the empty line after it.
        let row = lines.len() - 1;
        let col = lines[row].chars().count();
        Self {
            lines,
            row,
            col,
            top: 0,
            page: 10,
            undo: None,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Byte offset of character `col` in line `row`.
    fn at(&self, row: usize, col: usize) -> usize {
        let line = &self.lines[row];
        line.char_indices().nth(col).map_or(line.len(), |(b, _)| b)
    }

    fn len(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }

    fn remember(&mut self) {
        self.undo = Some((self.lines.clone(), self.row, self.col));
    }

    /// Inserts `text` at the cursor; its line breaks split the line.
    pub fn insert(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let at = self.at(self.row, self.col);
        let tail = self.lines[self.row].split_off(at);
        let mut parts = text.split('\n');
        let first = parts.next().unwrap_or("");
        self.lines[self.row].push_str(first);
        self.col += first.chars().count();
        for part in parts {
            self.row += 1;
            self.lines.insert(self.row, part.to_string());
            self.col = part.chars().count();
        }
        self.lines[self.row].push_str(&tail);
    }

    /// The reference line at or above the cursor.
    fn reference(&self) -> Option<(String, usize)> {
        self.lines[..=self.row]
            .iter()
            .rev()
            .find_map(|l| crate::scratchpad::parse_reference(l))
    }

    pub fn on_key(&mut self, key: KeyEvent) -> PadKey {
        if key.kind == KeyEventKind::Release {
            return PadKey::Other;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let last = self.lines.len() - 1;
        match key.code {
            KeyCode::Esc => return PadKey::Close,
            KeyCode::Char('z') if ctrl => {
                return match self.undo.take() {
                    Some((lines, row, col)) => {
                        (self.lines, self.row, self.col) = (lines, row, col);
                        PadKey::Edited
                    }
                    None => PadKey::Other,
                };
            }
            KeyCode::Char('g') if ctrl => {
                return match self.reference() {
                    Some((name, line)) => PadKey::Jump(name, line),
                    None => PadKey::Other,
                };
            }
            KeyCode::Char(c) if !ctrl => {
                self.remember();
                let c = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                self.insert(c.encode_utf8(&mut [0; 4]));
                return PadKey::Edited;
            }
            KeyCode::Tab => {
                self.remember();
                self.insert("    ");
                return PadKey::Edited;
            }
            KeyCode::Enter => {
                self.remember();
                self.insert("\n");
                return PadKey::Edited;
            }
            KeyCode::Backspace => {
                if self.col > 0 {
                    self.remember();
                    self.col -= 1;
                    let at = self.at(self.row, self.col);
                    self.lines[self.row].remove(at);
                } else if self.row > 0 {
                    self.remember();
                    let line = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.len(self.row);
                    self.lines[self.row].push_str(&line);
                } else {
                    return PadKey::Other;
                }
                return PadKey::Edited;
            }
            KeyCode::Delete => {
                if self.col < self.len(self.row) {
                    self.remember();
                    let at = self.at(self.row, self.col);
                    self.lines[self.row].remove(at);
                } else if self.row < last {
                    self.remember();
                    let next = self.lines.remove(self.row + 1);
                    self.lines[self.row].push_str(&next);
                } else {
                    return PadKey::Other;
                }
                return PadKey::Edited;
            }
            KeyCode::Left if self.col > 0 => self.col -= 1,
            KeyCode::Left if self.row > 0 => {
                self.row -= 1;
                self.col = self.len(self.row);
            }
            KeyCode::Right if self.col < self.len(self.row) => self.col += 1,
            KeyCode::Right if self.row < last => {
                self.row += 1;
                self.col = 0;
            }
            KeyCode::Up => self.row = self.row.saturating_sub(1),
            KeyCode::Down => self.row = (self.row + 1).min(last),
            KeyCode::PageUp => self.row = self.row.saturating_sub(self.page.max(1)),
            KeyCode::PageDown => self.row = (self.row + self.page.max(1)).min(last),
            KeyCode::Home if ctrl => (self.row, self.col) = (0, 0),
            KeyCode::End if ctrl => {
                self.row = last;
                self.col = self.len(last);
            }
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.len(self.row),
            KeyCode::Left | KeyCode::Right => {}
            _ => return PadKey::Other,
        }
        self.col = self.col.min(self.len(self.row));
        PadKey::Moved
    }

    /// Keeps the cursor line inside a window `height` lines tall.
    pub fn scroll(&mut self, height: usize) {
        self.page = height.max(1);
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + self.page {
            self.top = self.row + 1 - self.page;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_splitting_joining_and_undo() {
        let mut e = PadEditor::new("── a.log:3 ──\nboom\n");
        assert_eq!((e.row, e.col), (2, 0));
        for c in "note".chars() {
            e.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(e.text(), "── a.log:3 ──\nboom\nnote");
        e.on_key(key(KeyCode::Home));
        e.on_key(key(KeyCode::Backspace));
        assert_eq!(e.text(), "── a.log:3 ──\nboomnote");
        e.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert_eq!(e.text(), "── a.log:3 ──\nboom\nnote");
        e.insert("x\ny");
        assert_eq!(e.text(), "── a.log:3 ──\nboom\nx\nynote");
        assert_eq!(
            e.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            PadKey::Jump("a.log".into(), 2)
        );
    }
}
