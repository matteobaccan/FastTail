## Context

Since 0.9.0 the engine keeps `timestamps: Vec<i64>`, a prefix of the line index holding each line's effective timestamp in milliseconds (its own, or inherited from the entry above; `NO_TIMESTAMP` before the first timed line). It is filled lazily by `fill_timestamps`, which times at most `TIMESTAMP_FILL_BUDGET` lines per call, and `ensure_timestamps` loops it to completion for the time range and go-to-time. Timestamps are compared on the clock the log printed: zone suffixes are read past and not applied, epoch values are read as UTC. `timestamps_usable()` tells whether enough lines carry a recognised timestamp. The view maps visible rows to lines through `filtered_lines` (`get_actual_line_idx`); the selection is a `BTreeSet<usize>` of line indices plus `selection_all`. Line numbers are a global toggle (`show_line_numbers`) drawn as a gutter.

## Goals / Non-Goals

**Goals:**
- See the time between consecutive visible entries at a glance, filters respected.
- Measure the time from one line to others (anchor) and across a selection.
- No UI stall on large files, no additional per-line memory.

**Non-Goals:**
- Zone or clock-skew correction; mixing files (that is the merged view's concern).
- Deltas in HEX or Markdown view, or in exports.
- Statistics (histograms, slowest gaps list).

## Decisions

### D1. Delta to the previous visible row, shown only on rows with their own timestamp
For visible row `r` showing line `l`, with `p` the line of row `r-1`: `delta = ts[l] - ts[p]`. Because continuation lines inherit their entry's time, `ts[p]` is the time of the entry the previous row belongs to, which is what the user means by "previous". The value is displayed only when line `l` carries a timestamp of its own — determined by running `detect_timestamp` on the row text already read for display (64 bytes, on-screen rows only) — so stack-trace lines stay blank instead of repeating `+0.000`. The first visible row, and rows whose own or previous time is `NO_TIMESTAMP`, are blank.

Using visible rows (not file order) is deliberate: with the filter `payment`, the column answers "time between payment lines", which is the reason to filter. With no filter the two are the same.

*Alternative:* delta to the previous line in the file, whatever the filter — rejected; the hidden lines' times would appear as unexplained gaps between the visible rows.

### D2. Anchor mode
"Set time anchor here" (row context menu) stores the line index in `time_anchor`; while it is set the column shows `ts[l] - ts[anchor]`, signed, for every row with its own timestamp, and the anchor row shows `⚓`. "Clear time anchor" (context menu, or setting the anchor on the same row again) returns to D1. The anchor stays valid when filters change (it is a line index) even if the anchor row is hidden; it is cleared on truncation or rewrite with the rest of the per-line state. Not persisted: it is a measuring tool for one reading session.

### D3. Filling the cache without a stall
While the column is visible on a stream whose cache is incomplete, the frame calls `fill_timestamps` once (one bounded chunk) and requests a repaint until the cache covers the rows on screen; rows past the timed prefix show `…`. The column never calls `ensure_timestamps`, so turning it on for a 20 GB log does not freeze the interface (unlike the first use of the time range, which the stream-engine spec documents as synchronous). A jump to the end of an untimed large file therefore shows `…` for a while; this is visible and honest.

`timestamps_usable()` needs a sample to judge, so the column is shown once the rate is known and `>= MIN_TIMESTAMP_RATE`; on a stream below it the column is hidden and the toolbar button's tooltip says the stream has no usable timestamps.

### D4. Format
`timestamp::format_delta(ms)` with a sign always shown and millisecond precision where it matters:
- `|d| < 60 s` → `+12.300`
- `< 1 h` → `+4:05.120`
- `< 1 day` → `+2:03:04`
- otherwise → `+3d 04:05`

The column has a fixed width of 10 monospace characters, right-aligned, so rows do not shift. Negative values (out-of-order lines, rows above the anchor) use `-`. Rows at or above `time_delta_gap_ms` (default 1000, 0 disables, Settings and `fasttail.ini`) are drawn in the theme's accent colour; the threshold applies to the previous-row mode only, not to anchor mode where large values are expected.

### D5. Selection elapsed time
With two or more selected rows the stream status bar shows `Δ <format_delta(ts[last] - ts[first])> · N rows`, where `first`/`last` are the lowest and highest selected line indices (`BTreeSet` ends, O(1)); with `selection_all` they are the first and last visible rows. Hidden when either end has no timestamp or the stream has no usable timestamps. On an out-of-order log the value can be negative, which is shown as is rather than hidden, because it is informative.

### D6. Global toggle like line numbers
The column is toggled with a `Δt` button beside `# 123` in the stream toolbar and stored as `show_time_delta` in `fasttail.ini`, mirroring `show_line_numbers`. A per-stream toggle was considered, but the button already hides itself where it cannot work (D3), and a global setting keeps sessions and the workspace format unchanged.

## Risks / Trade-offs

- [Zones are not applied: a log that switches offset (DST, mixed `Z` and local) shows a one-hour jump] → documented; consistent with the time range filter.
- [Epoch-stamped lines mixed with local-clock lines produce large deltas] → same rule, documented.
- [Second-resolution logs show `+0.000` and `+1.000` only] → correct for the data; the column cannot invent precision.
- [Extra `detect_timestamp` per on-screen row] → bounded by the number of rows on screen (tens), 64 bytes each, far below layout cost.
