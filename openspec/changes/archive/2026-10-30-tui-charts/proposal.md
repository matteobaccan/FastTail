## Why

The analysis views of the terminal (0.30.0) are tables; the window adds charts: the
Statistics chart per time slice and the timeline histogram above the rows. A terminal
draws sparklines well (the plotted field row since 0.26.1), so the last gap against the
window can close.

## What Changes

- **Volume row**: `Alt+V` shows a one-row sparkline of the stream's lines per time slice
  above the rows, the window's histogram in one row (errors in the error colour); a click
  on a cell goes to that time.
- **Statistics over time**: the Statistics view gets a sparkline column per value (its
  count per slice) when the stream is timed.
- **Patterns and Operations**: the Patterns view shows each pattern's volume sparkline.

Target release: **0.31.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Medium**. Effort: **M**.

### Non-goals

- Multi-row charts with axes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirement below is added.

## Impact

`src/tui/spark.rs` reused; `src/tui/analysis.rs`; the stream window's rows; texts, `docs/tui.md`.
