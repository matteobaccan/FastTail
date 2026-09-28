## Why

With `structured-fields` a user can see `status` and `path` as columns and filter on
`status>=500`, but cannot answer the next questions: which paths fail most, which client
IPs send most requests, how the 5xx count moves over the afternoon, what the p95 of
`duration_ms` is. Today that means exporting the visible lines and running `awk | sort |
uniq -c` or a spreadsheet. angle-grinder and goaccess exist for exactly this, lnav answers
it with SQL, and LogViewPlus has dashboards and charts; the post-0.12.0 competitor scan
lists top-N and statistics of a field as gap 15 (value medium) and "patterns and
summaries" as a market signal.

## What Changes

- A **Statistics** tab for a stream, opened from the column header menu ("Statistics of
  `status`"), from the stream menu ("Statistics…") or from a cell's menu. Its **source** is
  a **field** of the stream's parser (`structured-fields`) or a **regex with one capture
  group**, which works on any stream, parser or not (`took (\d+)ms`).
- **Top values**: the most frequent values over the lines the stream shows (its current
  view, under every filter), with count, share and a horizontal **bar chart**; N = 20 by
  default, up to 1,000; distinct values counted exactly up to 100,000, then marked as a
  lower bound.
- **Numeric summary** when at least 90 % of the values read as numbers: count, min, max,
  mean, p50, p90, p95, p99 (percentiles from a log-scale histogram, within 1 % relative
  error).
- **Breakdowns**: counts per **level** for each top value, and counts per **time slice**
  (auto width, or 1 s / 1 min / 5 min / 1 h / 1 day) drawn as a **line chart** of the top 5
  values (or of the numeric p50 / p95 over time), on timed streams.
- **Click a value** to add a filter on it: a field term `key=value` on a stream with a
  parser, or, for a regex source, a regex include term matching that capture; `CTRL +
  click` adds an exclude instead.
- **Copy as CSV** copies the table (value, count, share, and the breakdown columns).
- **Recomputed** when the view changes (filters, appended lines are added incrementally),
  on a worker with progress for any size of file.
- The chosen source per stream is not persisted; the tab is not saved in the layout. No
  new key in `fasttail.ini`.

Target release: **0.14.0** (structured logs and analysis), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **medium**. Effort: **M (1–3 weeks)**.

### Non-goals

- Arbitrary aggregations, group-by several fields, joins: that is `query-language`.
- Dashboards with several charts, saved charts, or charts exported as images.
- Statistics across several streams in one table.
- Pie charts, heatmaps and scatter plots.

## Capabilities

### New Capabilities

- `field-statistics`: the Statistics tab, its sources, top values, numeric summary,
  breakdowns, charts, click-to-filter and CSV copy.

### Modified Capabilities

(none)

## Impact

- `src/field_stats.rs` (new): value counter (hash map with the 100,000 cap), log-scale
  numeric histogram, per-level and per-slice breakdowns, merge of partial results.
- `src/scan_job.rs`: `JobSpec::FieldStats { source, filter, slice }` over the visible lines,
  sending partial results.
- `src/tail_engine.rs`: statistics state per stream, incremental update on append, reset on
  reload.
- `src/ui/stats_tab.rs` (new): table, bar chart, line chart (drawn with egui painter, no
  plotting dependency), source picker, slice picker, CSV copy; column header and cell menu
  entries in `src/ui/dock.rs`.
- Depends on `structured-fields` (field extraction, `FieldTerm` for click-to-filter).
- `src/i18n.rs` (every language), README, CHANGELOG, tests.
