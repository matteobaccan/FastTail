// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Search all streams in the terminal: the window's Find results (`find_all`) in a
//! window over the dock. The query box, then the results grouped by stream (a header per
//! stream, collapsible, then its hits), walked with the arrows and the pages.

use crate::find_all::FindAllSession;
use crate::tui::form::TextField;

/// The Find results window.
pub struct FindWindow {
    pub session: FindAllSession,
    pub field: TextField,
    /// The query box has the keys (else the list).
    pub editing: bool,
    /// The row selected, and the first row on screen.
    pub selected: usize,
    pub top: usize,
    /// Drawn over the dock: `Enter` on a result hides it (the results stay, the key
    /// shows them again), `Esc` closes it.
    pub shown: bool,
}

/// A row of the results: a stream's header, or one of its hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Header(usize),
    Hit(usize, usize),
}

impl FindWindow {
    pub fn new(query: &str) -> Self {
        Self {
            session: FindAllSession::default(),
            field: TextField::new(query),
            editing: true,
            selected: 0,
            top: 0,
            shown: true,
        }
    }

    /// Rows of the list: one header per stream, the hits of the open ones.
    pub fn row_count(&self) -> usize {
        self.session
            .groups
            .iter()
            .map(|g| 1 + if g.collapsed { 0 } else { g.hits.len() })
            .sum()
    }

    /// The row at `index` (walks the groups, not the hits).
    pub fn row(&self, mut index: usize) -> Option<Row> {
        for (gi, g) in self.session.groups.iter().enumerate() {
            if index == 0 {
                return Some(Row::Header(gi));
            }
            index -= 1;
            let n = if g.collapsed { 0 } else { g.hits.len() };
            if index < n {
                return Some(Row::Hit(gi, index));
            }
            index -= n;
        }
        None
    }

    /// The index of `row` in the list.
    pub fn index_of(&self, row: Row) -> usize {
        let group = match row {
            Row::Header(g) | Row::Hit(g, _) => g,
        };
        let before: usize = self.session.groups[..group.min(self.session.groups.len())]
            .iter()
            .map(|g| 1 + if g.collapsed { 0 } else { g.hits.len() })
            .sum();
        match row {
            Row::Header(_) => before,
            Row::Hit(_, h) => before + 1 + h,
        }
    }

    /// Moves the selection by `delta` rows (clamped), keeping it within `height` rows on
    /// screen.
    pub fn move_by(&mut self, delta: isize, height: usize) {
        let n = self.row_count();
        if n == 0 {
            self.selected = 0;
            self.top = 0;
            return;
        }
        let at = self.selected as isize + delta;
        self.selected = at.clamp(0, n as isize - 1) as usize;
        self.reveal(height);
    }

    /// Keeps the selection on screen.
    pub fn reveal(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + height {
            self.top = self.selected + 1 - height;
        }
    }

    /// Opens or closes the group of the selected row (the selection goes to its header).
    pub fn toggle_group(&mut self) {
        let Some(row) = self.row(self.selected) else {
            return;
        };
        let g = match row {
            Row::Header(g) | Row::Hit(g, _) => g,
        };
        if let Some(group) = self.session.groups.get_mut(g) {
            group.collapsed = !group.collapsed;
        }
        self.selected = self.index_of(Row::Header(g));
        self.top = self.top.min(self.selected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tail_engine::TailEngine;
    use std::time::{Duration, Instant};

    fn engine(dir: &std::path::Path, name: &str, lines: &[&str]) -> TailEngine {
        let path = dir.join(name);
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let mut e = TailEngine::open(&path).unwrap();
        let start = Instant::now();
        while e.index_pending && start.elapsed() < Duration::from_secs(10) {
            e.poll_updates();
        }
        e
    }

    #[test]
    fn rows_walk_two_streams_and_groups_collapse() {
        let dir = tempfile::tempdir().unwrap();
        let a = engine(dir.path(), "a.log", &["ok", "timeout 1", "ok", "timeout 2"]);
        let b = engine(dir.path(), "b.log", &["timeout 3", "fine"]);
        let engines = [&a, &b];
        let mut w = FindWindow::new("TIMEOUT");
        w.session.input = w.field.text().to_string();
        w.session.start(&engines);
        let deadline = Instant::now() + Duration::from_secs(10);
        while w.session.is_active() {
            assert!(Instant::now() < deadline, "search did not finish");
            w.session.poll(&engines);
            std::thread::sleep(Duration::from_millis(2));
        }
        // a: header + 2 hits, b: header + 1 hit.
        assert_eq!(w.row_count(), 5);
        assert_eq!(w.row(0), Some(Row::Header(0)));
        assert_eq!(w.row(2), Some(Row::Hit(0, 1)));
        assert_eq!(w.row(3), Some(Row::Header(1)));
        assert_eq!(w.row(4), Some(Row::Hit(1, 0)));
        assert_eq!(w.row(5), None);
        assert_eq!(w.session.groups[0].hits, vec![1, 3]);
        assert_eq!(w.index_of(Row::Hit(1, 0)), 4);
        w.move_by(10, 3);
        assert_eq!((w.selected, w.top), (4, 2));
        // Collapsing the second group from its hit lands on its header.
        w.toggle_group();
        assert_eq!(w.selected, 3);
        assert_eq!(w.row_count(), 4);
        w.move_by(-10, 3);
        assert_eq!((w.selected, w.top), (0, 0));
    }
}
