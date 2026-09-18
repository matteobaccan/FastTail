# Log Intelligence Specification

## Purpose
Adds structure awareness to raw log lines: inline JSON detection with expandable pretty-printing and grouping of multiline stack traces with their parent entry.
## Requirements
### Requirement: Inline JSON auto-detection and expansion
The application SHALL detect single-line JSON log payloads and render an inline interactive expansion button (`[▼ JSON]`), allowing the user to expand the payload into formatted, pretty-printed tree view with syntax coloring.

#### Scenario: Line containing valid JSON payload
- **WHEN** a log line contains a JSON object or array
- **THEN** the viewer displays an expand icon next to the line, leaving the collapsed summary line uncluttered by default.

#### Scenario: User clicks expand JSON
- **WHEN** the user clicks the inline `[▼ JSON]` toggle or presses Enter on a focused JSON line
- **THEN** the line expands inline into indented, syntax-highlighted key-value pairs without breaking the overall scroll position.

### Requirement: Multiline stack trace grouping
The engine SHALL recognize multiline exceptions and stack traces (such as Java, .NET, Python, and Go tracebacks) and associate contiguous trace lines with their parent log entry.

#### Scenario: Filtering logs containing stack traces
- **WHEN** the user applies an include filter matching an error message that has a subsequent multiline stack trace
- **THEN** the entire stack trace is preserved and displayed alongside the matched error header rather than being severed by line-by-line filters.

### Requirement: Log Level Detection
The engine SHALL detect a level for each line among FATAL, ERROR, WARN, INFO, DEBUG, TRACE or unknown, by matching the first level token within the first 96 bytes as a whole word (optionally bracketed or followed by a colon, case-insensitive) or a syslog `<n>` severity. Detection SHALL not allocate and SHALL be cached per line.

#### Scenario: Common layouts
- **WHEN** the lines `2026-09-18 12:00:00 [ERROR] boom`, `WARN  main - slow` and `<3>kernel: oops` are appended
- **THEN** their levels are ERROR, WARN and ERROR.

#### Scenario: Level word in the message body only
- **WHEN** the line is `INFO user typed "error" in the search box`
- **THEN** the level is INFO.

### Requirement: Level Colouring Fallback
Rows with a detected level SHALL be coloured with the theme's level palette when no user highlight rule matches them. The feature SHALL be on by default and switchable in Settings.

#### Scenario: User rule keeps priority
- **WHEN** a user rule colours lines containing `payment` green and an ERROR line contains `payment`
- **THEN** the row is green, not the ERROR colour.

### Requirement: Minimum Level Filter and Counters
The stream bar SHALL offer a minimum-level selector combined with the include/exclude filters, with a toggle for lines of unknown level, and the stream status bar SHALL show per-level counts updated live.

#### Scenario: Filtering to warnings and above
- **WHEN** the user selects `>= WARN` on a stream with 100 INFO, 5 WARN and 2 ERROR lines
- **THEN** 7 rows are visible and the counters read WARN 5, ERROR 2.

