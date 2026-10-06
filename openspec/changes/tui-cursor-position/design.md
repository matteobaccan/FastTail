## Context

`draw_stream` (`src/tui/app.rs`) builds the bottom border from `counts_text` (left) and
`view_state_text` (right). The cursor row is `Tab::cursor`, a view row; `Tab::cursor_line()`
maps it to the file line index (`None` in HEX / ASM). `Tab::hscroll` is the number of cells
skipped at the start of every row (`←` `→` by 1, fast by 10, `0` back to 0).

All of it runs on the UI thread at draw time: one `format!` per window per frame, no
per-line work, no memory per file or line.

## Goals / Non-Goals

**Goals:** the user always knows the file line of the cursor and from which column the
rows are drawn.

**Non-Goals:** a caret column, the GUI, mouse horizontal scrolling.

## Decisions

1. **Line number = the file line**, 1-based, the same number the gutter shows
   (`cursor_line() + 1`), not the view row: with a filter, `Ln 1,234` still finds the line
   with Go to and in another editor. Alternative: `row/visible`; rejected, the count left
   of it already gives the visible total. On a collapsed group it is the group's first line.
2. **Column = `hscroll + 1`**, in terminal cells (the unit `←` `→` move by), shown only when
   `hscroll > 0`. Alternative: always shown; rejected to keep narrow windows readable.
3. **Placement:** appended to the bottom-left text with the same separator as the scan
   progress (` - `), before any progress: `120/5,000 lines - Ln 1,234 Col 41 - filtering 40%`.
4. **Room:** the text is built in priority order and cut to the border width minus the
   right-hand text: `Col` goes first, then `Ln`.
5. **Click on `Ln`** opens the Go to dialog of that stream; the hit rect is recorded with
   the other border hits.
6. **Empty stream / no cursor:** `Ln` is omitted.

## Risks / Trade-offs

- [Large files above 16 MB still being indexed] → `cursor_line` answers for indexed lines,
  and the number of an indexed line does not change while indexing goes on.
- [Truncated or rotated file] → the engine clamps the cursor as today; the number follows
  on the next frame.
- [Wide CJK labels push the right-hand text] → the drop order of decision 4.

## Migration Plan

None.

## Open Questions

- Show `Ln` as `Ln 1,234/5,000` (out of the total)? Proposed: no, the count is already there.
