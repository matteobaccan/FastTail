// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Mouse hit testing: the rectangles of the last drawn frame, and a pure mapping from a
//! terminal cell to what lies under it.

use ratatui::layout::{Position, Rect};

use crate::tui::dock::{DividerArea, LeafArea, Place};
use crate::tui::keys::Action;

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
    /// Rows of a list inside the open dialog, and the list position each shows.
    pub list_items: Vec<(Rect, usize)>,
    /// The `[ ... ]` buttons of the status bar, and their position in its list.
    pub buttons: Vec<(Rect, usize)>,
    /// The buttons of the top bar and what they run.
    pub top_buttons: Vec<(Rect, Action)>,
    /// The chips of the stream bar and filter row inside each window: the stream and
    /// the chip's position in `crate::tui::bars::BarChip`'s list.
    pub bar_chips: Vec<(Rect, usize, crate::tui::bars::BarChip)>,
    /// The dock's leaves, where a moved window can be dropped.
    pub leaves: Vec<LeafArea>,
    /// The dock's dividers, dragged to resize.
    pub dividers: Vec<DividerArea>,
    /// The titles of a window's tabs in its top border: the window and the tab's place.
    pub leaf_tabs: Vec<(Rect, Place, usize)>,
    /// The floating windows as drawn, bottom to top, with their place in `App::floats`.
    pub floats: Vec<(Rect, usize)>,
    /// The area of the windows (between the tab strip and the status bar).
    pub main: Rect,
    /// The `[x]` of each stream window, top right: the window it closes a stream of.
    pub close_buttons: Vec<(Rect, Place)>,
}

/// What a cell of the screen is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    DialogOk,
    DialogCancel,
    /// Inside a dialog, not on a button: nothing happens.
    DialogBody,
    /// A row of the list in the open dialog (its position in the list).
    ListItem(usize),
    /// A `[ ... ]` button of the status bar (its position in the bar).
    Button(usize),
    /// A button of the top bar.
    TopButton(Action),
    /// A chip of a window's stream bar or filter row: the stream and the chip.
    BarChip(usize, crate::tui::bars::BarChip),
    /// Anywhere else while a dialog is open: closes it, like Esc.
    OutsideDialog,
    /// A title in the stream strip at the top.
    TabTitle(usize),
    /// A tab title in a window's top border (its position in `HitMap::leaf_tabs`).
    LeafTab(usize),
    /// A divider between two windows (its position in `HitMap::dividers`).
    Divider(usize),
    /// The title row of a floating window (its position in `App::floats`): dragged, it
    /// moves the window.
    FloatTitle(usize),
    /// A corner or edge of a floating window (its position in `App::floats`): dragged,
    /// it resizes the window on those sides.
    FloatEdge(usize, Edges),
    /// The `[x]` of a window (its position in `HitMap::close_buttons`): closes the
    /// stream the window shows.
    Close(usize),
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

/// The sides of a floating window a drag moves: a corner moves two, an edge one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// The sides of the floating window `r` under (`col`, `row`): the four corners, the
/// bottom row, the left and right borders. The rest of the top row is the title.
fn edges_at(r: Rect, col: u16, row: u16) -> Option<Edges> {
    let left = col <= r.x + 1;
    let right = col + 2 >= r.right();
    let top = row == r.y;
    let bottom = row + 1 == r.bottom();
    let edges = if top {
        // Only the corner cells of the title row resize.
        Edges {
            left: col == r.x,
            right: col + 1 == r.right(),
            top: true,
            bottom: false,
        }
    } else if bottom {
        Edges {
            left,
            right,
            top: false,
            bottom: true,
        }
    } else {
        Edges {
            left: col == r.x,
            right: col + 1 == r.right(),
            ..Edges::default()
        }
    };
    (edges.left || edges.right || edges.bottom).then_some(edges)
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
        if let Some((_, i)) = map.list_items.iter().find(|(r, _)| r.contains(p)) {
            return Target::ListItem(*i);
        }
        return if d.outer.contains(p) {
            Target::DialogBody
        } else {
            Target::OutsideDialog
        };
    }
    if let Some((_, i)) = map.buttons.iter().find(|(r, _)| r.contains(p)) {
        return Target::Button(*i);
    }
    if let Some((_, action)) = map.top_buttons.iter().find(|(r, _)| r.contains(p)) {
        return Target::TopButton(*action);
    }
    // A chip lies inside its window, which may be under a floating one: the topmost
    // window under the pointer must be the chip's.
    if let Some((_, tab, chip)) = map.bar_chips.iter().rev().find(|(r, _, _)| r.contains(p)) {
        let top = map.windows.iter().rev().find(|w| w.outer.contains(p));
        if top.is_some_and(|w| w.tab == *tab) {
            return Target::BarChip(*tab, *chip);
        }
    }
    if let Some((_, tab)) = map.tab_titles.iter().find(|(r, _)| r.contains(p)) {
        return Target::TabTitle(*tab);
    }
    // Floating windows lie over the dock: the topmost under the pointer takes it.
    if let Some((r, f)) = map.floats.iter().rev().find(|(r, _)| r.contains(p)) {
        let close = map
            .close_buttons
            .iter()
            .position(|(b, place)| *place == Place::Float(*f) && b.contains(p));
        if let Some(i) = close {
            return Target::Close(i);
        }
        let tab = map
            .leaf_tabs
            .iter()
            .position(|(t, place, _)| *place == Place::Float(*f) && t.contains(p));
        if let Some(i) = tab {
            return Target::LeafTab(i);
        }
        if let Some(edges) = edges_at(*r, col, row) {
            return Target::FloatEdge(*f, edges);
        }
        if row == r.y {
            return Target::FloatTitle(*f);
        }
        return window_target(map, p);
    }
    if let Some(i) = map
        .close_buttons
        .iter()
        .position(|(b, place)| matches!(place, Place::Dock(_)) && b.contains(p))
    {
        return Target::Close(i);
    }
    if let Some(i) = map.leaf_tabs.iter().position(|(r, _, _)| r.contains(p)) {
        return Target::LeafTab(i);
    }
    // The innermost divider wins where two meet.
    if let Some(i) = map.dividers.iter().rposition(|d| d.handle.contains(p)) {
        return Target::Divider(i);
    }
    window_target(map, p)
}

/// What a window shows at `p`: its title, a row, or the window. Windows drawn later lie
/// on top, so they are tested first.
fn window_target(map: &HitMap, p: Position) -> Target {
    let row = p.y;
    for w in map.windows.iter().rev() {
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
        .rev()
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
            buttons: vec![(Rect::new(2, 30, 8, 1), 4)],
            ..HitMap::default()
        }
    }

    #[test]
    fn titles_rows_and_borders() {
        let map = split_map();
        assert_eq!(hit_test(&map, 12, 0), Target::TabTitle(1));
        assert_eq!(hit_test(&map, 5, 30), Target::Button(4));
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
    fn dividers_and_border_tabs_come_before_the_windows() {
        let mut map = split_map();
        map.dividers.push(DividerArea {
            path: vec![],
            dir: crate::dock_layout::Dir::Horizontal,
            split: Rect::new(0, 1, 80, 10),
            handle: Rect::new(39, 2, 2, 9),
        });
        map.leaf_tabs
            .push((Rect::new(45, 1, 8, 1), Place::Dock(vec![true]), 1));
        assert_eq!(hit_test(&map, 40, 5), Target::Divider(0));
        assert_eq!(hit_test(&map, 39, 5), Target::Divider(0));
        assert_eq!(hit_test(&map, 47, 1), Target::LeafTab(0));
        assert_eq!(hit_test(&map, 60, 1), Target::WindowTitle(1));
    }

    #[test]
    fn a_floating_window_covers_the_windows_under_it() {
        let mut map = split_map();
        // A floating window drawn last, over both windows and their divider.
        map.windows.push(WindowHit {
            tab: 2,
            outer: Rect::new(30, 3, 20, 6),
            rows: Rect::new(31, 4, 18, 4),
            first_row: 0,
            row_count: 4,
        });
        map.floats.push((Rect::new(30, 3, 20, 6), 0));
        map.dividers.push(DividerArea {
            path: vec![],
            dir: crate::dock_layout::Dir::Horizontal,
            split: Rect::new(0, 1, 80, 10),
            handle: Rect::new(39, 2, 2, 9),
        });
        map.close_buttons
            .push((Rect::new(46, 3, 3, 1), Place::Float(0)));
        map.close_buttons
            .push((Rect::new(76, 1, 3, 1), Place::Dock(vec![true])));
        assert_eq!(hit_test(&map, 47, 3), Target::Close(0));
        assert_eq!(hit_test(&map, 77, 1), Target::Close(1));
        assert_eq!(hit_test(&map, 35, 3), Target::FloatTitle(0));
        // Every corner and edge but the title resizes.
        let e = |left, right, top, bottom| Edges {
            left,
            right,
            top,
            bottom,
        };
        assert_eq!(
            hit_test(&map, 49, 8),
            Target::FloatEdge(0, e(false, true, false, true))
        );
        assert_eq!(
            hit_test(&map, 48, 8),
            Target::FloatEdge(0, e(false, true, false, true))
        );
        assert_eq!(
            hit_test(&map, 30, 8),
            Target::FloatEdge(0, e(true, false, false, true))
        );
        assert_eq!(
            hit_test(&map, 40, 8),
            Target::FloatEdge(0, e(false, false, false, true))
        );
        assert_eq!(
            hit_test(&map, 30, 3),
            Target::FloatEdge(0, e(true, false, true, false))
        );
        assert_eq!(
            hit_test(&map, 49, 3),
            Target::FloatEdge(0, e(false, true, true, false))
        );
        assert_eq!(
            hit_test(&map, 30, 5),
            Target::FloatEdge(0, e(true, false, false, false))
        );
        assert_eq!(
            hit_test(&map, 49, 5),
            Target::FloatEdge(0, e(false, true, false, false))
        );
        assert_eq!(hit_test(&map, 40, 5), Target::Row { tab: 2, row: 1 });
        assert_eq!(window_at(&map, 40, 5), Some(2));
        // Outside it, the divider and the windows under it are reached as before.
        assert_eq!(hit_test(&map, 40, 9), Target::Divider(0));
        assert_eq!(hit_test(&map, 10, 4), Target::Row { tab: 0, row: 102 });
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
        map.list_items.push((Rect::new(11, 4, 58, 1), 3));
        assert_eq!(hit_test(&map, 20, 4), Target::ListItem(3));
        // A row of a window outside the dialog now closes the dialog instead.
        assert_eq!(hit_test(&map, 5, 2), Target::OutsideDialog);
        assert_eq!(hit_test(&map, 12, 0), Target::OutsideDialog);
    }
}
