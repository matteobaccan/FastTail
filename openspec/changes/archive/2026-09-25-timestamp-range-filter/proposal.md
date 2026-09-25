## Why

"What happened between 14:02 and 14:05?" is the most common log question, and Tailviewer answers it with a timestamp range filter. FastTail can only approximate it with a regex on the timestamp text.

## What Changes

- The engine parses a leading timestamp per line from a set of built-in formats (ISO 8601 with `T` or space, with or without zone and milliseconds, syslog `Sep 18 14:02:05`, Apache `[18/Sep/2026:14:02:05 +0200]`, epoch seconds/millis), cached per line; lines without a timestamp inherit the previous one (continuation lines).
- A "from / to" pair in the filter panel restricts visible lines to the range, combined with the other filters; a "Go to time" entry in the go-to popup (Ctrl+G) jumps to the first line at or after a time.
- The stream status bar shows the time span of the visible lines.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `filters-and-highlighting`: adds the timestamp range filter.
- `log-intelligence`: adds timestamp detection.
- `search-and-navigation`: the go-to popup (Ctrl+G) also takes a time.

## Impact

- `src/tail_engine.rs`: `detect_timestamp(&str) -> Option<i64>` (unix millis), per-line cache (`Vec<i64>` filled lazily), `time_range: Option<(i64, i64)>` in `is_line_visible`, binary search for go-to-time.
- `src/ui/dock.rs`: range inputs with validation, span display; `src/i18n.rs`: keys.
- No new dependency: a hand-written parser for the fixed formats keeps it allocation-free.
