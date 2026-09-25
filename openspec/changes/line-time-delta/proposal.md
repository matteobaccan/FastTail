## Why

The question behind many log reads is "where did the time go?": which step of a request took two seconds, when a service stalled, how long a retry loop ran. With 0.9.0 FastTail reads each line's timestamp, but the user still subtracts `14:02:05.123` from `14:02:07.480` in their head. A per-line delta column makes gaps jump out, and a selection that reports its elapsed time answers "how long did this take" directly.

## What Changes

- An optional **Δt column** in the text view (normal and wrapped rows), toggled from the stream toolbar next to the line-number button and remembered in `fasttail.ini` like line numbers. For each row with a timestamp of its own it shows the time since the previous **visible** row (so it follows the active filters), e.g. `+0.125`, `+12.300`, `+4:05.120`, `+2:03:04`.
- **Anchor mode**: "Set time anchor here" in the row context menu switches the column to the time relative to that line (signed, so rows above it read negative); "Clear time anchor" returns to the previous-row delta. The anchor is per stream and not persisted.
- **Gap tint**: deltas of at least a configurable threshold (default 1 s, `time_delta_gap_ms`) are drawn in the accent colour.
- **Selection elapsed time**: with two or more rows selected, the stream status bar shows the time from the first to the last selected row (`Δ +2.357 · 14 rows`).
- Times are those of the timestamp cache and follow its rule: the clock the log printed, zones not applied, continuation lines inheriting their entry's time. Streams without usable timestamps show no column and the toolbar button explains why.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `log-intelligence`: adds the time delta column, the time anchor and the selection elapsed time.

## Impact

- `src/tail_engine.rs`: `time_anchor: Option<usize>` (cleared on truncation/rewrite like the selection), a helper giving the delta for a visible row from the timestamp cache and the previous visible row, and the selection span from the first and last selected rows.
- `src/timestamp.rs`: `format_delta(millis)`.
- `src/ui/dock.rs`: the column in `render_log_stream` and `render_wrapped_rows`, toolbar toggle, context-menu entries, status-bar label; the timestamp cache is filled in bounded chunks per frame (`fill_timestamps`) while the column is shown, rows not timed yet show `…`.
- `src/config.rs`: `show_time_delta`, `time_delta_gap_ms`.
- i18n: new keys in all 16 languages. README and CHANGELOG.
- No new dependency; memory unchanged (the cache already holds 8 bytes per timed line).
