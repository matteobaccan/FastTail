## Context

`structured-fields` (0.14.0) gives each stream an optional `FieldParser` that scans a line
into `(key, value)` byte spans without allocation, and field filter terms in `FilterSpec`.
The engine keeps the visible lines (`filtered_lines`), a level byte and a timestamp per
line once scanned, and runs work on `ScanJob` workers with progress. The timeline
histogram already bins timestamps in buckets that double in width.

## Goals / Non-Goals

**Goals:**
- Top-N, numeric summary and breakdowns of one field or capture over the current view, on
  files of any size, with bounded memory.
- One click from a value to a filter.

**Non-Goals:**
- A query language, multi-field group-by, dashboards (see the proposal).

## Decisions

1. **Two sources, one pipeline.** A source is `Field(key)` (read with the stream's parser)
   or `Capture(Regex)` (first capture group, or the whole match without a group). Both yield
   an optional value slice per line; the rest of the pipeline does not know which. A regex
   source makes the feature useful before and without `structured-fields`' parsers.
2. **Scope = the current view.** The job iterates the visible lines (the `FilterSpec` of
   the stream, pattern filter and context lines excluded: context lines are not matches)
   and reads each through its own file handle. Only entry lines are counted; continuation
   lines have no fields. A filter change cancels and restarts the job.
3. **Bounded counter.** `HashMap<Box<str>, ValueStats>` with at most 100,000 distinct
   values (values longer than 256 bytes truncated with `…`); past the cap, new values go to
   an "other values" bucket and the distinct count is shown as `≥ 100,000`. `ValueStats`
   holds the count and 7 per-level counts (36 bytes). *Rejected:* count-min sketch /
   space-saving top-K — approximate counts for the common case (a few hundred distinct
   statuses or paths) to save memory that is not needed; the cap bounds the rare case.
4. **Numeric summary.** When ≥ 90 % of non-empty values parse as `f64` (after stripping a
   unit suffix `ms`, `s`, `us`, `%`, `B`, `KB`, `MB`), values are added to a log-scale
   histogram (buckets of 1 % relative width over 1e-9..1e15, plus zero and negatives
   mirrored: ≈ 7,000 `u32` = 28 KB) giving percentiles within 1 %, plus exact count, min,
   max and sum. *Rejected:* storing all values and sorting — unbounded memory on a 10 GB log.
5. **Time slices.** Width auto-picked so the view's span has at most 240 slices (from the
   series 1 s, 5 s, 15 s, 1 min, 5 min, 15 min, 1 h, 6 h, 1 day), or chosen by the user. The
   line chart shows the top 5 values' counts per slice (a `[u32; 5]` per slice, plus an
   "other" series), or p50 / p95 per slice for numeric sources (a small histogram per slice
   with 1 % buckets truncated to the values seen, capped at 240 × 2 KB). Needs the
   timestamp cache: opening a breakdown times the stream as the histogram does.
6. **Charts drawn with the egui painter.** Bars and polylines with hover tooltips, theme
   colours from the level palette and the accent. *Rejected:* `egui_plot` — a dependency
   for two chart types, and its interaction model (pan / zoom) is not needed.
7. **Click-to-filter.** A field source adds the include term `key=value` (quoted per the
   `structured-fields` rules when the value has spaces or operators); a regex source adds
   a regex include term: the source regex with its capture group replaced by the escaped
   value. `CTRL + click` adds an exclude term. If the 8 terms of that side are used, a
   notice says so and nothing is added.
8. **Incremental updates.** Appended visible lines are counted on the UI thread (bounded by
   the append batch); partial results of a running job are merged every 250 ms, so the
   table fills in while a large file is read.

Threads and memory: the job runs on a worker for every size (opened on demand); merging and
drawing on the UI thread. Memory per open Statistics tab: the counter (≤ 100,000 values,
≤ ~15 MB worst case with 128-byte values), the numeric histogram (28 KB), the slices
(≤ 240 × 2 KB); nothing per line, and nothing when the tab is closed. Growing files add
appended lines; truncation, rotation or rewrite reset and recompute. The job's file handle
uses the engine's sharing flags, so Windows writers are not blocked.

## Risks / Trade-offs

- [High-cardinality fields (request ids) fill the cap] → the cap and the `≥` mark; the tab
  suggests a different field when every value occurs once.
- [Unit stripping misreads a value] → only the listed suffixes, and only when 90 % of the
  values parse; otherwise the source is treated as text.
- [Stale charts on a fast-growing log] → merged every 250 ms, never more often.

## Open Questions

- Should the chosen source be saved per stream in sessions?
- Should the time-slice line chart also be offered for the whole stream (not a field),
  as a filter-aware complement of the timeline histogram?
