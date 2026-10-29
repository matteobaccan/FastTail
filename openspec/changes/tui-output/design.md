## Context

`crate::export` writes text, CSV and HTML from an engine and a scope in steps;
`crate::bookmark_report` builds the Markdown; the window drives both from dialogs. The
search scope is `TailEngine::search_scope`.

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** A file picker beyond the existing path field with completion.

## Decisions

### D1. Same writers
The terminal calls the window's writers in the same steps, with a progress line in the
status bar and `Esc` to cancel.

### D2. Paths typed
The export path is a text field with Tab completion, prefilled with the stream's folder
and `name-filtered.csv`.

## Risks / Trade-offs

- [Large exports block the UI] → steps of a few milliseconds per frame, as in the window.

## Open Questions

- Put the results pane in its own dock panel instead of under the rows? Proposed: under the rows, as the window does.
