## Context

`TailEngine::time_delta_of`, `show_time_delta`, the time display of the window
(`TimeDisplay`) and its rewrite of the timestamp span are in the engine.

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** Per-line time zone detection beyond the window's.

## Decisions

### D1. Same cells
The Δt cell uses the window's text (`+1.2 s`) and threshold, right-aligned in 8 cells.

## Risks / Trade-offs

- [Narrow terminals] → the Δt column is the first to go below 60 columns.

## Open Questions

- None.
