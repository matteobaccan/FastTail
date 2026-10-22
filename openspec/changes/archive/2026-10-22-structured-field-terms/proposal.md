## Why

`structured-fields` shipped in 0.14.0 with its first phase: the field parsers (JSON,
logfmt, regex, Apache, syslog) with detection, and the column view. The rest of that change
did not make the release and continues here: filtering on a field (`status>=500`,
`level=error|fatal`), which is the point of parsing fields for most users; the level and
time read from the fields, without which the time range, the histogram and the time delta
column do not work on a JSON log whose time is inside the object; search and rule colours
inside the cells, "Copy as shown" as columns, and a wrapping last column.

## What Changes

- **Field filter terms** in the existing include / exclude terms on streams with an active
  parser: `key=value`, `key!=value`, `key~=text`, `key>n`, `key>=n`, `key<n`, `key<=n`,
  with `|` for alternative values; a term in double quotes stays literal text; on a stream
  without a parser every term is text, as today. Field terms are marked in the term rows.
- **Level and timestamp from fields** (`level`, `lvl`, `severity`, `log.level`; `ts`,
  `time`, `timestamp`, `@timestamp`, `t`) for the level colours and filter, the time
  range, the histogram and the time delta column, on the synchronous and background paths.
- **Column view**: search tints and rule spans drawn in the cells their bytes fall in,
  "Copy as shown" as tab-separated cells, the character selection kept inside one cell
  (from `partial-line-selection` task 2.6), and the last column wrapping in wrap mode
  (0.14.0 keeps one row per line).

Target release: **0.21.0** (structured logs and analysis, continued), per the release plan
in `docs/competitor-analysis.md` section 8. Effort: **M**.

### Non-goals

As in `structured-fields` (archived with 0.14.0): no query language (see
`boolean-filter-expressions`), no sorting or statistics of a field (see
`field-statistics`), no other formats (see `custom-log-formats`).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `structured-fields`: adds Field Filter Terms, Level and Timestamp from Fields, and
  Search, Rules, Copy and Export with Fields; the Column View wraps its last column.
- `filters-and-highlighting`: Combined Filter Terms admits field terms on a stream with an
  active field parser.

## Impact

- `src/fields.rs`: `FieldTerm` (key, operator, values) and its evaluation.
- `src/scan_job.rs`: `FilterTerm.field`, `FilterSpec.fields`; `JobSpec::Levels` and
  `JobSpec::Timestamps` carry the parser.
- `src/tail_engine.rs`: filters rebuilt when the parser changes; level and timestamp caches
  reset without rebuilding the index; global terms compiled per stream.
- `src/log_level.rs`, `src/timestamp.rs`: level and time from a field value.
- `src/ui/dock.rs`, `src/ui/filters_window.rs`: `ƒ` badge on field terms; cell spans,
  copy as shown, cell-bound selection, wrapped last column.
- i18n (16 languages), help, README, CHANGELOG, tests.
