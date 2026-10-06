## MODIFIED Requirements

### Requirement: Keyboard Cursor Row
Each stream window SHALL have a cursor row, drawn reversed, that the keys move: `↑` `↓` / `j` `k` by one row, `PgUp` `PgDn` / `Ctrl+B` `Ctrl+F` by a page, `Home` / `g` to the first row and `End` / `G` to the last row with follow on. The view SHALL scroll to keep the cursor visible, and moving the cursor up SHALL pause follow. `←` `→` SHALL scroll sideways by one cell and `0` SHALL return to the first column, measuring wide characters by their cell width. Row actions (copy, bookmark, note, show in context, external tools) SHALL act on the selection when there is one, otherwise on the cursor row; `Shift+↑` / `Shift+↓` SHALL extend the selection from the cursor. On a collapsed group the cursor row SHALL stand for the whole group, as in the GUI. Outside HEX and ASM, the bottom-left border of the window SHALL show, after the line counts, `Ln N` with N the 1-based file line number of the cursor row (the number the line-number gutter shows, whatever the filters; the group's first line on a collapsed group), and, while the rows are scrolled sideways, `Col N` with N the first cell column drawn (1 plus the cells skipped). When the border is too narrow, `Col` and then `Ln` SHALL be left out before the counts are shortened. A click on `Ln` SHALL open Go to for that stream.

#### Scenario: Paging with the cursor
- **WHEN** a 10,000-line stream shows rows 1 to 40 with the cursor on row 40 and the user presses `PgDn`
- **THEN** the cursor is on row 80, it is visible at the bottom of the window, and follow is off.

#### Scenario: Back to the tail
- **WHEN** the cursor is on row 120 of a growing file and the user presses `G`
- **THEN** the cursor is on the last row, follow is on, and appended lines keep the cursor on the new last row.

#### Scenario: Line of the cursor with a filter
- **WHEN** an include filter shows 50 of 10,000 lines and the cursor is on the third shown row, which is line 1,234 of the file
- **THEN** the bottom border reads `50/10,000 lines - Ln 1,234` and shows no `Col`.

#### Scenario: Scrolled sideways
- **WHEN** the user presses `→` 40 times
- **THEN** the bottom border shows `Col 41`, and after `0` it no longer shows `Col`.

#### Scenario: Narrow window
- **WHEN** a window is 30 columns wide with the rows scrolled sideways
- **THEN** `Col` is left out of the bottom border before `Ln`, and `Ln` before the counts.

#### Scenario: HEX unchanged
- **WHEN** the stream is in HEX
- **THEN** the bottom border shows the byte and row counts as before, without `Ln` or `Col`.
