## Why

The time range filter answers "show me 14:02 to 14:05", but the user first has to know that 14:02 is when things went wrong. On a day-long log the level counters say there were 212 errors, not when. lnav's histogram view and the timeline bars of hosted log tools show the volume of lines per level over time, so a burst of errors or a gap in logging stands out and can be selected directly. FastTail already caches, per line, the detected level and (once the time controls are used) the effective timestamp; a histogram is a small aggregate over those two caches.

## What Changes

- A **timeline histogram** strip for a stream, toggled from the time range row: bars over the stream's time span, each bar stacked by level (ERROR/FATAL, WARN, INFO, DEBUG/TRACE, unknown) with the theme's level colours, plus an optional lane for the lines matching the current search.
- **Built incrementally from the timestamp and level caches**, in a bounded number of buckets (at most 2,048, widths doubling from one second) that merge in pairs as the span grows, so the cost is constant per line and the memory is tens of KB per stream. Opening the strip requests the timestamp cache; with the background scan it fills in while the log is being timed.
- **Click or drag on the strip sets the existing from/to time range** (and writes the bounds into the from/to fields), so the selection is the same window the user could have typed, combined with the other filters. The current window is shaded on the strip; the existing clear button removes it.
- Hovering a bar shows its time span and its counts per level.
- **Depends on `background-timestamp-scan`**: without it, opening the histogram on a large untimed log would time the whole file on the interface thread.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `log-intelligence`: new Timeline Histogram requirement.

## Impact

- New `src/time_histogram.rs`: `TimeHistogram { bucket_ms, first_bucket, counts: VecDeque<[u32; LogLevel::COUNT]>, untimed }` with `add(ts, level)`, `remove(ts, level)`, pair-merge on growth, reset.
- `src/tail_engine.rs`: a `histogram_len` watermark fed from `min(levels.len(), timestamps.len())` after either cache grows; lines subtracted before `truncate_timestamps` / `truncate_levels` drop them; reset on reload and pattern switch; accessor for the UI.
- `src/ui/dock.rs`: toggle next to `render_time_range`, the strip painter (resampling buckets to pixels, stacked levels, search lane, shaded window, hover tooltip), click/drag to `apply_time_range_text` with formatted bounds.
- `src/config.rs`: `timeline_histogram` (shown flag) and `timeline_search_lane` in `fasttail.ini`.
- `src/i18n.rs`: new keys in all 16 languages. README "Time range" section, CHANGELOG.
