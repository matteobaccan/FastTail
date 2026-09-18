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
