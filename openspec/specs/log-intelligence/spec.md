# Log Intelligence Specification

## Purpose
Adds structure awareness to raw log lines: inline JSON detection with expandable pretty-printing, grouping of multiline stack traces with their parent entry, log level detection with colouring, a minimum-level filter and counters, and leading-timestamp detection used by the time range filter and go-to-time.
## Requirements
### Requirement: Inline JSON auto-detection and expansion
The application SHALL treat a line as a JSON payload when, trimmed, it starts with `{` and ends with `}` or starts with `[` and ends with `]`, and SHALL render an inline toggle (`[+] JSON`, `[-] JSON` once expanded) that expands the payload into an indented, pretty-printed block below the row. A line that only carries JSON after other text (a timestamp, a level) is not detected.

#### Scenario: Line containing valid JSON payload
- **WHEN** a log line is a JSON object or array
- **THEN** the viewer displays a `[+] JSON` toggle next to the line, leaving the collapsed line unchanged by default.

#### Scenario: User clicks expand JSON
- **WHEN** the user clicks the inline `[+] JSON` toggle
- **THEN** the line expands inline into indented, pretty-printed key-value pairs without breaking the overall scroll position, and the toggle reads `[-] JSON` until it is clicked again.

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
Rows with a detected level SHALL be coloured with the theme's level palette when no whole-row user highlight rule matches them, on the bytes that no capture-only rule, quick label or ANSI colour span colours. The feature SHALL be on by default and switchable in Settings.

#### Scenario: User rule keeps priority
- **WHEN** a user rule colours lines containing `payment` green and an ERROR line contains `payment`
- **THEN** the row is green, not the ERROR colour.

### Requirement: Minimum Level Filter and Counters
The stream bar SHALL offer a minimum-level selector combined with the include/exclude filters, with a toggle for lines of unknown level, and the stream status bar SHALL show per-level counts updated live.

#### Scenario: Filtering to warnings and above
- **WHEN** the user selects `>= WARN` on a stream with 100 INFO, 5 WARN and 2 ERROR lines
- **THEN** 7 rows are visible and the counters read WARN 5, ERROR 2.

### Requirement: Timestamp Detection
The engine SHALL detect a leading timestamp per line among ISO 8601 (with `T` or space, optional fraction and zone), syslog (`Mon DD HH:MM:SS`, the current year assumed), Apache/nginx (`[DD/Mon/YYYY:HH:MM:SS zone]`) and epoch seconds (10 digits) or milliseconds (13 digits), within the first 64 bytes, without allocation, trying the format that matched last first, and SHALL cache the result per line. Timestamps SHALL be compared on the clock the log printed: a zone suffix (`Z`, `+02:00`, `+0200`) is read past and not applied, and epoch values are read as UTC. Lines without a timestamp SHALL inherit the previous line's timestamp, so a stack trace stays with its entry. The cache SHALL be reset from the first changed line when the file is truncated or rewritten. The cache is built when the time range or the time jump is first used, as a background scan with progress (see the stream-engine capability) when more than 16 MB remain to be timed; the result SHALL be identical to timing the same lines synchronously. Lines appended afterwards SHALL be timed incrementally.

#### Scenario: Mixed entry with continuation lines
- **WHEN** the lines `2026-09-18T14:02:05.123Z ERROR boom`, `    at Foo.bar(Foo.java:10)` are appended
- **THEN** both lines carry the timestamp 14:02:05.123 of 2026-09-18.

#### Scenario: Zone suffix is not applied
- **WHEN** a line starts with `[18/Sep/2026:14:02:05 +0200]`
- **THEN** its timestamp is 2026-09-18 14:02:05, the clock the log printed, and a range from `14:02` includes it.

#### Scenario: Background and synchronous timing agree
- **WHEN** the same log is timed once below the 16 MB threshold and once, padded above it, by the background scan
- **THEN** the cached timestamps of the common lines, the share of lines with a timestamp of their own and the out-of-order flag are identical.

