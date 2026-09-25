## ADDED Requirements

### Requirement: Timestamp Range Filter
The filter panel SHALL offer "from" and "to" time inputs, either side optional (an empty side is an open end). Each input SHALL accept a bare `HH:MM` or `HH:MM:SS`, a `YYYY-MM-DD HH:MM[:SS]` (space or `T` separator), or any timestamp the line parser recognises, so a timestamp copied out of a line works. A bare time SHALL belong to the day of the stream's first timestamped line, not to the current day. The "to" side SHALL cover the whole unit typed: a time without seconds covers the whole minute, a time with seconds the whole second. Only lines whose detected timestamp (or the timestamp inherited by a continuation line) lies inside the range SHALL be visible, combined with the include/exclude and level filters; while a window is set, the lines before the first timestamped line cannot be placed in time and SHALL be hidden. An input that cannot be read SHALL be flagged next to the fields, and a clear button SHALL remove the window. The stream status bar SHALL show the time span of the visible lines. The controls SHALL be disabled with a hint when fewer than half of the timed lines carry a recognised timestamp of their own.

#### Scenario: Three-minute window
- **WHEN** the user enters from `14:02` to `14:05` on a log whose first entry is dated 2026-09-18
- **THEN** only lines stamped between 2026-09-18 14:02:00.000 and 14:05:59.999 are visible, including the stack-trace lines that follow an entry in that window.

#### Scenario: Banner lines before the first timestamp
- **WHEN** a log starts with a banner line without a timestamp and the user sets a "from" time
- **THEN** the banner line is hidden, and clearing the window shows it again.

#### Scenario: Open-ended window
- **WHEN** the user enters only from `14:02`
- **THEN** every line stamped at or after 14:02:00.000 is visible, up to the end of the log.

#### Scenario: A log that cannot be timed
- **WHEN** fewer than half of the lines of a stream carry a recognised timestamp
- **THEN** the from/to fields are disabled and a hint says the stream has no usable timestamps, instead of a window hiding the whole file.
