## Why

In the column view of a wide structured log (twenty JSON fields, a CSV export), scrolling
right to read a field hides the timestamp and level, so the row loses its anchor. lnav's
request to keep the time column visible while scrolling sideways (#674) is near the top of
its reaction-sorted issues; spreadsheets call it freezing panes. Gap 25 (post-0.27.0 scan),
value medium, small.

## What Changes

- In the column view, a column header's menu gets **Pin column**: pinned columns stay at the
  left while the others scroll horizontally; **Unpin** releases it.
- The timestamp column is pinned by default when a parser has one (setting
  `pin_time_column=true`).
- Pinned columns are kept per stream in the workspace and sessions (`pinned=ts,level`).
- The terminal column view pins the same columns (sideways scroll keeps them).

Target release: **0.28.0** (proposed after the post-0.27.0 scan of 2026-10-28; planned when the maintainer agrees), per the release plan in `docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **S**.

### Non-goals

- Pinning columns at the right, or rows.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `structured-fields`: the requirements below are added.

## Impact

- `src/ui/dock.rs` column view: a fixed left part and a horizontally scrolled part sharing
  the row layout; header menu.
- `src/tui/app.rs` column drawing with a pinned prefix.
- `src/tail_engine.rs` / workspace / sessions: `pinned=`; `src/config.rs` `pin_time_column`.
- README, CHANGELOG, `docs/ui-design.md`, i18n.
