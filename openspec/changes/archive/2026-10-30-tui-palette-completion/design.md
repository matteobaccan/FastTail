## Context

`terminal_action` maps registry ids to terminal actions; `[recent_files]` is in `fasttail.ini` and the terminal already adds to it.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Window-only commands (zoom, always on top, wrap, Markdown view, timeline strip).

## Decisions

### D1. A window-only list
The terminal palette keeps a `window_only` list next to its mapping (the registry stays as it is); the test walks `actions::all()` and fails on a command that is neither mapped nor listed, so a new command cannot be forgotten.

## Risks / Trade-offs

- [Commands that make no sense in a terminal] → flagged window-only with a comment.

## Open Questions

- None.
