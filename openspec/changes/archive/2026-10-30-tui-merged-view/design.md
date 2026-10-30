## Context

`crate::merged_stream` builds the merged index in the background and serves its rows; the window draws them with a source column. The terminal's dock already restores the saved view as a placeholder.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** A merged view of streams the terminal cannot open (none: every source opens in both).

## Decisions

### D1. Same merged engine
The terminal opens the same `MergedStream` the window uses; only the drawing (a tag column of 2 to 10 cells) is new.

## Risks / Trade-offs

- [Narrow terminals] → the source tag shrinks to its first letters, then a coloured mark.

## Open Questions

- Tag the sources by file name or by a letter? Proposed: the file name, cut to 10 cells.
