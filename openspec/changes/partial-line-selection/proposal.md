## Why

Selection in FastTail is by row: a click selects a whole line and `CTRL + C` copies it.
To copy one request id, a URL or the exception message out of a 300-character line the
user copies the whole line and trims it in another program. SnakeTail (#60) and Tailviewer
(#231) users asked for exactly this; every editor and terminal does it. The post-0.12.0
competitor scan ranks it gap 19 (value Low–Medium, effort M).

## What Changes

- **Character selection inside a row** in the Text view: pressing the mouse on the text of
  a row and dragging selects characters of that row; the selection is drawn with the
  theme's text-selection colour over the row tints.
- **Keyboard**: a click on the text places a **caret** in that row (the row is also
  selected, as today); `SHIFT + ←` / `SHIFT + →` extend the character selection by one
  character, `CTRL + SHIFT + ←` / `→` by one word, `SHIFT + Home` / `SHIFT + End` to the
  start / end of the row. `Esc` clears the character selection.
- **Copy**: with a character selection, `CTRL + C` copies just that text; without one it
  copies the selected rows as today. The row menu gains "Copy selected text" when a
  character selection exists.
- **Search from the selection**: `CTRL + F` with a character selection puts the text in
  the search box (like editors do).
- A click without a drag, `SHIFT + click`, `CTRL + click` and `CTRL + A` keep their row
  meaning. A **double-click** selects the word under the pointer as a character selection,
  and triple-click the whole row text; the selection highlight of `quick-wins-0-13` (which
  also starts from a double-click) outlines that word's other occurrences at the same time.
- No new `fasttail.ini` key.

Target release: **0.14.0** (planned for 0.13.0, moved when 0.13.0 shipped early), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low–Medium**. Effort: **M**.

### Non-goals

- Character selection across several rows (editor-style ranges); a drag that leaves the
  row stays clamped to that row.
- Character selection in the HEX view, the rendered Markdown view, the Find results list
  and the search results pane.
- Character selection inside the pretty-printed JSON expansion of a row (the expansion is
  copied with its row, as today).
- Keyboard-only caret movement without `SHIFT` (plain arrows keep scrolling the view).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `selection-and-export`: new requirement Character Selection Inside a Row.

## Impact

- `src/ui/dock.rs`: rows sense click-and-drag on the text area; hit-test with the row's
  `Galley` (`cursor_from_pos`) in both the unwrapped and the wrapped layout, taking the
  horizontal scroll into account; the selection painted as a layer between the row tints
  and the text; `Esc`, `SHIFT` arrows and `CTRL + C` routing.
- `src/tail_engine.rs`: `char_selection: Option<(line, Range<usize>)>` per stream, cleared
  on truncation, reload and when the line leaves the view's filter; byte-range mapping
  between the shown text and the line text (ANSI stripped, long-line cap).
- With `structured-fields`: in the column view a selection stays inside one cell (tasks).
- `src/i18n.rs` (16 languages), help dialog, README shortcuts table, CHANGELOG.
- **TUI (0.20.0, PR #132)**: terminals select characters themselves; the TUI keeps `y` +
  OSC 52 and its mouse-off mode, so this change adds nothing there beyond the shared
  "copy text range" helper.
