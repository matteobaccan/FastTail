// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Compare lines in the terminal (`=`): two lines, or two regions, side by side with the
//! window's alignment (`crate::compare`), the differing tokens marked, the window's
//! ignore toggles, and the next / previous change.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::compare::{CompareOptions, RegionDiff};

/// One side of a compare: where its lines come from and the lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    /// The stream's name, as the header shows it.
    pub name: String,
    /// 0-based line of the first line.
    pub first: usize,
    pub lines: Vec<String>,
}

/// What a key did to the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareKey {
    Moved,
    /// `y`: the compare as `diff -u` text, for the clipboard.
    Copy(String),
    Close,
    Other,
}

pub struct CompareView {
    pub left: Side,
    pub right: Side,
    pub opts: CompareOptions,
    pub diff: RegionDiff,
    /// The lines compared: the sides, each line pretty-printed as JSON in JSON mode.
    pub shown: (Vec<String>, Vec<String>),
    /// First row drawn.
    pub top: usize,
    /// The block of changes last gone to (`]` / `[`), an index into `diff.changes`.
    pub change: Option<usize>,
    /// Rows the last frame drew, for the page keys.
    pub page: usize,
}

impl CompareView {
    pub fn new(left: Side, right: Side, opts: CompareOptions) -> Self {
        let mut v = Self {
            left,
            right,
            opts,
            diff: RegionDiff::default(),
            shown: (Vec::new(), Vec::new()),
            top: 0,
            change: None,
            page: 10,
        };
        v.recompute();
        v
    }

    /// Aligns the sides again with the current options.
    fn recompute(&mut self) {
        let expand = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .flat_map(|l| crate::compare::canonical_json(l).unwrap_or_else(|| vec![l.clone()]))
                .collect()
        };
        self.shown = if self.opts.json {
            (expand(&self.left.lines), expand(&self.right.lines))
        } else {
            (self.left.lines.clone(), self.right.lines.clone())
        };
        self.diff = crate::compare::diff_regions(&self.shown.0, &self.shown.1, &self.opts);
        self.top = self.top.min(self.diff.rows.len().saturating_sub(1));
        self.change = None;
    }

    /// Whether the two sides compare equal under the options.
    pub fn is_equal(&self) -> bool {
        self.diff.changes.is_empty()
    }

    /// The toggles as `[x] time  [ ] numbers ...`, each with its key.
    pub fn toggles(&self) -> Vec<(char, &'static str, bool)> {
        vec![
            ('t', crate::i18n_tui::en("time"), self.opts.timestamp),
            ('u', crate::i18n_tui::en("numbers"), self.opts.numbers),
            ('i', crate::i18n_tui::en("ids"), self.opts.ids),
            ('w', crate::i18n_tui::en("spaces"), self.opts.whitespace),
            ('c', crate::i18n_tui::en("case"), self.opts.case),
            ('J', "JSON", self.opts.json),
        ]
    }

    fn go_change(&mut self, forward: bool) {
        let n = self.diff.changes.len();
        if n == 0 {
            return;
        }
        let next = match (self.change, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.change = Some(next);
        self.top = self.diff.changes[next].saturating_sub(2);
    }

    pub fn on_key(&mut self, key: KeyEvent) -> CompareKey {
        if key.kind == KeyEventKind::Release {
            return CompareKey::Other;
        }
        let last = self.diff.rows.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return CompareKey::Close,
            KeyCode::Up | KeyCode::Char('k') => self.top = self.top.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.top = (self.top + 1).min(last),
            KeyCode::PageUp => self.top = self.top.saturating_sub(self.page.max(1)),
            KeyCode::PageDown => self.top = (self.top + self.page.max(1)).min(last),
            KeyCode::Home | KeyCode::Char('g') => self.top = 0,
            KeyCode::End | KeyCode::Char('G') => self.top = last,
            KeyCode::Char(']') | KeyCode::Char('n') | KeyCode::F(7) => self.go_change(true),
            KeyCode::Char('[') | KeyCode::Char('N') => self.go_change(false),
            KeyCode::Char('y') => {
                return CompareKey::Copy(crate::compare::unified(
                    &self.left.name,
                    &self.right.name,
                    &self.shown.0,
                    &self.shown.1,
                ))
            }
            KeyCode::Char(c @ ('t' | 'u' | 'i' | 'w' | 'c' | 'J')) => {
                let o = &mut self.opts;
                let flag = match c {
                    't' => &mut o.timestamp,
                    'u' => &mut o.numbers,
                    'i' => &mut o.ids,
                    'w' => &mut o.whitespace,
                    'c' => &mut o.case,
                    _ => &mut o.json,
                };
                *flag = !*flag;
                self.recompute();
            }
            _ => return CompareKey::Other,
        }
        CompareKey::Moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn side(name: &str, lines: &[&str]) -> Side {
        Side {
            name: name.into(),
            first: 0,
            lines: lines.iter().map(|l| l.to_string()).collect(),
        }
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn two_lines_differ_by_a_number_until_numbers_are_ignored() {
        let mut v = CompareView::new(
            side("a.log", &["2026-10-29 10:00:00 GET /api id=1 took=120"]),
            side("a.log", &["2026-10-29 10:05:00 GET /api id=1 took=480"]),
            CompareOptions::default(),
        );
        assert!(!v.is_equal(), "the time is ignored, the number is not");
        let words = v.diff.rows[0].words.clone().unwrap();
        assert_eq!(&v.left.lines[0][words.left[0].clone()], "120");
        v.on_key(key('u'));
        assert!(v.is_equal());
        match v.on_key(key('y')) {
            CompareKey::Copy(text) => assert!(text.contains("-2026-10-29 10:00:00"), "{text}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_change_keys_walk_the_blocks_and_wrap() {
        let mut v = CompareView::new(
            side("a", &["x", "same", "y", "same2", "z"]),
            side("b", &["x2", "same", "y2", "same2", "z2"]),
            CompareOptions::default(),
        );
        let n = v.diff.changes.len();
        assert!(n >= 2, "{:?}", v.diff);
        for _ in 0..n {
            v.on_key(key(']'));
        }
        assert_eq!(v.change, Some(n - 1));
        v.on_key(key(']'));
        assert_eq!(v.change, Some(0));
        assert_eq!(v.on_key(key('q')), CompareKey::Close);
    }
}
