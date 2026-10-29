## Why

Compare lines and the scratchpad are two of the window's working tools during an
incident: diff a good and a bad request, collect the lines that matter with references.
The terminal names a Compare panel but draws nothing, and has no scratchpad.

## What Changes

- **Compare** (palette, `=` on two selected rows, or mark then compare): the two lines or
  regions side by side, differing tokens highlighted, the ignore toggles of the window
  (timestamps, numbers, ids, whitespace, case, as JSON), `]` / `[` to walk the changes,
  copy as unified diff.
- **Scratchpad** (palette, `Ctrl+N` to send the selected rows): the session's pad as a
  dock panel, editable, with the `── app.log:1204 ──` references and Enter on one to go
  to its line; saved beside the session as in the window.

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Low–Med**. Effort: **S**.

### Non-goals

- Three-way compare.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/` compare and scratchpad panels over `crate::compare` and the scratchpad file
  of the session; texts, `docs/tui.md`, README, CHANGELOG.
