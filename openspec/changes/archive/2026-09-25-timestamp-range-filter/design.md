## Context

`is_line_visible` is the single visibility gate; `filtered_lines` is rebuilt when filters change. Timestamps are monotonic in the vast majority of logs, which makes go-to-time a binary search over the cache.

## Goals / Non-Goals

**Goals:** range filtering and time jumps without user-supplied formats, cheap detection, correct handling of multi-line entries.

**Non-Goals:** user-defined timestamp formats (later); time zone conversion (times are compared on the clock the log printed: a zone suffix is read past and not applied, epoch values are read as UTC); relative "last 5 minutes" live windows (later).

## Decisions

- **Fixed-format parser, first 64 bytes**, trying formats in order of frequency; the first successful parse wins and the format id is remembered per stream to try it first next time.
- **Cache as `Vec<i64>` millis, `i64::MIN` for none**, 8 bytes per line, built when the range filter or the time jump is first used and then kept up to date on append; reset from the first changed line on truncation or rewrite. Continuation lines inherit the previous timestamp so stack traces stay inside the range; lines before the first timestamped line have none and are hidden while a window is set. The first build currently runs synchronously on the interface thread (in bounded passes of 200,000 lines, looped until done); moving it to the scan worker is future work. The status-bar span reads the first and last visible lines and does not need the cache.
- **Range inputs accept `HH:MM[:SS]` (on the day of the stream's first timestamp, not today), `YYYY-MM-DD HH:MM[:SS]` with a space or `T`, or any timestamp the line parser reads**; an empty side is open-ended. The "to" side covers the whole unit typed (minute or second).
- **Go-to-time** treats go-to input containing `:` as a time. It uses `partition_point` over the cache when timestamps are non-decreasing; a stream is marked unordered as soon as one line is earlier than the line before it (not a percentage threshold), and then falls back to a linear scan.

## Risks / Trade-offs

- [Formats not covered] → the range controls are disabled with a hint when fewer than 50% of the timed lines carry a timestamp of their own (measured once at least 200 lines are timed).
- [8 bytes per line] → 100 MB for 12 million lines, only when the feature is used.
