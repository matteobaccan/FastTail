## Why

In a terminal stream window the cursor row is drawn reversed, but nothing says which line
of the file it is: with line numbers off, or with filters that skip lines, the user cannot
tell where they are. `←` / `→` scroll the rows sideways, and nothing says from which column
the rows are drawn, so after a few presses it is not clear how much of the line start is
hidden. Reported by the maintainer on 2026-10-06.

## What Changes

- The bottom-left border of each stream window, after the `visible/total lines` count,
  shows the cursor position: `Ln 1,234` (the file line number of the cursor row, as the
  line-number gutter numbers it) and `Col 41` (the first cell column drawn, 1 when not
  scrolled sideways).
- `Col` is shown only when the rows are scrolled sideways, so an unscrolled window keeps
  today's shorter border; `Ln` is always shown when the stream has a cursor row.
- HEX and ASM windows are unchanged (they already show the offset).
- When the border has no room, `Col` and then `Ln` are dropped before the counts.
- A click on `Ln` opens Go to (`Ctrl+G`).
- No key is added to `fasttail.ini`.

Target release: the next **nightly patch (0.20.x)**. Priority: **high**. Effort: **XS
(a day)**.

### Non-goals

- A cursor column (a caret inside the row); the cursor stays a whole row.
- Showing the position in the GUI (the window has no keyboard cursor row).
- Horizontal scrolling with the mouse.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `terminal-interface`: Keyboard Cursor Row (the cursor's line and the first drawn column
  shown in the window's bottom border).

## Impact

- `src/tui/app.rs`: `counts_text` (or the bottom-left title of `draw_stream`) gains the
  position and a hit rect for the click; `Tab::cursor_line` and `Tab::hscroll` already
  hold it.
- `src/i18n_tui.rs`: `Ln {0}` and `Col {0}` in every language.
- `docs/tui.md`, `docs/ui-design.md`, CHANGELOG, tests.
