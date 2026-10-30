## Context

Copy as shown builds the rows' text; the bookmark report writes Markdown blocks.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Posting to a service.

## Decisions

### D1. One builder
The excerpt is built in `crate::export` from the selection, the stream name and the first and last timestamps.

## Risks / Trade-offs

- [Large selections] → capped at the clipboard limit (4 MB), as the report is.

## Open Questions

- Default flavour? Proposed: Markdown.
