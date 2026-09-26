## Context

`TailEngine` holds two per-line prefix caches: `levels: Vec<u8>` (always filled — synchronously up to 16 MB, by a `Levels` job above — with running `level_counts`) and `timestamps: Vec<i64>` (filled on demand when the time controls are used; `NO_TIMESTAMP` before the first timed line; continuation lines inherit the entry's time). Both are truncated from the first changed line on rewrite. The time range is `time_from` / `time_to` set through `apply_time_range_text(from_text, to_text)`, which parses what the fields hold; `format_millis` prints `YYYY-MM-DD HH:MM:SS` and `end_of_typed_time` makes a "to" value with seconds cover that whole second. The `background-timestamp-scan` change (0.9.1) moves the first timing of a large file to a worker and applies a window typed during it once timing completes.

## Goals / Non-Goals

**Goals:** see the volume of lines per level over the whole log at a glance; select a window with the mouse; constant work per line and bounded memory; no whole-file pass on the interface thread.

**Non-Goals:** zooming or panning inside the histogram (selecting a window and reopening is the zoom); per-field or per-regex series beyond the current search; histograms across several streams (that belongs with `merged-timeline-view`); clock-skew or zone correction (times stay on the clock the log printed).

## Decisions

- **Depends on `background-timestamp-scan`.** Opening the strip calls `request_timeline()`, a public wrapper over the private `request_timestamps()` that does nothing once the cache is complete or a timing is already wanted (so the strip can call it every frame). Without the background scan that call would time a multi-GB file on the interface thread, which is exactly the 0.9.0 known issue; this change is scheduled after 0.9.1 for that reason.

- **Incremental aggregate, not a scan per frame.** A `TimeHistogram` keeps per-level counts in buckets numbered absolutely (`ts.div_euclid(bucket_ms)`), `first_bucket` plus a `VecDeque` so a line earlier than the first bucket (an out-of-order log) extends to the left. The engine feeds it every line in `histogram_len..min(levels.len(), timestamps.len())` whenever either cache grows — after a sync pass, a drained `Levels` or `Timestamps` batch, or an append — so each line is added once, in the work that already produced its level and time. Lines with `NO_TIMESTAMP` are counted as untimed and not placed. *Alternative:* recompute from the caches when the strip is drawn. Rejected: 500 million lines are 4.5 GB of cache to read per recomputation.

- **At most 2,048 buckets, widths doubling from one second.** When the span would need more than 2,048 buckets, the width doubles and adjacent buckets merge (`bucket >> 1` with absolute numbering, so alignment is automatic). One second as the floor keeps every bucket boundary on a whole second, which is what the time fields can express. 2,048 buckets × 7 levels × `u32` ≈ 57 KB per stream. *Alternative:* a "nice" ladder (1 s, 5 s, 15 s, 1 min, ...). Rejected: non-integer ratios cannot be merged without the per-line data.

- **Removal on truncation.** Before `truncate_timestamps` / `truncate_levels` drop lines at or above `keep` while `keep < histogram_len`, the engine subtracts those lines from their buckets (the time and level are still in the caches at that point), then lowers `histogram_len`. The bucket width never shrinks back except on a full reset (`keep == 0`, reload, pattern switch), which empties the histogram.

- **Counts all lines, ignoring the filters.** The strip is the tool for choosing a window, so it must show what lies outside the current window and filters; the current window is drawn as a shaded band over it. *Alternative:* histogram of the visible lines. Rejected: after the first selection the strip would show only the selected span, and a filter change would need a full recomputation.

- **Search lane from the existing hit list.** When the search lane is on and a query is active, a thin lane above the bars marks, per pixel, the buckets containing hits: each hit's line is looked up in the timestamp cache. The lane is rebuilt only when the hit list changes (keyed by the search generation counter from `search-results-pane`, the histogram generation and the buffer generation), which costs one lookup per stored hit. Hits are computed over the visible lines, so with a window set they only appear inside it; the tooltip says so.

- **Resampled to the strip width.** The strip spans the stream's width, about 56 px high, above the rows. Buckets are resampled to pixel columns (a column sums the buckets it covers, or a bucket spans several columns), stacked bottom-up ERROR/FATAL, WARN, INFO, DEBUG/TRACE, unknown, in the theme's level colours, with a linear scale to the tallest column and its count as a label. While timing is still running the strip grows as batches arrive and shows the timing progress. The strip shows the same "no usable timestamps" hint as the time fields when `timestamps_usable()` is false.

- **Selection writes the time fields.** A click selects one column's span, a drag the span between the columns where it started and ended. The start bucket's first second is written into the "from" field and the end bucket's last second (`bucket end − 1 s`) into the "to" field, both as `format_millis`, and applied through `apply_time_range_text`; since the "to" text carries seconds, `end_of_typed_time` extends it to the end of that second, so the window covers exactly the selected buckets. Everything else — combination with include/exclude/level filters, the clear button, the status-bar span, a window pending while timing runs — is the existing time range behaviour. Hovering a column shows its time span and counts per level.

- **Preferences.** The strip's shown state (`timeline_histogram`) and the search lane (`timeline_search_lane`) are global preferences in `fasttail.ini`; the histogram itself is not persisted.

## Risks / Trade-offs

- [One mis-parsed outlier (a 1970 epoch, a year-rollover syslog line) stretches the span and squeezes the real data into a few columns] → the tooltip shows the span; a percentile-trimmed view is a possible follow-up, kept out of this change.
- [Bars at one-second floor resolution cannot select sub-second windows] → the typed fields still accept seconds; sub-second selection is not a goal.
- [Keeping the histogram consistent across truncation, resumed scans and appends adds engine bookkeeping] → covered by tests comparing the incremental histogram with one rebuilt from the caches after each of those events.
- [Opening the strip on a large log starts a timestamp scan that competes with other scans] → the priority rules of `background-timestamp-scan` apply: with no window waiting, a filter or search request preempts it and it resumes afterwards.
