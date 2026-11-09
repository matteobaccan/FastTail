## Why

A check of the command registry against the terminal palette shows commands the terminal
could run but does not list: select all, play / pause all, line numbers, auto-update, save
session, save as default session, remove bookmark, open a folder pattern, and the recent
files (open one, clear the list).

## What Changes

- The palette lists *Select all* (`Ctrl+A`), *Play all* / *Pause all*, *Line numbers*,
  *Auto-update*, *Save session*, *Save as default session*, *Remove bookmark*, *Open folder
  pattern…* and *Recent files…* (a list: `Enter` opens, `Delete` removes, *Clear recent
  files*), each with its terminal key.
- A test fails when a registry command that is not window-only has no terminal entry.

Target release: **0.31.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Medium**. Effort: **S**.

### Non-goals

- Window-only commands (zoom, always on top, wrap, Markdown view, timeline strip).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirement below is added.

## Impact

`src/tui/palette.rs`, `src/tui/app.rs`, a recent files list; texts, `docs/tui.md`, CHANGELOG.
