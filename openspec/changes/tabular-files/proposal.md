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

Target release: **0.24.0** (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **medium**. Effort: **M (1–3 weeks)**.

### Non-goals

- Sorting rows by a column, editing cells, formulas, Excel files.
- Fixed-width columns (a regex parser covers them).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `structured-fields`: the CSV / TSV parser, header detection and quoted multi-line cells.

## Impact

- `src/fields.rs`: the delimited parser and its detection; record boundaries for quoted
  newlines.
- `src/tail_engine.rs`: rows that join physical lines (like the multiline grouping of
  stack traces) for quoted cells.
- `src/ui/dock.rs`, `src/tui/app.rs`: the parser in the selectors, the header row.
- `src/session.rs`: `csv_separator=`, `csv_header=`.
- i18n, README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG, tests.
