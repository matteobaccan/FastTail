## Why

Service logs interleave hundreds of requests, jobs or transactions, each tagged with an id
(`req=7f3a`, `"trace_id": …`, `[job-42]`). "Which requests were slow, which overlapped the
outage, which never finished" means filtering by one id at a time and subtracting
timestamps by hand. lnav 0.12 added a **timeline** view that draws each operation as a
bar from its first to its last line, and trace viewers (Jaeger, the Grafana trace view)
made the Gantt layout the expected answer for "what ran when". No desktop log viewer has
it; with `structured-fields` giving ids as fields, FastTail can.

## What Changes

- An **Operations** tab for a stream (stream menu "Operations…", column header menu
  "Operations by `trace_id`"). Its **id source** is a field of the stream's parser or a
  regex with one capture group (`req=(\w+)`), the same choice as `field-statistics`; an
  optional **name source** (a field, or the first line's message) labels each operation.
- One **row per operation id**: id, name, start and end (the first and last timestamps of
  its lines), duration, line count and worst level, next to a **Gantt bar** on a shared
  time axis, coloured by the worst level; an operation of one line is a tick.
- Sortable by start (default), duration, lines or worst level; a text box narrows by id or
  name, a minimum duration hides short ones. The time axis zooms with the wheel and pans
  with a drag; the stream's time window is shaded.
- Click an operation to go to its first line; "Filter to this operation" adds the field
  term `key=value` (or the regex include term for a regex source); "Set as time window"
  applies its span. Hover shows its first line.
- **Scope**: the lines the stream shows (its filters apply). Lines without an id are not
  part of any operation. Continuation lines belong to their entry's operation.
- Runs on a **worker** with progress; appended lines extend operations incrementally;
  truncation or rotation rebuilds. Limit **100,000 operations** (further ids are counted
  and reported, not drawn).
- "Copy as CSV" copies the table. Not persisted; no new key in `fasttail.ini`.

Target release: **0.15.0** (structured logs and analysis, continued; planned for 0.14.0, moved when 0.14.0 shipped early), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Parent / child spans and nesting (a trace tree): one flat row per id.
- Operations across several streams (possible once `merged-timeline-view` ships: the tab
  then works on the merged stream like any other).
- Detecting ids automatically, or pairing "start" / "end" messages with patterns.
- OTLP traces (that is the `otlp-receiver` change).

## Capabilities

### New Capabilities

- `operation-timeline`: the Operations tab, id and name sources, the table and Gantt
  bars, navigation and filtering from an operation, limits and incremental updates.

### Modified Capabilities

None.

## Impact

- `src/operations.rs` (new): `OpSource { Field, Regex }`, `OpStat { id, name, first_line,
  last_line, first_ts, last_ts, lines, worst_level }` in a hash map keyed by id with the
  100,000 cap; merge of partial results; append handling.
- `src/scan_job.rs`: `JobSpec::Operations` over the visible lines with the timestamp and
  level caches.
- `src/tail_engine.rs`: operations state per stream, fed by appends, reset on reload.
- `src/ui/operations_tab.rs` (new): virtualised table, time axis with zoom / pan, bars,
  CSV copy; menu entries in `src/ui/dock.rs`.
- With `structured-fields`: field extraction and `FieldTerm` for "Filter to this
  operation". Shares the source picker with `field-statistics` if that lands first.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
- **TUI (0.20.0, PR #132)**: `operations.rs` is UI-free; the TUI can draw the bars with
  block characters.
