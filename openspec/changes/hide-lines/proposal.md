## Why

Some lines are noise only once (a burst of retries, a test run) and do not deserve an
exclude term: their text is too generic to write one, or an exclude would also hide the
lines that matter. LogExpert 1.50 lets the user hide selected lines by hand (#338).
FastTail can only exclude by text.

## What Changes

- **Hide lines**: in the stream's context menu, the palette and `Ctrl+H` (`H` in the
  terminal interface), the selected rows (or the cursor row) are hidden from the view.
- Hidden lines are kept as **line ranges** (`120-180`, `4096`), not as text, so hiding a
  range of a million lines costs one entry.
- The stream bar shows `N hidden` while some are; a click opens the list of hidden ranges
  with **Show** per range and **Show all**.
- Hidden lines behave like lines removed by a filter: search, the overview strip,
  navigation, copy and export skip them; **Show in context** shows them (dimmed) like the
  other filtered lines.
- The ranges are saved per stream (`hidden=120-180,4096`) in `fasttail.ini` and sessions;
  older builds ignore the key. A truncation, rotation or rewrite of the file clears them,
  since line numbers no longer match.

Target release: **0.24.0** (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **medium**. Effort: **S (under a week)**.

### Non-goals

- Hiding by pattern (that is the exclude filter) or "hide all lines like this one"
  (the pattern grouping change covers similar lines).
- Hiding columns or parts of a line.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `filters-and-highlighting`: hidden line ranges as one more filter of the view.

## Impact

- `src/tail_engine.rs`: a sorted list of hidden ranges applied with the filters (the
  filtered line index skips them; a background filter scan honours them).
- `src/ui/dock.rs`: menu entry, the `N hidden` chip and its popup; `src/actions.rs`: the
  action; `src/tui/app.rs` and `src/tui/keys.rs`: `H`, the bar chip and a small dialog.
- `src/session.rs` / `src/workspace.rs`: `hidden=` per stream.
- `src/i18n.rs`, `src/i18n_tui.rs`, README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG.
