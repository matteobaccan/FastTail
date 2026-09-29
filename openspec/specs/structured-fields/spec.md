# structured-fields Specification

## Purpose
TBD - created by archiving change structured-fields. Update Purpose after archive.
## Requirements
### Requirement: Field Parsers and Detection
Each text stream SHALL have a field parser among off, JSON, logfmt, regex (a user pattern with named groups) and the regex presets Apache / nginx combined and RFC 3164 syslog. The JSON parser SHALL read one object per line, flattening nested objects into dotted keys up to depth 3 and keeping deeper objects and arrays as one raw value; the logfmt parser SHALL read space-separated `key=value` pairs with double-quoted values and backslash escapes, the text before the first pair becoming the `_prefix` field; the regex parser SHALL take each named group as a field. Unless the user has chosen a parser for the stream, the parser SHALL be detected once from the first 200 lines that are not stack-trace continuation lines, reading at most 256 KB: JSON when at least 80 % of them hold a JSON object that parses, optionally after a leading timestamp; otherwise logfmt when at least 80 % hold 3 or more pairs; otherwise off. A stream opened with fewer than 200 such lines SHALL be detected again once when it first holds 200. The regex presets SHALL NOT be detected automatically. The stream bar SHALL show the active parser and offer a menu to change it, including off and a pattern field for the regex parser that flags a pattern that does not compile or has no named group. The choice SHALL be saved with the stream in the workspace and in session files as `fields_parser` (`off`, `json`, `logfmt`, `regex`, `apache`, `syslog`; no key meaning detection) and, for the regex parser, `fields_regex`. Parsing a line SHALL NOT allocate memory per line once the parser has processed its first lines, and no parsed field SHALL be kept per line.

#### Scenario: JSON detected
- **WHEN** the user opens a log whose lines read `{"ts":"2026-09-28T14:02:05.120Z","level":"warn","msg":"slow","http":{"status":503}}`
- **THEN** the stream bar shows the JSON parser and the fields `ts`, `level`, `msg` and `http.status` are known for the stream.

#### Scenario: logfmt detected
- **WHEN** the lines read `ts=2026-09-28T14:02:05Z level=error msg="payment failed" status=502 user=bob`
- **THEN** the stream bar shows the logfmt parser and `msg` has the value `payment failed`.

#### Scenario: Plain text stays plain
- **WHEN** fewer than 80 % of the first 200 lines are JSON objects or logfmt pairs
- **THEN** no parser is active and the stream behaves exactly as without this feature.

#### Scenario: Forcing a regex
- **WHEN** the user picks the regex parser with the pattern `^(?P<ip>\S+) \S+ \S+ \[(?P<time>[^\]]+)\] "(?P<req>[^"]*)" (?P<status>\d{3})`
- **THEN** the fields `ip`, `time`, `req` and `status` are available and the choice is restored after a restart.

### Requirement: Column View
A stream with an active parser SHALL offer a column view, off by default and toggled per stream from the stream bar in the Text view; the text view SHALL remain the default for every stream. In the column view each shown field SHALL be a column after the marker, line-number and time delta columns, followed by a message column holding the `msg` or `message` field when there is one and otherwise the part of the line not taken by the shown fields. By default the first 8 fields found in the detection sample SHALL be shown. The user SHALL be able to show and hide fields, reorder columns by dragging their header or from the header menu, change a column's width, and reset the layout. The fields offered SHALL be those seen in the detection sample and in the rows drawn since, at most 256 per stream. A line that does not parse, and a stack-trace continuation line, SHALL be drawn across the field columns, dimmed when it does not parse. In wrap mode the column view SHALL keep one row per line. Only the rows drawn SHALL be parsed, with at most 1,024 parsed rows kept per stream. The view state SHALL be saved with the stream as `fields_view=true` (only when on), `fields_columns` (the shown fields in order) and `fields_width.<field>`.

#### Scenario: Lining up a field
- **WHEN** the user turns on the column view of a JSON stream and shows `level`, `http.status` and `msg`
- **THEN** each row shows its level, status and message in three aligned columns, and a row without `http.status` shows an empty cell there.

#### Scenario: Reordering and hiding
- **WHEN** the user drags the `msg` header before `level` and hides `http.status`, then restarts FastTail
- **THEN** the stream reopens in the column view with the columns `msg` and `level` in that order.

#### Scenario: A stack trace in column view
- **WHEN** a JSON line is followed by 12 `at …` continuation lines
- **THEN** the 12 lines are drawn across the columns under their entry and stay attached to it when filtered.

#### Scenario: Scrolling a large file
- **WHEN** the user scrolls a 5 GB JSON log in the column view
- **THEN** only the rows drawn are parsed and the memory of the stream does not grow with the number of lines scrolled past.

