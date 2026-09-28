## Context

Rows are painted in `dock.rs`: each visible row is laid out as an egui `Galley` (a
`LayoutJob` with the rule, label, level, ANSI and hit spans), wrapped or not, and painted
with `painter.galley`; the row area senses clicks only (`Sense::click()`), a click selects
the row, `SHIFT` / `CTRL` extend or toggle. The shown text is the line after ANSI
handling, cut at the long-line cap; horizontal scrolling offsets the text. `CTRL + C` is
handled per focused stream. Plain arrows scroll the view.

## Goals / Non-Goals

**Goals:** copy part of a line with the mouse or keyboard, with no change to row selection
habits; exact bytes copied (no tints, no line number).

**Non-Goals:** multi-row character ranges, other views.

## Decisions

### D1. One character selection per stream, inside one row
`TailEngine::char_selection: Option<CharSelection { line, anchor, head }>` with byte
offsets into the **shown** text (see D3). It lives beside the row selection and never
replaces it: pressing on the text also selects the row (a click is a click), dragging then
extends the character range. The selection is cleared by `Esc`, by a click on another row,
by truncation or reload, and when a filter change hides its line. Memory: a few bytes.
*Alternative:* multi-row ranges — rejected for now: rows are virtualised and can be
hundreds of millions, collapsed or filtered; a range across hidden rows has no obvious
meaning, and copying several whole rows is already covered.

### D2. Hit-testing
The row keeps its `Galley` for the frame (already built to paint it); a press or drag
position is converted with `Galley::cursor_from_pos(pos - text_origin)` where the origin
includes the horizontal scroll and the gutter columns. A drag beyond the row clamps to its
start or end; dragging near the left or right edge scrolls horizontally, 1 column per 16 ms.
In wrap mode the galley is multi-line and the same call works. The sense becomes
`click_and_drag` on the text area only; the gutter (markers, line numbers, time delta)
keeps click-only so a drag there does nothing new.

### D3. What is copied
The shown text differs from the stored line only by ANSI stripping (render / strip modes)
and the long-line cap; the copy takes the selected range of the shown text, i.e. what the
user sees. Tabs are copied as tabs. A collapsed row's `×N` badge is not part of the text.
A selection inside the cap never reaches beyond it.

### D4. Keys
A click on text sets the caret (an empty `CharSelection`). With a caret, `SHIFT + ←/→`,
`CTRL + SHIFT + ←/→` (word boundaries: Unicode word segmentation of egui's text cursor),
`SHIFT + Home/End` move the head; plain arrows still scroll the view and keep the caret.
These keys are consumed only when a caret exists in the focused stream, so `SHIFT + F2`
and other shortcuts are untouched. `CTRL + C`: character selection first, then rows.
`CTRL + F` with a non-empty character selection of at most 256 characters fills the
search box with it. Once `remappable-shortcuts` exists, these become actions in its table.

### D5. Double- and triple-click
Double-click selects the word under the pointer (same word rule as D4); triple-click
selects the whole shown text. `quick-wins-0-13` uses the double-click to outline the
token's other occurrences: both happen on the same gesture, with the token rule of that
change deciding the outline and this change deciding the copy range.

### D6. UI thread only
Everything runs on the UI thread on rows already laid out: no file access beyond the row
being painted, no cost when no selection exists.

## Risks / Trade-offs

- [Accidental drags while clicking to select rows] → a drag starts after 4 px of motion
  (egui's default threshold); below it a press is a click.
- [Wrapped rows with the pretty-printed JSON block] → the block is outside the text galley
  and not selectable (non-goal).
- [Right-to-left scripts] → egui's galley handles cursor mapping; covered by a test with
  Arabic text as far as egui supports it.

## Migration Plan

Nothing to migrate.

## Open Questions

- Should `SHIFT + ↑/↓` extend into the neighbouring rows as whole rows (hybrid)? Proposed:
  no, keep one row; revisit after user feedback.
- Should the selection survive a follow-mode scroll that moves its row off screen?
  Proposed: yes, it stays until cleared; `CTRL + C` still copies it.
