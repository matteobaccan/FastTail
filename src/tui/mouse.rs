// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Mouse hit testing: the rectangles of the last drawn frame, and a pure mapping from a
//! terminal cell to what lies under it.

use ratatui::layout::{Position, Rect};

/// A stream window as it was last drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowHit {
    pub tab: usize,
    /// The whole window, border included.
    pub outer: Rect,
    /// The rows area inside the border.
    pub rows: Rect,
    /// View row shown on the first line of `rows`, and how many rows were drawn.
    pub first_row: usize,
    pub row_count: usize,
}

/// A dialog as it was last drawn: its frame and its buttons (`Cancel` is absent on the
/// help dialog, which only has `OK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogHit {
    pub outer: Rect,
    pub ok: Rect,
    pub cancel: Option<Rect>,
}

/// Everything clickable on the last frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMap {
    pub tab_titles: Vec<(Rect, usize)>,
    pub windows: Vec<WindowHit>,
    pub dialog: Option<DialogHit>,
}

/// What a cell of the screen is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    DialogOk,
    DialogCancel,
    /// Inside a dialog, not on a button: nothing happens.
    DialogBody,
    /// Anywhere else while a dialog is open: closes it, like Esc.
    OutsideDialog,
    /// A title in the stream strip at the top.
    TabTitle(usize),
    /// The top border of a window, where its `[#N]` title sits.
    WindowTitle(usize),
    /// A drawn row of a window (a view row of the engine).
    Row {
        tab: usize,
        row: usize,
    },
    /// Elsewhere on a window: its border, or below the last row.
    Window(usize),
    Nothing,
}

/// Maps the cell (`col`, `row`) to what the last frame drew there. A dialog sits on top
/// of everything, so it is tested first.
pub fn hit_test(map: &HitMap, col: u16, row: u16) -> Target {
    let p = Position::new(col, row);
    if let Some(d) = &map.dialog {
        if d.ok.contains(p) {
            return Target::DialogOk;
        }
        if d.cancel.is_some_and(|c| c.contains(p)) {
            return Target::DialogCancel;
        }
        return if d.outer.contains(p) {
            Target::DialogBody
        } else {
            Target::OutsideDialog
        };
    }
    if let Some((_, tab)) = map.tab_titles.iter().find(|(r, _)| r.contains(p)) {
        return Target::TabTitle(*tab);
    }
    for w in &map.windows {
        if !w.outer.contains(p) {
            continue;
        }
        if row == w.outer.y {
            return Target::WindowTitle(w.tab);
        }
        if w.rows.contains(p) {
            let offset = (row - w.rows.y) as usize;
            if offset < w.row_count {
                return Target::Row {
                    tab: w.tab,
                    row: w.first_row + offset,
                };
            }
        }
        return Target::Window(w.tab);
    }
    Target::Nothing
}

/// The window under (`col`, `row`), for the wheel: it scrolls the window under the
/// pointer, focused or not.
pub fn window_at(map: &HitMap, col: u16, row: u16) -> Option<usize> {
    let p = Position::new(col, row);
    map.windows
        .iter()
        .find(|w| w.outer.contains(p))
        .map(|w| w.tab)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two windows side by side, 40 columns each, rows 1..=10 of the screen, over a
    /// strip with two titles.
    fn split_map() -> HitMap {
        HitMap {
            tab_titles: vec![(Rect::new(0, 0, 9, 1), 0), (Rect::new(10, 0, 9, 1), 1)],
            windows: vec![
                WindowHit {
                    tab: 0,
                    outer: Rect::new(0, 1, 40, 10),
                    rows: Rect::new(1, 2, 38, 8),
                    first_row: 100,
                    row_count: 8,
                },
                WindowHit {
                    tab: 1,
                    outer: Rect::new(40, 1, 40, 10),
                    rows: Rect::new(41, 2, 38, 8),
                    first_row: 0,
                    row_count: 3,
                },
            ],
            dialog: None,
        }
    }

    #[test]
    fn titles_rows_and_borders() {
        let map = split_map();
        assert_eq!(hit_test(&map, 12, 0), Target::TabTitle(1));
        assert_eq!(hit_test(&map, 5, 1), Target::WindowTitle(0));
        assert_eq!(hit_test(&map, 50, 1), Target::WindowTitle(1));
        // The third drawn row of the left window is view row 102.
        assert_eq!(hit_test(&map, 10, 4), Target::Row { tab: 0, row: 102 });
        // The right window drew three rows: below them is the window, not a row.
        assert_eq!(hit_test(&map, 50, 3), Target::Row { tab: 1, row: 1 });
        assert_eq!(hit_test(&map, 50, 7), Target::Window(1));
        // The left border of a window.
        assert_eq!(hit_test(&map, 40, 5), Target::Window(1));
        assert_eq!(hit_test(&map, 5, 20), Target::Nothing);
        assert_eq!(window_at(&map, 60, 8), Some(1));
        assert_eq!(window_at(&map, 60, 0), None);
    }

    #[test]
    fn a_dialog_covers_everything() {
        let mut map = split_map();
        map.dialog = Some(DialogHit {
            outer: Rect::new(10, 3, 60, 5),
            ok: Rect::new(40, 6, 6, 1),
            cancel: Some(Rect::new(48, 6, 10, 1)),
        });
        assert_eq!(hit_test(&map, 42, 6), Target::DialogOk);
        assert_eq!(hit_test(&map, 50, 6), Target::DialogCancel);
        assert_eq!(hit_test(&map, 20, 4), Target::DialogBody);
        // A row of a window outside the dialog now closes the dialog instead.
        assert_eq!(hit_test(&map, 5, 2), Target::OutsideDialog);
        assert_eq!(hit_test(&map, 12, 0), Target::OutsideDialog);
    }
}
