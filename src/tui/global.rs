// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The global filter editor of the terminal (`F`): the on switch, up to 8 include and
//! 8 exclude terms and the case and regex toggles of `[global_filter]`, combined by
//! every stream with its own filter (see `crate::global_filter`). The dialog edits its
//! own copy; the caller applies it `APPLY_DELAY_MS` after the last key, as the GUI's
//! bar does, and at once when the dialog closes.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::global_filter::GlobalFilter;
use crate::scan_job::{FilterSpec, MAX_FILTER_TERMS};
use crate::tui::form::{CheckBox, FieldKey, TextField};

/// A row of the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Enabled,
    MatchCase,
    Regex,
    Include(usize),
    Exclude(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDialog {
    pub enabled: CheckBox,
    pub case_sensitive: CheckBox,
    pub is_regex: CheckBox,
    pub include: Vec<TextField>,
    pub exclude: Vec<TextField>,
    /// The row with the keyboard, an index into `rows()`.
    pub focus: usize,
    /// Edited since the filter was last applied.
    pub dirty: bool,
    /// Regex terms that do not compile, by row (recomputed on each edit).
    pub invalid: Vec<Row>,
}

/// What a key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalKey {
    /// The filter changed: apply it after the delay.
    Edited,
    /// Only the focus moved.
    Moved,
    /// The dialog closes (the caller applies a pending edit first).
    Close,
    Other,
}

fn fields(terms: &[String]) -> Vec<TextField> {
    terms
        .iter()
        .filter(|t| !t.is_empty())
        .take(MAX_FILTER_TERMS)
        .map(|t| TextField::new(t))
        .collect()
}

impl GlobalDialog {
    pub fn new(gf: &GlobalFilter) -> Self {
        let mut d = Self {
            enabled: CheckBox { on: gf.enabled },
            case_sensitive: CheckBox {
                on: gf.case_sensitive,
            },
            is_regex: CheckBox { on: gf.is_regex },
            include: fields(&gf.include),
            exclude: fields(&gf.exclude),
            focus: 0,
            dirty: false,
            invalid: Vec::new(),
        };
        // With no term yet, the first include row has the keyboard.
        if !gf.has_terms() {
            d.focus = 3;
        }
        d.check();
        d
    }

    /// The rows shown: the toggles, the filled include rows plus an empty one (up to 8),
    /// then the same for the exclude rows.
    pub fn rows(&self) -> Vec<Row> {
        let mut out = vec![Row::Enabled, Row::MatchCase, Row::Regex];
        let shown = |terms: &[TextField]| (terms.len() + 1).min(MAX_FILTER_TERMS);
        out.extend((0..shown(&self.include)).map(Row::Include));
        out.extend((0..shown(&self.exclude)).map(Row::Exclude));
        out
    }

    /// The text of term row `row` (empty for the spare row).
    pub fn term(&self, row: Row) -> &str {
        match row {
            Row::Include(i) => self.include.get(i).map_or("", |f| f.text()),
            Row::Exclude(i) => self.exclude.get(i).map_or("", |f| f.text()),
            _ => "",
        }
    }

    /// The filter as edited, keeping `base`'s bar state.
    pub fn filter(&self, base: &GlobalFilter) -> GlobalFilter {
        let terms = |list: &[TextField]| -> Vec<String> {
            list.iter()
                .map(|f| f.text().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        };
        GlobalFilter {
            enabled: self.enabled.on,
            case_sensitive: self.case_sensitive.on,
            is_regex: self.is_regex.on,
            include: terms(&self.include),
            exclude: terms(&self.exclude),
            ..base.clone()
        }
    }

    /// Marks the regex terms that do not compile.
    fn check(&mut self) {
        self.invalid.clear();
        if !self.is_regex.on {
            return;
        }
        let case = self.case_sensitive.on;
        let bad = |t: &str| {
            !t.is_empty()
                && FilterSpec::build(&[t.to_string()], &[], case, true)
                    .include
                    .first()
                    .is_some_and(|x| x.invalid)
        };
        for (i, f) in self.include.iter().enumerate() {
            if bad(f.text()) {
                self.invalid.push(Row::Include(i));
            }
        }
        for (i, f) in self.exclude.iter().enumerate() {
            if bad(f.text()) {
                self.invalid.push(Row::Exclude(i));
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> GlobalKey {
        if key.kind == KeyEventKind::Release {
            return GlobalKey::Other;
        }
        let rows = self.rows();
        self.focus = self.focus.min(rows.len() - 1);
        match key.code {
            KeyCode::Esc | KeyCode::Enter => return GlobalKey::Close,
            KeyCode::Tab | KeyCode::Down => {
                self.focus = (self.focus + 1) % rows.len();
                return GlobalKey::Moved;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + rows.len() - 1) % rows.len();
                return GlobalKey::Moved;
            }
            _ => {}
        }
        let row = rows[self.focus];
        let done = match row {
            Row::Enabled => self.enabled.on_key(key),
            Row::MatchCase => self.case_sensitive.on_key(key),
            Row::Regex => self.is_regex.on_key(key),
            Row::Include(i) => edit_term(&mut self.include, i, key),
            Row::Exclude(i) => edit_term(&mut self.exclude, i, key),
        };
        if done != FieldKey::Edited {
            return GlobalKey::Other;
        }
        // A term row emptied goes, so the spare row stays last.
        for list in [&mut self.include, &mut self.exclude] {
            list.retain(|f| !f.text().is_empty());
        }
        let rows_after = self.rows();
        if let Some(at) = rows_after.iter().position(|r| *r == row) {
            self.focus = at;
        } else {
            self.focus = self.focus.min(rows_after.len() - 1);
        }
        self.dirty = true;
        self.check();
        GlobalKey::Edited
    }
}

/// A key for term `i` of `list`; typing in the spare row adds the term.
fn edit_term(list: &mut Vec<TextField>, i: usize, key: KeyEvent) -> FieldKey {
    if i == list.len() {
        let mut field = TextField::default();
        let done = field.on_key(key);
        if done == FieldKey::Edited && !field.text().is_empty() {
            list.push(field);
        }
        return done;
    }
    list[i].on_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(d: &mut GlobalDialog, s: &str) {
        for c in s.chars() {
            d.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn terms_are_typed_in_the_spare_rows_and_removed_when_emptied() {
        let mut d = GlobalDialog::new(&GlobalFilter::default());
        // With no term the first include row has the keyboard.
        assert_eq!(d.rows()[d.focus], Row::Include(0));
        typed(&mut d, "payment");
        assert!(d.dirty);
        assert_eq!(d.rows().len(), 3 + 2 + 1, "a spare include row appears");
        // Down to the exclude row, then a term there.
        d.on_key(key(KeyCode::Down));
        d.on_key(key(KeyCode::Down));
        assert_eq!(d.rows()[d.focus], Row::Exclude(0));
        typed(&mut d, "healthcheck");
        // Switched on from the first row.
        d.focus = 0;
        d.on_key(key(KeyCode::Char(' ')));
        let gf = d.filter(&GlobalFilter::default());
        assert!(gf.enabled);
        assert_eq!(gf.include, ["payment"]);
        assert_eq!(gf.exclude, ["healthcheck"]);
        assert!(gf.is_applied());
        // An emptied term goes.
        d.focus = 3;
        for _ in 0..7 {
            d.on_key(key(KeyCode::Backspace));
        }
        assert!(d.filter(&GlobalFilter::default()).include.is_empty());
        assert_eq!(d.on_key(key(KeyCode::Esc)), GlobalKey::Close);
    }

    #[test]
    fn eight_terms_at_most_and_bad_regexes_are_marked() {
        let gf = GlobalFilter {
            include: (1..=8).map(|n| format!("t{n}")).collect(),
            is_regex: true,
            ..GlobalFilter::default()
        };
        let mut d = GlobalDialog::new(&gf);
        assert_eq!(
            d.rows()
                .iter()
                .filter(|r| matches!(r, Row::Include(_)))
                .count(),
            8,
            "no spare row past 8"
        );
        d.focus = d.rows().iter().position(|r| *r == Row::Exclude(0)).unwrap();
        typed(&mut d, "a(b");
        assert_eq!(d.invalid, vec![Row::Exclude(0)]);
        d.focus = 2;
        d.on_key(key(KeyCode::Char(' ')));
        assert!(d.invalid.is_empty(), "plain text is never invalid");
    }
}
