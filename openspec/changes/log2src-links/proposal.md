## Why

lnav links a log message to the logging statement that wrote it (`log2src`); FastTail opens
source locations that are written in the line, not the place a plain message comes from.

## What Changes

- With source folders set (Settings), FastTail indexes the logging calls of Rust, Java,
  Python, Go, C# and JavaScript files (`log::info!`, `logger.info(`, `log.Printf(` ...) in the
  background, their format strings as patterns.
- *Go to the logging statement* (row menu, palette, `Ctrl+Shift+O` / `Alt+L`) opens the
  editor at the statement whose pattern matches the row's message; several matches are
  listed in the notice (the best one opens). Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.33.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low–Med**. Effort: **L**.

### Non-goals

- Parsing the languages fully (patterns over the format strings are enough).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `log-intelligence`: the requirement below is added.

## Impact

A new `src/log2src.rs` (regex-based extraction, no new crate), a background index, both interfaces; texts; docs.
