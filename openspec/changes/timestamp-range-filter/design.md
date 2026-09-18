## Context

`is_line_visible` is the single visibility gate; `filtered_lines` is rebuilt when filters change. Timestamps are monotonic in the vast majority of logs, which makes go-to-time a binary search over the cache.

## Goals / Non-Goals

**Goals:** range filtering and time jumps without user-supplied formats, cheap detection, correct handling of multi-line entries.

**Non-Goals:** user-defined timestamp formats (later); time zone conversion (times are compared as parsed, naive times treated as local); relative "last 5 minutes" live windows (later).

## Decisions

- **Fixed-format parser, first 64 bytes**, trying formats in order of frequency; the first successful parse wins and the format id is remembered per stream to try it first next time.
- **Cache as `Vec<i64>` millis, `i64::MIN` for none**, 8 bytes per line, filled on append and lazily for existing lines when the range filter or time jump is first used; reset on truncation. Continuation lines inherit the previous timestamp so stack traces stay inside the range.
- **Range inputs accept `HH:MM[:SS]` (today), `YYYY-MM-DD HH:MM[:SS]`, or a full ISO string**; an empty side is open-ended.
- **Go-to-time uses `partition_point`** over the cache when timestamps are non-decreasing; if a stream is detected as unordered (more than 1% inversions in the sample), it falls back to a linear scan and says so.

## Risks / Trade-offs

- [Formats not covered] → the range controls are disabled with a hint when fewer than 50% of sampled lines parse.
- [8 bytes per line] → 100 MB for 12 million lines, only when the feature is used.
