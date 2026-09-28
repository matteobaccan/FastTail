## Why

FastTail exports only plain text: "Export visible lines…" and "Export search matches…"
write the raw lines. To count errors per minute in a spreadsheet the user re-parses the
text by hand; to show a colleague "the red lines" they take a screenshot, because the
highlight colours are lost. LogFusion and LogViewPlus export CSV and HTML, BareTailPro
exports its search results; the post-0.12.0 competitor scan ranks the gap 18 (value Low,
effort S). Once `structured-fields` ships, the fields the user already sees as columns are
the obvious CSV columns.

## What Changes

- The export items become one **Export…** dialog per stream with three choices:
  **what** (visible lines, selected lines, search matches) and **format** (text, CSV,
  HTML). Text behaves exactly as today's export. The two existing menu items stay as
  shortcuts to the dialog with "text" preselected, so nothing moves for existing users.
- **CSV** (RFC 4180, UTF-8): columns `line`, `time`, `level`, `text` — the 1-based line
  number, the detected timestamp in ISO 8601 (empty without one), the detected level
  (empty without one) and the line text without ANSI escapes. On a stream with an active
  `structured-fields` parser the columns are `line`, then the **shown fields in view
  order**, then `message`. Options: separator comma, semicolon or tab; a UTF-8 BOM (on by
  default, so Excel reads accents); **formula protection** (on by default) that prefixes a
  `'` to a cell starting with `=`, `+`, `-`, `@`, tab or carriage return.
- **HTML**: one standalone file, no script and no external resource, with the line
  numbers and the lines drawn with the current theme and the colours FastTail shows:
  highlight rules (foreground, background, bold, italic, underline), quick labels, level
  colours, ANSI colours in render mode and the search hits. At most 200,000 lines; the
  dialog says so and exports the first 200,000.
- New `fasttail.ini` keys in `[general]`: `export_format` (`text`, `csv`, `html`; the last
  one used), `export_csv_separator` (`comma`, `semicolon`, `tab`), `export_csv_bom`
  (default `true`), `export_csv_protect` (default `true`).

Target release: **0.14.0** (structured logs and analysis), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **S**.

### Non-goals

- Excel `.xlsx`, JSON or XML export.
- Exporting the column view's layout (widths) or hidden fields into the CSV.
- Paging the HTML output or embedding a viewer with search; the file is for reading and
  attaching to a ticket.
- Exporting from the Find results tab (a possible follow-up; it holds hits of several
  streams).
- Changing what the text export writes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `selection-and-export`: new requirements Export Dialog, CSV Export and HTML Export.

## Impact

- `src/export.rs` (new): `ExportScope`, `ExportFormat`, CSV writer (quoting, BOM,
  formula protection), HTML writer (escaping, spans from the row style pipeline).
- `src/tail_engine.rs`: an iterator of export rows with line number, timestamp and level;
  selection scope.
- `src/ui/dock.rs`: dialog and the existing menu items; the span computation already used
  to paint a row (rules, labels, levels, ANSI, hits) exposed for the HTML writer.
- `src/config.rs`: the four keys.
- With `structured-fields`: the row field parser and the view's column order.
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: the writers are UI-free and can serve a `:export` command;
  the HTML colours come from the GUI theme, so the TUI would pass its own palette.
