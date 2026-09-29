## Why

More and more logs are structured: JSON lines from containers and cloud services, logfmt
(`ts=… level=warn msg="…" status=503`) from Go services, access logs with a fixed layout.
FastTail shows them as long text rows. The user cannot line up `status` across rows, hide
the twenty fields they do not care about, or filter on a field: `503` as a text term also
matches a `duration_ms=503` or a request id, and "status 500 or above" cannot be written
at all. A JSON log's timestamp is inside the object, so the time range, the histogram and
the time delta column do not work on it. Structured fields are the top request across the
category (lnav, klogg #510, LogExpert #197; LogExpert, LogViewPlus, hl, Seq and Loki all
have them) and the first gap of the post-0.12.0 competitor scan.

## What Changes

- A per-stream **field parser**: **JSON** (one object per line, nested objects flattened
  as `a.b`), **logfmt** (`key=value`, quoted values) or **regex** (a user pattern with
  named groups, plus built-in presets for the Apache / nginx combined format and
  RFC 3164 syslog). By default it is **detected** from the first 200 lines; the user can
  pick another one or force **off**. The detected parser is shown as a chip in the stream
  bar.
- A per-stream **column view** (`▦ Columns`), off by default: the text view stays the
  default. Each field becomes a column; fields can be **shown, hidden and reordered**
  (drag, or the column menu), widths can be changed, and a **message** column holds the
  rest of the line. Lines that do not parse, and stack-trace continuation lines, are shown
  across the columns. The layout is **saved per stream**.
- **Field filter terms** in the existing include / exclude terms, on streams with an
  active parser: `key=value`, `key!=value`, `key~=text` (contains; regex with `.*` on),
  `key>n`, `key>=n`, `key<n`, `key<=n` (numeric), with `|` for alternative values
  (`level=error|fatal`). Terms still combine as today (includes ANDed, excludes ORed);
  a term in double quotes stays literal text. On a stream without a parser every term is
  text, as today, so nothing changes for existing users.
- **Level and timestamp from fields**: when the parser finds a level field (`level`,
  `lvl`, `severity`, `log.level`) or a time field (`ts`, `time`, `timestamp`,
  `@timestamp`, `t`), the level colouring, the `≥ level` filter, the time range, the
  histogram and the time delta column use it.
- **Large files**: nothing is stored per field. Fields are parsed **on demand for the rows
  drawn** (a small cache of at most 1,024 rows); a field filter is evaluated by the
  existing filter scan (on a worker above 16 MB), which parses each line as it passes;
  column discovery reads a bounded sample.
- Search, highlight rules, bookmarks, copy and export keep working on the **line text**;
  in the column view search hits and rule spans are drawn in the cell they fall in, and
  "Copy as shown" copies the shown columns separated by tabs.
- New keys per stream in the workspace and session files: `fields_parser`,
  `fields_regex`, `fields_view`, `fields_columns`, `fields_width.<field>` (see the design).
  No new key in the global `fasttail.ini` settings.

Target release: **0.14.0** (structured logs and analysis), per the release plan in
`docs/competitor-analysis.md` section 8. Effort: **L**.
Shipped in 0.14.0: the parsers with detection and the column view. Field filter terms,
level and timestamp from fields, and the column view's cell spans, copy as shown and
wrapping moved to `structured-field-terms` (0.15.0), which holds their requirements.

### Non-goals

- A query language with parentheses, `AND` / `OR` / `NOT` across fields, or SQL: OR is
  written as value alternatives, NOT as exclude terms or `!=` (see the design).
- Sorting rows by a column, group-by, top-N or charts of a field (a later "field
  statistics" change can build on the parser).
- Other formats: CSV / TSV with a header, XML, CLEF, Log4j XML, GELF. The parser interface
  allows them later.
- A persistent per-line field index or a field cache on disk.
- Field-scoped highlight rules (a rule matching only inside one field).
- Fields in the HEX and rendered Markdown views and in the Find results tab (it keeps
  showing line text; its search honours field terms through each stream's filter).

## Capabilities

### New Capabilities

- `structured-fields`: field parsers and their detection, the column view, field filter
  terms, level and timestamp from fields, fields on large and growing files, and how
  search, highlight rules, copy and export behave in the column view.

### Modified Capabilities

None in this change: the change to Combined Filter Terms (`filters-and-highlighting`)
moved to `structured-field-terms` with the field terms.

## Impact

- `src/fields.rs` (new): `FieldParser { Json, Logfmt, Regex }`, zero-allocation scanners
  that yield `(key, value)` byte spans, detection over a sample, `FieldTerm` (key,
  operator, values) and its evaluation.
- `src/scan_job.rs`: `FilterTerm` gains an optional `FieldTerm`; `FilterSpec` gains
  `fields: Option<Arc<FieldParser>>`; `JobSpec::Levels` and `JobSpec::Timestamps` accept
  the parser for level and time fields.
- `src/tail_engine.rs`: parser per engine, detection on open and when an empty stream
  first holds 200 lines, the row field cache, filter rebuild when the parser changes.
- `src/log_level.rs`, `src/timestamp.rs`: level and time from a field value.
- `src/ui/dock.rs`: parser chip and menu, column view (header, drag reorder, widths,
  column menu), cell drawing with search and rule spans, "Copy as shown" as TSV.
- `src/session.rs` / `src/config.rs`: the `fields_*` keys in `StreamEntry`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests, a benchmark in
  `benches/` for the JSON and logfmt scanners.
