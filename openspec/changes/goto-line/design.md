## Context

`TailEngine` exposes `requested_scroll_y` and the UI already jumps to a target row for search matches (`show_rows` maps a line index to a virtual row through `filtered_lines.partition_point`). Go-to-line reuses that path with a user-provided index.

## Goals / Non-Goals

**Goals:** reach any line by number in two keystrokes, behave sensibly with filters, keep line numbers 1-based as displayed.

**Non-Goals:** go to byte offset (HEX view has byte search already); go to timestamp (separate change `timestamp-range-filter`).

## Decisions

- **1-based in the UI, 0-based in the engine**, converted at the popup boundary; the popup rejects 0 and non-numeric input inline.
- **Under a filter, resolve to the first visible line at or after the target** so the user lands where the requested line would be; the status text shows "line 1,234,567 hidden, showing 1,234,570".
- **Relative jumps** `+N` / `-N` are parsed by the popup and applied to the current top line, a cheap addition that avoids arithmetic for the user.
- **Follow mode pauses** on jump, matching Ctrl+Home behaviour.

## Risks / Trade-offs

- [Ctrl+G is used by egui for nothing today] → free; documented in the shortcut table.
