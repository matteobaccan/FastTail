// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The form toolkit of the terminal dialogs. A text field edits one line: `←` `→`,
//! `Home` `End`, `Backspace`, `Delete`, `Ctrl+U` to clear and paste, and scrolls sideways
//! to keep its cursor in view. Widths are counted in terminal cells. On top of it: a
//! number field bound to a range, a check box, a radio list, a reorderable list and a
//! colour field (`#RRGGBB` or a swatch). Each takes a key and says what it did; the
//! dialogs draw them.

use std::ops::RangeInclusive;

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
            KeyCode::Char(c) if !ctrl => {
                // SHIFT + a lowercase letter (the Windows console) is the capital.
                let c = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                self.insert(c.encode_utf8(&mut [0; 4]))
            }
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

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// A press of the key, not a release (the Windows console reports both).
fn pressed(key: &KeyEvent) -> bool {
    key.kind != KeyEventKind::Release
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// A whole number typed in a text field and checked against a range; `↑` / `↓` step it
/// by one inside the range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberField {
    pub field: TextField,
    pub range: RangeInclusive<u64>,
}

#[allow(dead_code)]
impl NumberField {
    pub fn new(value: u64, range: RangeInclusive<u64>) -> Self {
        Self {
            field: TextField::new(&value.to_string()),
            range,
        }
    }

    /// The value, or the message the dialog shows next to the field: the range, as
    /// the GUI Settings shows it.
    pub fn value(&self) -> Result<u64, String> {
        let (lo, hi) = (*self.range.start(), *self.range.end());
        match self.field.text().trim().parse::<u64>() {
            Ok(v) if self.range.contains(&v) => Ok(v),
            _ => Err(format!("{lo} to {hi}")),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        if !pressed(&key) {
            return FieldKey::Other;
        }
        let step = |v: u64, up: bool| {
            let (lo, hi) = (*self.range.start(), *self.range.end());
            if up {
                v.saturating_add(1).clamp(lo, hi)
            } else {
                v.saturating_sub(1).clamp(lo, hi)
            }
        };
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                let current = self
                    .field
                    .text()
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(*self.range.start());
                let v = step(current, key.code == KeyCode::Up);
                self.field = TextField::new(&v.to_string());
                FieldKey::Edited
            }
            // Only digits are typed; the editing keys work as in any field.
            KeyCode::Char(c)
                if !c.is_ascii_digit() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                FieldKey::Other
            }
            _ => self.field.on_key(key),
        }
    }
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// An on / off switch: `Space` toggles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CheckBox {
    pub on: bool,
}

#[allow(dead_code)]
impl CheckBox {
    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        if pressed(&key) && key.code == KeyCode::Char(' ') {
            self.on = !self.on;
            FieldKey::Edited
        } else {
            FieldKey::Other
        }
    }

    /// `[x] label` or `[ ] label`.
    pub fn text(&self, label: &str) -> String {
        format!("[{}] {label}", if self.on { 'x' } else { ' ' })
    }
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// One choice among a few, shown on a line: `←` `→` (or `↑` `↓`) move it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioList {
    pub options: Vec<String>,
    pub selected: usize,
}

#[allow(dead_code)]
impl RadioList {
    pub fn new(options: &[&str], selected: usize) -> Self {
        Self {
            options: options.iter().map(|o| o.to_string()).collect(),
            selected: selected.min(options.len().saturating_sub(1)),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        if !pressed(&key) || self.options.is_empty() {
            return FieldKey::Other;
        }
        let last = self.options.len() - 1;
        self.selected = match key.code {
            KeyCode::Left | KeyCode::Up => self.selected.saturating_sub(1),
            KeyCode::Right | KeyCode::Down => (self.selected + 1).min(last),
            KeyCode::Home => 0,
            KeyCode::End => last,
            _ => return FieldKey::Other,
        };
        FieldKey::Edited
    }

    /// `(o) first  ( ) second`.
    pub fn text(&self) -> String {
        self.options
            .iter()
            .enumerate()
            .map(|(i, o)| format!("({}) {o}", if i == self.selected { 'o' } else { ' ' }))
            .collect::<Vec<_>>()
            .join("  ")
    }
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// An ordered list with a selected row: `↑` `↓` select, `Alt+↑` / `Alt+↓` (or `K` / `J`)
/// move the selected item, `Delete` removes it. The order is the priority, as in the
/// rule list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorderList<T> {
    pub items: Vec<T>,
    pub selected: usize,
}

#[allow(dead_code)]
impl<T> ReorderList<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items, selected: 0 }
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.items.get(self.selected)
    }

    /// Adds `item` after the selected one and selects it.
    pub fn insert(&mut self, item: T) {
        let at = if self.items.is_empty() {
            0
        } else {
            self.selected + 1
        };
        self.items.insert(at, item);
        self.selected = at;
    }

    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        if !pressed(&key) || self.items.is_empty() {
            return FieldKey::Other;
        }
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let last = self.items.len() - 1;
        let i = self.selected;
        match key.code {
            KeyCode::Up if alt => self.swap_to(i.checked_sub(1)),
            KeyCode::Down if alt => self.swap_to((i < last).then_some(i + 1)),
            // `K` / `J`, also as SHIFT + a lowercase letter (the Windows console).
            KeyCode::Char('K') => self.swap_to(i.checked_sub(1)),
            KeyCode::Char('J') => self.swap_to((i < last).then_some(i + 1)),
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.swap_to(i.checked_sub(1))
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.swap_to((i < last).then_some(i + 1))
            }
            KeyCode::Up => self.selected = i.saturating_sub(1),
            KeyCode::Down => self.selected = (i + 1).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            KeyCode::Delete => {
                self.items.remove(i);
                self.selected = i.min(self.items.len().saturating_sub(1));
            }
            _ => return FieldKey::Other,
        }
        FieldKey::Edited
    }

    fn swap_to(&mut self, to: Option<usize>) {
        if let Some(to) = to {
            self.items.swap(self.selected, to);
            self.selected = to;
        }
    }
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// A colour typed as `#RRGGBB` (or `RRGGBB`), or picked from `swatches` with `[` / `]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourField {
    pub field: TextField,
    pub swatches: Vec<[u8; 3]>,
    /// The swatch picked last, for `[` / `]` to move from.
    swatch: Option<usize>,
}

#[allow(dead_code)]
impl ColourField {
    pub fn new(rgb: [u8; 3], swatches: Vec<[u8; 3]>) -> Self {
        Self {
            field: TextField::new(&hex(rgb)),
            swatch: swatches.iter().position(|&s| s == rgb),
            swatches,
        }
    }

    /// The colour, or `None` while the text is not `#RRGGBB`.
    pub fn value(&self) -> Option<[u8; 3]> {
        parse_hex(self.field.text())
    }

    pub fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        if !pressed(&key) {
            return FieldKey::Other;
        }
        let n = self.swatches.len();
        let pick = match key.code {
            KeyCode::Char(']') if n > 0 => Some(self.swatch.map_or(0, |i| (i + 1) % n)),
            KeyCode::Char('[') if n > 0 => Some(self.swatch.map_or(n - 1, |i| (i + n - 1) % n)),
            _ => None,
        };
        if let Some(i) = pick {
            self.swatch = Some(i);
            self.field = TextField::new(&hex(self.swatches[i]));
            return FieldKey::Edited;
        }
        let done = self.field.on_key(key);
        if done == FieldKey::Edited {
            self.swatch = None;
        }
        done
    }
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// `#rrggbb` of a colour, upper case.
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

// Used by the Settings and editor dialogs (tasks 4.1 to 4.4).
#[allow(dead_code)]
/// `#RRGGBB` or `RRGGBB`, any case.
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let t = text.trim();
    let t = t.strip_prefix('#').unwrap_or(t);
    if t.len() != 6 || !t.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
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
    fn shift_with_a_lowercase_letter_types_the_capital() {
        let mut f = TextField::default();
        f.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::SHIFT));
        f.on_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT));
        f.on_key(key(KeyCode::Char('r')));
        assert_eq!(f.text(), "ERr");
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

    fn mods(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    #[test]
    fn a_number_field_takes_digits_steps_and_says_its_range() {
        let mut n = NumberField::new(250, 50..=5000);
        assert_eq!(n.value(), Ok(250));
        assert_eq!(n.on_key(key(KeyCode::Char('x'))), FieldKey::Other);
        n.on_key(key(KeyCode::Up));
        assert_eq!(n.value(), Ok(251));
        n.on_key(mods(KeyCode::Char('u'), KeyModifiers::CONTROL));
        typed_number(&mut n, "9");
        assert_eq!(n.value(), Err("50 to 5000".to_string()));
        n.on_key(key(KeyCode::Down));
        assert_eq!(n.value(), Ok(50), "a step brings it into the range");
        let mut top = NumberField::new(5000, 50..=5000);
        top.on_key(key(KeyCode::Up));
        assert_eq!(top.value(), Ok(5000));
    }

    fn typed_number(n: &mut NumberField, s: &str) {
        for c in s.chars() {
            n.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn check_box_and_radio_list() {
        let mut c = CheckBox::default();
        assert_eq!(c.text("Sound"), "[ ] Sound");
        assert_eq!(c.on_key(key(KeyCode::Char(' '))), FieldKey::Edited);
        assert_eq!(c.text("Sound"), "[x] Sound");
        assert_eq!(c.on_key(key(KeyCode::Enter)), FieldKey::Other);

        let mut r = RadioList::new(&["gui", "tui"], 0);
        assert_eq!(r.text(), "(o) gui  ( ) tui");
        r.on_key(key(KeyCode::Right));
        r.on_key(key(KeyCode::Right));
        assert_eq!(r.selected, 1, "stops at the last");
        r.on_key(key(KeyCode::Home));
        assert_eq!(r.selected, 0);
    }

    #[test]
    fn a_reorder_list_moves_items_and_keeps_the_selection_on_them() {
        let mut l = ReorderList::new(vec!["a", "b", "c"]);
        l.on_key(key(KeyCode::Down));
        l.on_key(mods(KeyCode::Down, KeyModifiers::ALT));
        assert_eq!((l.items.as_slice(), l.selected), (&["a", "c", "b"][..], 2));
        l.on_key(key(KeyCode::Char('K')));
        l.on_key(key(KeyCode::Char('K')));
        l.on_key(key(KeyCode::Char('K')));
        assert_eq!((l.items.as_slice(), l.selected), (&["b", "a", "c"][..], 0));
        l.on_key(key(KeyCode::Delete));
        assert_eq!((l.items.as_slice(), l.selected), (&["a", "c"][..], 0));
        l.insert("z");
        assert_eq!((l.items.as_slice(), l.selected), (&["a", "z", "c"][..], 1));
        assert_eq!(l.selected_item(), Some(&"z"));
    }

    #[test]
    fn a_colour_is_typed_as_hex_or_picked_from_the_swatches() {
        assert_eq!(parse_hex("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("FF8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("#ff80"), None);
        assert_eq!(parse_hex("#gg8000"), None);
        let swatches = vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]];
        let mut c = ColourField::new([0, 255, 0], swatches);
        assert_eq!(c.field.text(), "#00FF00");
        c.on_key(key(KeyCode::Char(']')));
        assert_eq!(c.value(), Some([0, 0, 255]));
        c.on_key(key(KeyCode::Char(']')));
        assert_eq!(c.value(), Some([255, 0, 0]), "wraps");
        c.on_key(key(KeyCode::Backspace));
        assert_eq!(c.value(), None, "#FF000 is not a colour");
        c.on_key(key(KeyCode::Char('1')));
        assert_eq!(c.value(), Some([255, 0, 1]));
    }
}
