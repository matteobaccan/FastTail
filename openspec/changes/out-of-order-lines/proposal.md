## Why

When a clock jumps back (NTP correction, a container with a wrong zone, several writers
buffering differently), a line's timestamp is earlier than the one before it. The time
range filter and the histogram then quietly place those lines elsewhere, and the user
reading the log top to bottom does not notice. nerdlog 1.12 marks out-of-order records.

## What Changes

- A line whose timestamp is earlier than the previous timed line by more than a tolerance
  (1 second by default) is **marked out of order**: a `↶` in the gutter, with a tooltip
  giving the jump (`-00:05:12`).
- The stream bar shows `N out of order` when any is found; a click walks them (next /
  previous), as the level counters do.
- The timeline histogram marks the buckets that received out-of-order lines.
- The time range filter is unchanged (it keeps filtering by each line's own time); its
  popup says how many lines it placed out of order.
- The tolerance is a setting (`out_of_order_tolerance_ms`, 0 turns the marks off).
- The terminal interface shows the same mark in the gutter and the count in the bar.

Target release: **0.24.0** (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **low–medium**. Effort: **S (under a week)**.

### Non-goals

- Reordering the lines by time (the merged timeline view does that across files).
- Detecting gaps forward in time (the time delta column shows them).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `log-intelligence`: out-of-order timestamps marked, counted and walked.

## Impact

- `src/tail_engine.rs` / `src/timestamp.rs`: the timing pass compares each timestamp with
  the previous timed line and keeps a sorted list of out-of-order lines (with the jump).
- `src/ui/dock.rs`, `src/ui/timeline_strip.rs`, `src/tui/`: gutter mark, chip, walk,
  histogram marks; `src/config.rs`: the tolerance.
- i18n, README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG, tests.
