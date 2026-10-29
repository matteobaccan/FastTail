## Why

Two small inconsistencies remain in the terminal: Settings offers "Time delta column in
new streams" but the terminal never draws the column, and the terminal cannot show times
in UTC or local time as the window can. The listener prompt's hint also leaves out
`otlp://`, which works.

## What Changes

- The **Δt column** in the terminal rows (the gap to the previous line, the window's gap
  threshold colouring), toggled per stream (`Alt+D`) and by the existing setting.
- **Time display** Written / UTC / Local (palette, a chip in the stream bar), rewriting the
  line's timestamp as the window does.
- The listener prompt's hint lists `otlp://`.

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Low**. Effort: **S**.

### Non-goals

- Per-line time zone detection beyond the window's.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/app.rs` the row renderer and a chip; texts, `docs/tui.md`, CHANGELOG.
