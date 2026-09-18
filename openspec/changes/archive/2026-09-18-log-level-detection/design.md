## Context

`is_line_visible` evaluates exclude then include; `match_highlight` walks user rules top-down. The renderer asks both per visible row. Detection must be cheap enough to run on every appended line at high throughput.

## Goals / Non-Goals

**Goals:** correct level for the common layouts without configuration, zero-allocation detection, filter by minimum level, colour rows without user rules.

**Non-Goals:** user-defined level patterns (a later change), parsing structured JSON levels (the JSON expander is separate), per-level sounds.

## Decisions

- **Detection = first level token in the first 96 bytes** of the line, matched case-insensitively against a fixed table (`FATAL|CRITICAL|ERROR|SEVERE|WARN|WARNING|INFO|NOTICE|DEBUG|TRACE|VERBOSE`) as a whole word, optionally in brackets or followed by a colon. Numeric syslog severities inside `<n>` are mapped too. This covers log4j, logback, Serilog, Python logging, Go zap console, nginx and syslog.
- **Cache one byte per line** (`Vec<u8>`, `0 = unknown`), filled lazily on first access and on append, so a 12 million line file costs 12 MB. Truncation resets it with the line index.
- **Minimum-level filter is a third stage** in `is_line_visible`, after exclude and include; unknown-level lines pass when the threshold is `TRACE`/off and are hidden otherwise, with a toggle "show unknown levels".
- **Colouring is a fallback**, applied in `match_highlight` only when no user rule matched, so existing configurations look the same until the user enables nothing new. The palette lives in the theme so Light stays readable.
- **Counters** are maintained incrementally on append (per-level `u64`), reset on truncation.

## Risks / Trade-offs

- [False positives on lines that mention "error" in prose] → whole-word and first-96-bytes rules keep it to the header; users can disable colouring.
- [Memory for the cache] → 1 byte/line, negligible next to the 8 bytes/line offsets.
