## Why

An open question since 0.20.1: `PgUp` / `PgDn` held down move one page per key repeat;
on a 10-million-line file that is slow. Queued presses are already drawn once.

## What Changes

- Held `PgUp` / `PgDn` (presses closer than 120 ms, so key repeats over SSH count too) accelerate: after 1 s each press moves
  2 pages, after 2 s 4, after 3 s 8, back to 1 when released.
- `[general] page_acceleration=false` turns it off.

Target release: **0.31.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low**. Effort: **S**.

### Non-goals

- Acceleration of the arrow keys.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirement below is added.

## Impact

`src/tui/app.rs` page keys, `src/config.rs` key; `docs/tui.md`, CHANGELOG.
