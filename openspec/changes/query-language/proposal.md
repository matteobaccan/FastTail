## Why

Filters answer "which lines", `field-statistics` answers "which values of one field". Some
questions need more: "error count and p95 latency per endpoint, slowest first", "requests
per client IP per minute where status ≥ 500", "the 10 users with most failed logins, with
their first and last attempt". lnav answers these with SQLite (and PRQL), LogViewPlus with
a SQL engine over parsed fields, Loki with LogQL pipelines. In FastTail the user exports
lines and leaves the tool. This is the deepest analysis feature in the category and the
least often needed; the post-0.12.0 scan does not rank it, and it only makes sense once
fields (`structured-fields`) and single-field statistics (`field-statistics`) exist.

## What Changes

- A **Query** tab for a stream (stream menu "Query…"), with a query box, **Run** / **Stop**
  and a result **table**.
- **Pipeline syntax** (decided against SQL, see the design): a sequence of stages joined
  by `|`, read left to right, starting from the lines the stream shows:
  - `where <expr>`: the `boolean-filter-expressions` syntax (text, regex and field
    operands, `AND` / `OR` / `NOT`);
  - `parse /regex with (?P<name>…)/`: adds fields from named groups;
  - `fields a, b, c`: keeps and orders columns (`line`, `ts`, `level`, `text` are built in);
  - `stats count(), sum(x), avg(x), min(x), max(x), p50(x) … p99(x), dc(x), first(x),
    last(x) by k1, k2`;
  - `timeslice 1m` (adds `_slice` from the timestamp, usable in `by`);
  - `sort [-]col, …`, `head N`.
- Example: `where status>=500 | stats count() as errors, p95(duration_ms) by path | sort -errors | head 10`.
- **Results**: a virtualized table, sortable by clicking a header, "Copy as CSV"; for row
  results (no `stats`), double-click jumps to the line in the stream.
- **Runs on a worker** over the stream's visible lines, with progress and Stop; limits:
  100,000 result rows, 100,000 groups, 64 KB query text, 16 stages.
- **Errors** name the stage and the position; the last 20 queries are kept in a history
  (`fasttail.ini`, `[query] history.1`…`history.20`, values through
  `filter_preset::ini_value`).

Target release: **0.24.0** (re-planned by the maintainer on 2026-10-06, from 0.22.0: about one large, two medium and three small changes per release) (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **low**. Effort: **L (more than 3 weeks)**.

### Non-goals

- SQL, joins between streams, sub-queries, window functions, user-defined functions.
- Live queries that update as the file grows (a query is a snapshot; Run again refreshes).
- Charts of query results (the `field-statistics` charts cover the common case).
- Writing results back into the stream view as a filter.
- AI that writes queries.

## Capabilities

### New Capabilities

- `query-language`: the Query tab, the pipeline syntax and stages, execution limits,
  results table and history.

### Modified Capabilities

(none)

## Impact

- `src/query/` (new): lexer and parser for stages, reusing `filter_expr` for `where`;
  stage executors (streaming `where` / `parse` / `fields` / `timeslice`, blocking `stats`,
  `sort`, `head`); aggregators reusing `field_stats`' numeric histogram for percentiles.
- `src/scan_job.rs`: `JobSpec::Query` streaming visible lines into the pipeline.
- `src/ui/query_tab.rs` (new): editor, history, table, CSV copy, jump to line.
- `src/config.rs`: `[query]` history.
- Depends on `structured-fields`, `boolean-filter-expressions` and `field-statistics`.
- `src/i18n.rs` (every language), README, CHANGELOG, tests.
