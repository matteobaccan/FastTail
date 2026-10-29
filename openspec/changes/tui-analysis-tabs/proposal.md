## Why

The window has five analysis tabs that the terminal interface lacks: Field statistics,
the Query tab, Patterns, Explain a spike and Operations. The terminal already names their
dock panels but draws nothing in them, so a user on a server over SSH cannot count the
values of a field, run a query or see which kinds of lines a log holds. They are lists and
tables, which a terminal draws well; lnav has its statistics and query views in a TUI.
Section 7 of `docs/competitor-analysis.md` lists them as the largest gap.

## What Changes

- **Statistics** (`S` on a column of the `[Fields...]` list, the palette): the top values
  of a field or of a regex capture with count, share, a bar and per-level counts; the
  numeric summary (min, max, mean, percentiles, with units); Enter filters on the value,
  Ctrl+Enter excludes it; time slices drawn as a sparkline per value.
- **Query** (palette): a one-line editor for the pipeline (`where`, `parse`, `timeslice`,
  `fields`, `stats`, `sort`, `head`), the result as a scrollable table, sort by column,
  Enter on a row result goes to its line; the 20-query history shared with the window.
- **Patterns** (palette, `Ctrl+G`): the Drain templates with count, share and a volume
  sparkline; Enter shows only a pattern's entries, `x` hides it.
- **Explain** (palette, `E` in the time range dialog): the rates, levels, templates and
  field values that stand out in the chosen span, Enter goes to a template's first line.
- **Operations** (palette, from a field of the `[Fields...]` list): one row per id with
  start, duration, lines, worst level and a text Gantt bar; sort, filter, Enter goes to
  the first line.
- The tabs dock like the stream windows (split, tabs, floating) and are not saved in the
  layout, as in the window.

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Medium**. Effort: **L**.

### Non-goals

- Charts beyond sparklines and text bars.
- Editing a tab's settings that the window keeps only in its own panels (colours, sizes).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/` a module per tab (`stats.rs`, `query.rs`, `patterns.rs`, `explain.rs`,
  `operations.rs`) over the engine's existing jobs (`TailEngine::open_stats`,
  `run_query`, the pattern, spike and operation scans); `DockTab` panels drawn.
- `src/i18n_tui.rs` texts; `docs/tui.md`; README (terminal), CHANGELOG.
