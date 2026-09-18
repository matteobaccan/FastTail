## Context

`show_rows` assumes a constant row height and maps scroll offset to line index arithmetically, which is what makes 12 million lines cheap. Wrapping breaks the constant-height assumption.

## Goals / Non-Goals

**Goals:** readable long lines without horizontal scrolling, no regression in extend mode, keyboard toggle.

**Non-Goals:** wrap in HEX view (fixed columns) or MD rendered view (already flows); exact scrollbar proportion in wrap mode for huge files.

## Decisions

- **Estimated total height, exact visible height.** In wrap mode the scroll area uses `average_row_height * visible_line_count` as its virtual size, and lays out the rows around the current offset with their real wrapped heights, re-anchoring the offset to the first visible line index. This keeps memory O(visible) and scrolling by line correct, at the cost of a scrollbar thumb that is approximate on files with very uneven line lengths.
- **Line-index anchoring** is the contract for all navigation (search jump, go-to, PgUp/PgDown), so those keep working unchanged.
- **Toggle is per stream** and persisted in the workspace entry, since wrapping wanted for a JSON log is unwanted for an access log.

## Risks / Trade-offs

- [Approximate scrollbar in wrap mode] → documented; extend mode stays exact.
- [Layout cost of wrapping very long lines] → rows are truncated to 64 KB for layout, the rest is reachable via copy/export.
