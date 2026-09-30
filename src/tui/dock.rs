// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The dock on a terminal: the pane tree laid out in cells, the dividers the mouse
//! drags to resize, and the drop zones of a window being moved (its edges split, its
//! centre adds a tab), as in the GUI.

use ratatui::layout::Rect;

use crate::dock_layout::{Dir, Pane};

/// A leaf of the tree and the cells it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafArea {
    pub path: Vec<bool>,
    pub area: Rect,
}

/// A split: its path, its direction, the cells it covers and the handle the mouse
/// drags (the border between its two children).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DividerArea {
    pub path: Vec<bool>,
    pub dir: Dir,
    pub split: Rect,
    pub handle: Rect,
}

/// Smallest side of a window: its two borders and one row or column.
const MIN_SIDE: u16 = 3;

/// Lays `pane` out in `area`: the leaves in order, and the dividers.
pub fn layout(pane: &Pane, area: Rect) -> (Vec<LeafArea>, Vec<DividerArea>) {
    fn go(
        pane: &Pane,
        area: Rect,
        path: &mut Vec<bool>,
        leaves: &mut Vec<LeafArea>,
        dividers: &mut Vec<DividerArea>,
    ) {
        match pane {
            Pane::Leaf { .. } => leaves.push(LeafArea {
                path: path.clone(),
                area,
            }),
            Pane::Split {
                dir,
                fraction,
                first,
                second,
            } => {
                let (a, b, handle) = divide(area, *dir, *fraction);
                dividers.push(DividerArea {
                    path: path.clone(),
                    dir: *dir,
                    split: area,
                    handle,
                });
                path.push(false);
                go(first, a, path, leaves, dividers);
                path.pop();
                path.push(true);
                go(second, b, path, leaves, dividers);
                path.pop();
            }
        }
    }
    let (mut leaves, mut dividers) = (Vec::new(), Vec::new());
    go(pane, area, &mut Vec::new(), &mut leaves, &mut dividers);
    (leaves, dividers)
}

/// The two halves of `area` and the divider's handle: side by side the facing borders
/// below the title row; stacked the bottom border of the upper window (the lower one's
/// top border is its title, which moves the window).
fn divide(area: Rect, dir: Dir, fraction: f32) -> (Rect, Rect, Rect) {
    let total = match dir {
        Dir::Horizontal => area.width,
        Dir::Vertical => area.height,
    };
    let first = first_size(total, fraction);
    match dir {
        Dir::Horizontal => {
            let a = Rect {
                width: first,
                ..area
            };
            let b = Rect {
                x: area.x + first,
                width: total - first,
                ..area
            };
            let handle = Rect::new(
                (a.right()).saturating_sub(1),
                area.y + 1.min(area.height),
                2.min(area.width),
                area.height.saturating_sub(1),
            );
            (a, b, handle)
        }
        Dir::Vertical => {
            let a = Rect {
                height: first,
                ..area
            };
            let b = Rect {
                y: area.y + first,
                height: total - first,
                ..area
            };
            let handle = Rect::new(area.x, a.bottom().saturating_sub(1), area.width, 1);
            (a, b, handle)
        }
    }
}

/// Cells of the first child: the fraction of `total`, leaving both children at least
/// `MIN_SIDE` when there is room.
fn first_size(total: u16, fraction: f32) -> u16 {
    if total < 2 {
        return total;
    }
    let want = (total as f32 * fraction).round() as u16;
    let low = MIN_SIDE.min(total / 2);
    want.clamp(low, total - low)
}

/// The fraction that puts the divider's handle of `split` at the cell (`col`, `row`).
pub fn fraction_at(split: Rect, dir: Dir, col: u16, row: u16) -> f32 {
    let (start, len, at) = match dir {
        Dir::Horizontal => (split.x, split.width, col),
        Dir::Vertical => (split.y, split.height, row),
    };
    // The handle is the last cell of the first child.
    let first = at.saturating_sub(start) + 1;
    first as f32 / len.max(1) as f32
}

/// Where a window's tabs live: a leaf of the dock tree, or a floating window (its
/// position in `App::floats`, bottom to top).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Dock(Vec<bool>),
    Float(usize),
}

/// Where a window dropped on a leaf goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Left,
    Right,
    Top,
    Bottom,
    /// A tab of the leaf.
    Center,
    /// Out of the dock, a floating window at the pointer.
    Float,
}

impl Zone {
    /// The split an edge makes: its direction and whether the window goes after.
    pub fn split(self) -> Option<(Dir, bool)> {
        match self {
            Zone::Left => Some((Dir::Horizontal, false)),
            Zone::Right => Some((Dir::Horizontal, true)),
            Zone::Top => Some((Dir::Vertical, false)),
            Zone::Bottom => Some((Dir::Vertical, true)),
            Zone::Center | Zone::Float => None,
        }
    }
}

/// The zone of `area` under (`col`, `row`): the nearest edge within a fifth of the
/// window, the middle (the central 30% each way) for a tab, the ring between them for a
/// floating window.
pub fn zone_at(area: Rect, col: u16, row: u16) -> Zone {
    let fx = (col.saturating_sub(area.x) as f32 + 0.5) / area.width.max(1) as f32;
    let fy = (row.saturating_sub(area.y) as f32 + 0.5) / area.height.max(1) as f32;
    let edges = [
        (fx, Zone::Left),
        (1.0 - fx, Zone::Right),
        (fy, Zone::Top),
        (1.0 - fy, Zone::Bottom),
    ];
    let (d, zone) = edges.into_iter().fold((f32::MAX, Zone::Center), |best, e| {
        if e.0 < best.0 {
            e
        } else {
            best
        }
    });
    if d < 0.2 {
        zone
    } else if (fx - 0.5).abs() < 0.15 && (fy - 0.5).abs() < 0.15 {
        Zone::Center
    } else {
        Zone::Float
    }
}

/// Smallest floating window: a title, three rows and the bottom border.
pub const MIN_FLOAT: (u16, u16) = (20, 5);

/// A new floating window in `bounds`: 60% of it (at least 40 x 10 when there is room),
/// its title under the pointer.
pub fn float_rect_at(bounds: Rect, col: u16, row: u16) -> Rect {
    let w = (bounds.width * 3 / 5).max(40).min(bounds.width);
    let h = (bounds.height * 3 / 5).max(10).min(bounds.height);
    let r = Rect::new(col.saturating_sub(w / 2), row, w, h);
    clamp_into(r, bounds)
}

/// `r` moved (and shrunk if needed) to lie inside `bounds`.
pub fn clamp_into(r: Rect, bounds: Rect) -> Rect {
    let w = r.width.clamp(MIN_FLOAT.0.min(bounds.width), bounds.width);
    let h = r
        .height
        .clamp(MIN_FLOAT.1.min(bounds.height), bounds.height);
    let x = r.x.clamp(bounds.x, bounds.right().saturating_sub(w));
    let y = r.y.clamp(bounds.y, bounds.bottom().saturating_sub(h));
    Rect::new(x, y, w, h)
}

/// The part of `area` a drop on `zone` would fill, drawn while dragging.
pub fn zone_rect(area: Rect, zone: Zone) -> Rect {
    let half_w = area.width / 2;
    let half_h = area.height / 2;
    match zone {
        Zone::Left => Rect {
            width: half_w,
            ..area
        },
        Zone::Right => Rect {
            x: area.x + half_w,
            width: area.width - half_w,
            ..area
        },
        Zone::Top => Rect {
            height: half_h,
            ..area
        },
        Zone::Bottom => Rect {
            y: area.y + half_h,
            height: area.height - half_h,
            ..area
        },
        Zone::Center => area,
        Zone::Float => float_rect_at(area, area.x + area.width / 2, area.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tree() -> Pane {
        // a | (b over c), a on 40%.
        Pane::Split {
            dir: Dir::Horizontal,
            fraction: 0.4,
            first: Box::new(Pane::with_streams(&[PathBuf::from("a")])),
            second: Box::new(Pane::Split {
                dir: Dir::Vertical,
                fraction: 0.5,
                first: Box::new(Pane::with_streams(&[PathBuf::from("b")])),
                second: Box::new(Pane::with_streams(&[PathBuf::from("c")])),
            }),
        }
    }

    #[test]
    fn the_tree_fills_the_area_with_its_fractions() {
        let (leaves, dividers) = layout(&tree(), Rect::new(0, 1, 100, 30));
        let areas: Vec<Rect> = leaves.iter().map(|l| l.area).collect();
        assert_eq!(
            areas,
            vec![
                Rect::new(0, 1, 40, 30),
                Rect::new(40, 1, 60, 15),
                Rect::new(40, 16, 60, 15),
            ]
        );
        assert_eq!(leaves[2].path, vec![true, true]);
        // Side by side: the two facing borders below the titles.
        assert_eq!(dividers[0].handle, Rect::new(39, 2, 2, 29));
        // Stacked: the bottom border of the upper window.
        assert_eq!(dividers[1].handle, Rect::new(40, 15, 60, 1));
        assert_eq!(dividers[1].path, vec![true]);
    }

    #[test]
    fn dragging_a_handle_gives_the_fraction_back() {
        let split = Rect::new(0, 1, 100, 30);
        let f = fraction_at(split, Dir::Horizontal, 59, 10);
        assert_eq!(f, 0.6);
        let (a, _, handle) = divide(split, Dir::Horizontal, f);
        assert_eq!(a.width, 60);
        assert_eq!(handle.x, 59, "the handle follows the pointer");
        // A tiny window keeps its borders.
        assert_eq!(first_size(20, 0.01), 3);
        assert_eq!(first_size(20, 0.99), 17);
    }

    #[test]
    fn edges_split_and_the_centre_adds_a_tab() {
        let area = Rect::new(10, 10, 40, 20);
        assert_eq!(zone_at(area, 11, 20), Zone::Left);
        assert_eq!(zone_at(area, 48, 20), Zone::Right);
        assert_eq!(zone_at(area, 30, 10), Zone::Top);
        assert_eq!(zone_at(area, 30, 29), Zone::Bottom);
        assert_eq!(zone_at(area, 30, 20), Zone::Center);
        // Between the edges and the middle: out of the dock.
        assert_eq!(zone_at(area, 20, 15), Zone::Float);
        assert_eq!(zone_rect(area, Zone::Right), Rect::new(30, 10, 20, 20));
        assert_eq!(Zone::Bottom.split(), Some((Dir::Vertical, true)));
        assert_eq!(Zone::Float.split(), None);
    }

    #[test]
    fn floating_windows_stay_inside_the_screen() {
        let screen = Rect::new(0, 1, 100, 30);
        // Centred on the pointer, the title under it, 60% of the screen.
        assert_eq!(float_rect_at(screen, 50, 5), Rect::new(20, 5, 60, 18));
        // Near the bottom-right corner it is pushed back in.
        assert_eq!(float_rect_at(screen, 99, 29), Rect::new(40, 13, 60, 18));
        // A window larger than the screen shrinks; a tiny one grows to the minimum.
        assert_eq!(
            clamp_into(Rect::new(5, 0, 200, 50), screen),
            Rect::new(0, 1, 100, 30)
        );
        assert_eq!(
            clamp_into(Rect::new(5, 3, 2, 1), screen),
            Rect::new(5, 3, 20, 5)
        );
    }
}
