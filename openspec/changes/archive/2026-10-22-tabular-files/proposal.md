## Why

Exports from databases, monitoring tools and spreadsheets arrive as CSV or TSV, often next
to the logs. FastTail shows them as raw lines; a quoted field holding a newline breaks one
record over several rows. lnav 0.15 and LogExpert's CSV columnizer show such files as
columns. FastTail already has a column view for structured logs (`structured-fields`).

## What Changes

- A **CSV / TSV parser** for the column view: detected from the extension (`.csv`,
  `.tsv`) and the first lines (consistent separator count among `,` `;` `\t` `|`), or
  chosen in the field parser selector.
- The **header row** is detected (non-numeric, distinct names) and gives the column names;
  without one the columns are `1`, `2`, ...
- **Quoted cells** follow RFC 4180: `"a,b"` is one cell, `""` an escaped quote, and a
  quoted cell spanning lines joins them into one row (shown with `⏎` where the newline was).
- Columns can be chosen, ordered and resized as in the column view; field filter terms,
  sorting by nothing (the file order stays), copy as shown and export work on the cells.
- The separator and the header choice are saved per stream (`csv_separator=`,
  `csv_header=`), older builds ignore them.
- **Column view in the terminal interface**, which has none today: for every field parser
  (JSON, logfmt, regex presets, CSV / TSV), toggled per stream from the stream bar, with
  a header row, the list of columns to show or hide, and the same saved
  `fields_view` / `fields_columns` keys as the window. It is what makes this change large.

Target release: **0.21.0** (re-planned by the maintainer on 2026-10-06, from 0.24.0, together with `structured-field-terms`, whose field filter terms it needs; about one large, two medium and three small changes per release) (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **medium**. Effort: **L** (M for the parser, plus the terminal column view).

### Non-goals

- Sorting rows by a column, editing cells, formulas, Excel files.
- Fixed-width columns (a regex parser covers them).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `structured-fields`: the CSV / TSV parser, header detection and quoted multi-line cells.
- `terminal-interface`: the column view in the terminal.

## Impact

- `src/fields.rs`: the delimited parser and its detection; record boundaries for quoted
  newlines.
- `src/tail_engine.rs`: rows that join physical lines (like the multiline grouping of
  stack traces) for quoted cells.
- `src/ui/dock.rs`, `src/tui/app.rs`: the parser in the selectors, the header row; a
  terminal column view (`src/tui/`, new module) for every parser.
- `src/session.rs`: `csv_separator=`, `csv_header=`.
- i18n, README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG, tests.
