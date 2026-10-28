## Context

The column view draws every column in one horizontal `ScrollArea`; the terminal one scrolls
the whole row sideways.

## Goals / Non-Goals

**Goals:** the anchor columns always visible; no change for streams without pins.

**Non-Goals:** right pins, frozen rows.

## Decisions

### D1. Two areas, one row height
The pinned columns are drawn first in a fixed area, the rest in the horizontal scroll area,
both with the row heights of the wrapped message column, so rows stay aligned.

### D2. Default pin
With `pin_time_column=true` (default) the parser's timestamp column is pinned when a stream
first shows its column view; the user's pins then win.

## Risks / Trade-offs

- [Pinned columns wider than the pane] → pins stop at half the pane width; the rest scroll.

## Open Questions

- Pin the level column by default too? Proposed: no, only the time.
