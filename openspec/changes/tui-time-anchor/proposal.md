## Why

The window's time delta column can measure every row from a chosen anchor line (*Set
time anchor here*); the terminal draws the column since 0.29.0 but cannot set the anchor,
and does not show the elapsed time of a selection.

## What Changes

- `@` sets the time anchor on the cursor row (again: clears it); the Δt cells read `-0.500`,
  `+1.250` from it and the anchor row shows `@`.
- A selection of several rows shows its elapsed time in the status bar
  (`Δ +2.357 · 14 rows`), as the window does.

Target release: **0.31.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low–Med**. Effort: **S**.

### Non-goals

- Several anchors.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirement below is added.

## Impact

`src/tui/app.rs`, keys, palette (`TimeAnchorSet` / `TimeAnchorClear`); texts, `docs/tui.md`.
