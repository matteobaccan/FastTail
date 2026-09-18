//! Viewport anchoring for the wrapped text view.
//!
//! In wrap mode rows have different heights, so the viewport cannot be mapped to a line
//! index arithmetically. The view keeps an anchor instead: the visible row at the top of
//! the viewport and how many pixels of it are hidden above the top edge. Scrolling moves
//! the anchor by pixels through the real row heights, jumps set it directly, and the
//! scroll bar is fed an approximate offset (`row * average height + within`). Everything
//! here is pure so it can be tested without a UI.

/// Bytes of a line handed to the text layout in wrap mode; the rest stays reachable
/// through copy / export.
pub const WRAP_LAYOUT_CAP: usize = 64 * 1024;

/// Scroll requests issued by shortcuts and jumps, resolved by the wrap renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WrapScroll {
    /// First row at the top of the viewport.
    Top,
    /// Last row at the bottom of the viewport (follow mode).
    Bottom,
    /// Move the anchor by whole rows (negative = up).
    Lines(i64),
    /// Move by viewport pages (negative = up).
    Pages(i32),
    /// Centre the given line index (not a visible row) in the viewport.
    CenterLine(usize),
}

/// The visible row at the top of the viewport and the pixels of it hidden above the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WrapAnchor {
    pub row: usize,
    pub within: f32,
}

impl WrapAnchor {
    pub const TOP: WrapAnchor = WrapAnchor {
        row: 0,
        within: 0.0,
    };
}

/// The prefix of `text` given to the layout: at most `WRAP_LAYOUT_CAP` bytes, cut on a
/// character boundary.
pub fn layout_slice(text: &str) -> &str {
    if text.len() <= WRAP_LAYOUT_CAP {
        return text;
    }
    let mut end = WRAP_LAYOUT_CAP;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Moves `anchor` by `delta` pixels (positive = down) through rows `0..rows` whose heights
/// come from `height`. Clamps at the first row (nothing hidden above) and at the last row.
pub fn walk_anchor(
    anchor: WrapAnchor,
    delta: f32,
    rows: usize,
    mut height: impl FnMut(usize) -> f32,
) -> WrapAnchor {
    if rows == 0 {
        return WrapAnchor::TOP;
    }
    let mut row = anchor.row.min(rows - 1);
    let mut within = anchor.within.max(0.0) + delta;
    if within >= 0.0 {
        loop {
            let h = height(row).max(1.0);
            if within < h {
                break;
            }
            if row + 1 >= rows {
                // Past the last row: keep its bottom edge reachable, not beyond.
                within = within.min(h - 1.0).max(0.0);
                break;
            }
            within -= h;
            row += 1;
        }
    } else {
        while within < 0.0 {
            if row == 0 {
                within = 0.0;
                break;
            }
            row -= 1;
            within += height(row).max(1.0);
        }
    }
    WrapAnchor { row, within }
}

/// The anchor that puts the bottom of the last row on the bottom edge of a viewport
/// `viewport_h` tall. When every row fits, the anchor is the top.
pub fn anchor_to_bottom(
    rows: usize,
    viewport_h: f32,
    mut height: impl FnMut(usize) -> f32,
) -> WrapAnchor {
    if rows == 0 {
        return WrapAnchor::TOP;
    }
    let mut row = rows - 1;
    let mut filled = height(row).max(1.0);
    while filled < viewport_h {
        if row == 0 {
            return WrapAnchor::TOP;
        }
        row -= 1;
        filled += height(row).max(1.0);
    }
    WrapAnchor {
        row,
        within: (filled - viewport_h).max(0.0),
    }
}

/// The anchor that centres `row` in the viewport (or shows it from the top when it is
/// taller than the viewport).
pub fn anchor_center(
    row: usize,
    rows: usize,
    viewport_h: f32,
    mut height: impl FnMut(usize) -> f32,
) -> WrapAnchor {
    if rows == 0 {
        return WrapAnchor::TOP;
    }
    let row = row.min(rows - 1);
    let h = height(row).max(1.0);
    let above = ((viewport_h - h) / 2.0).max(0.0);
    walk_anchor(WrapAnchor { row, within: 0.0 }, -above, rows, &mut height)
}

/// Rows laid out from `anchor` until the viewport is filled or the rows run out:
/// `(rows laid out, pixels covered below the viewport top)`.
pub fn fill_from(
    anchor: WrapAnchor,
    rows: usize,
    viewport_h: f32,
    mut height: impl FnMut(usize) -> f32,
) -> (usize, f32) {
    let mut covered = -anchor.within;
    let mut count = 0;
    let mut row = anchor.row;
    while row < rows && covered < viewport_h {
        covered += height(row).max(1.0);
        count += 1;
        row += 1;
    }
    (count, covered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(row: usize) -> f32 {
        // Rows 0..10: 20 px, row 5 is a long wrapped line of 100 px.
        if row == 5 {
            100.0
        } else {
            20.0
        }
    }

    #[test]
    fn layout_slice_caps_on_char_boundary() {
        let short = "abc";
        assert_eq!(layout_slice(short), short);
        // 'é' is two bytes; build a string whose cap lands inside a character.
        let mut long = "x".repeat(WRAP_LAYOUT_CAP - 1);
        long.push('é');
        long.push_str("tail");
        let sliced = layout_slice(&long);
        assert_eq!(sliced.len(), WRAP_LAYOUT_CAP - 1);
        assert!(sliced.ends_with('x'));
        let mut exact = "y".repeat(WRAP_LAYOUT_CAP);
        exact.push_str("more");
        assert_eq!(layout_slice(&exact).len(), WRAP_LAYOUT_CAP);
    }

    #[test]
    fn walk_forward_crosses_rows_and_clamps_at_end() {
        let a = walk_anchor(WrapAnchor::TOP, 45.0, 10, h);
        assert_eq!(
            a,
            WrapAnchor {
                row: 2,
                within: 5.0
            }
        );
        // Into the tall row: 40 px covers rows 0-1, row 5 starts at 100 px.
        let a = walk_anchor(WrapAnchor::TOP, 130.0, 10, h);
        assert_eq!(
            a,
            WrapAnchor {
                row: 5,
                within: 30.0
            }
        );
        // Way past the end: last row, nothing beyond its bottom.
        let a = walk_anchor(WrapAnchor::TOP, 10_000.0, 10, h);
        assert_eq!(a.row, 9);
        assert!(a.within >= 0.0 && a.within < 20.0);
    }

    #[test]
    fn walk_backward_clamps_at_top() {
        let a = walk_anchor(
            WrapAnchor {
                row: 6,
                within: 5.0,
            },
            -10.0,
            10,
            h,
        );
        assert_eq!(
            a,
            WrapAnchor {
                row: 5,
                within: 95.0
            }
        );
        let a = walk_anchor(
            WrapAnchor {
                row: 2,
                within: 5.0,
            },
            -1_000.0,
            10,
            h,
        );
        assert_eq!(a, WrapAnchor::TOP);
    }

    #[test]
    fn walk_is_reversible() {
        let start = WrapAnchor {
            row: 3,
            within: 7.0,
        };
        let down = walk_anchor(start, 150.0, 10, h);
        let back = walk_anchor(down, -150.0, 10, h);
        assert_eq!(back, start);
    }

    #[test]
    fn empty_rows_anchor_at_top() {
        assert_eq!(
            walk_anchor(
                WrapAnchor {
                    row: 4,
                    within: 3.0
                },
                50.0,
                0,
                h
            ),
            WrapAnchor::TOP
        );
        assert_eq!(anchor_to_bottom(0, 100.0, h), WrapAnchor::TOP);
        assert_eq!(anchor_center(3, 0, 100.0, h), WrapAnchor::TOP);
    }

    #[test]
    fn bottom_anchor_fills_the_viewport_with_the_last_rows() {
        // Rows 6..10 are 4 x 20 = 80 px; a 100 px viewport also needs 20 px of row 5.
        let a = anchor_to_bottom(10, 100.0, h);
        assert_eq!(
            a,
            WrapAnchor {
                row: 5,
                within: 80.0
            }
        );
        // Everything fits: top.
        assert_eq!(anchor_to_bottom(3, 100.0, h), WrapAnchor::TOP);
    }

    #[test]
    fn center_anchor_places_the_row_mid_viewport() {
        // Row 8 (20 px) in a 100 px viewport: 40 px above it = rows 6 and 7.
        let a = anchor_center(8, 10, 100.0, h);
        assert_eq!(
            a,
            WrapAnchor {
                row: 6,
                within: 0.0
            }
        );
        // A row taller than the viewport is shown from its top.
        let a = anchor_center(5, 10, 60.0, h);
        assert_eq!(
            a,
            WrapAnchor {
                row: 5,
                within: 0.0
            }
        );
        // Near the top there is nothing above: clamps.
        assert_eq!(anchor_center(0, 10, 100.0, h), WrapAnchor::TOP);
    }

    #[test]
    fn fill_counts_rows_until_the_viewport_is_covered() {
        let (count, covered) = fill_from(
            WrapAnchor {
                row: 3,
                within: 10.0,
            },
            10,
            100.0,
            h,
        );
        // row 3: 10 px visible, row 4: 20, row 5: 100 -> 130 >= 100 after 3 rows.
        assert_eq!(count, 3);
        assert_eq!(covered, 130.0);
        let (count, covered) = fill_from(
            WrapAnchor {
                row: 8,
                within: 0.0,
            },
            10,
            100.0,
            h,
        );
        assert_eq!(count, 2, "runs out of rows");
        assert_eq!(covered, 40.0);
    }
}
