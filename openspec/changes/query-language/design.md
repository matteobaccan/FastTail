## Context

After `structured-fields`, `boolean-filter-expressions` and `field-statistics`, FastTail
has a field scanner per stream, an expression evaluator over text and fields, and bounded
aggregators (value counter, log-scale percentile histogram). What is missing is a way to
chain them: select, extract, group, aggregate, sort.

## Goals / Non-Goals

**Goals:**
- Group-by and aggregation over the visible lines of one stream, with results in a table.
- Reuse the filter expression syntax and the aggregators, not a second implementation.
- Bounded memory on files of any size.

**Non-Goals:**
- SQL, joins, live queries, charts (see the proposal).

## Decisions

1. **Pipeline syntax, not SQL.** Stages joined by `|` (as in Loki LogQL, Splunk SPL,
   angle-grinder, PRQL). *Rejected: an SQL subset.* Embedding SQLite (as lnav does) adds a
   C dependency and ~1.5 MB to the binary, needs a virtual-table module over a file that
   is never held in memory, and makes every filter a second, different syntax from the
   stream's. A hand-written SQL subset is a larger grammar (`SELECT` lists, aliases,
   `GROUP BY` / `HAVING` / `ORDER BY` placement rules) for the same power, and users of a
   log viewer think "filter, then count": a pipeline reads in that order, and its first
   stage is exactly the filter expression they already type.
2. **Streaming then blocking.** `where`, `parse`, `fields` and `timeslice` are streaming
   (one line in, zero or one row out, no memory); `stats` and `sort` block (they consume
   everything first); `head` stops the scan early when no blocking stage precedes it. The
   executor is a vector of stage objects over a reused row buffer (`SmallVec` of value
   slices into the line plus owned parse captures).
3. **Aggregators.** `count`, `sum`, `avg`, `min`, `max` exact; `dc` exact up to 100,000
   distinct values then a lower bound; `p50`…`p99` from `field_stats`' log-scale histogram
   (1 % relative error); `first` / `last` by line order. Groups keyed by the `by` values,
   at most 100,000 groups (then the query stops with a "too many groups" error, rather than
   silently dropping).
4. **Types.** Values are text; numeric functions and numeric comparisons parse as `f64`
   and skip values that do not parse (counted in a per-query "skipped values" note).
   `ts` is the line's timestamp in milliseconds, shown formatted.
5. **Execution.** A `Query` scan job over the visible lines (stream `FilterSpec`), with
   progress and cancel; results sent once at the end for blocking pipelines and in batches
   for streaming ones. A query is a snapshot of the view at start; the tab says so and
   offers Run again.
6. **Errors.** Parsing reports the stage number and byte position; unknown functions and
   fields that no line has are errors only for functions (a missing field is an empty
   value).

Threads and memory: parsing on the UI thread; execution always on a worker. Memory: the
result rows (≤ 100,000) or the groups (≤ 100,000 × aggregator state, worst case ~50 MB
with several percentile aggregators; each percentile histogram is allocated sparsely),
nothing per line of the file. Growing files: not followed (snapshot). Truncation or
rotation during a run cancels it with a notice. File access through the job's own
handle with the engine's sharing flags (Windows writers not blocked).

## Risks / Trade-offs

- [Scope creep toward a database] → the stage list is closed in this change; new stages
  need their own change.
- [Group memory on high-cardinality keys] → the 100,000-group error and the sparse
  histograms.

## Open Questions

- Should query results be able to feed the Statistics charts?
- Should a query run across all open streams (with a `_stream` column)?
