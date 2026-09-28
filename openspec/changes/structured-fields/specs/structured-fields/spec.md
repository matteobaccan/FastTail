## ADDED Requirements

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
A stream with an active parser SHALL offer a column view, off by default and toggled per stream from the stream bar in the Text view; the text view SHALL remain the default for every stream. In the column view each shown field SHALL be a column after the marker, line-number and time delta columns, followed by a message column holding the `msg` or `message` field when there is one and otherwise the part of the line not taken by the shown fields. By default the first 8 fields found in the detection sample SHALL be shown. The user SHALL be able to show and hide fields, reorder columns by dragging their header or from the header menu, change a column's width, and reset the layout. The fields offered SHALL be those seen in the detection sample and in the rows drawn since, at most 256 per stream. A line that does not parse, and a stack-trace continuation line, SHALL be drawn across the field columns, dimmed when it does not parse. In wrap mode only the last column SHALL wrap. Only the rows drawn SHALL be parsed, with at most 1,024 parsed rows kept per stream. The view state SHALL be saved with the stream as `fields_view=true` (only when on), `fields_columns` (the shown fields in order) and `fields_width.<field>`.

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

### Requirement: Field Filter Terms
On a stream with an active parser, an include or exclude term SHALL be a field term when it has the form `key` operator `value` with an operator among `=`, `!=`, `~=`, `>`, `>=`, `<` and `<=`, the key starting with a letter, `_` or `@` and made of letters, digits, `_`, `.`, `@` and `-`; a term wrapped in double quotes SHALL be literal text without the quotes. `=` SHALL match when the field's value equals one of the `|`-separated alternatives, `!=` when it equals none of them, and `~=` when the value contains the text; with the stream's regex toggle on, alternatives and the `~=` text SHALL be regular expressions matched against the value, and the case-sensitive toggle SHALL apply to all three. `>`, `>=`, `<` and `<=` SHALL compare the value and the term as numbers and SHALL fail when either is not a number. A line without the field SHALL fail every field term, so it is not shown by an include field term and not hidden by an exclude field term. Field terms SHALL combine with text terms, the level and time filters and the global filter exactly as text terms do, and SHALL be evaluated in the background above 16 MB like any filter. A global field term SHALL be evaluated with each stream's own parser. On a stream without an active parser every term SHALL be text, as without this feature. The term rows SHALL mark field terms so that they can be told from text terms.

#### Scenario: Server errors only
- **WHEN** a JSON stream has the include term `http.status>=500`
- **THEN** only lines whose `http.status` is a number of 500 or more are shown, and a line with `"duration_ms":503` but `"http":{"status":200}` is hidden.

#### Scenario: Alternatives and exclusion
- **WHEN** a logfmt stream has the include term `level=error|fatal` and the exclude term `user~=bot`
- **THEN** only error and fatal lines are shown, except those whose `user` contains `bot`.

#### Scenario: Missing field on the exclude side
- **WHEN** the exclude term is `env=test` and a line has no `env` field
- **THEN** that line is not hidden by the term.

#### Scenario: Literal text on request
- **WHEN** the include term is `"level=error"` on a JSON stream
- **THEN** only lines containing the text `level=error` are shown.

#### Scenario: Background field filter
- **WHEN** the user types `status>=500` on a 3 GB logfmt stream
- **THEN** the interface stays responsive, the stream bar shows the filter progress, and the result equals the synchronous evaluation of the same term.

### Requirement: Level and Timestamp from Fields
On a stream with an active parser, the level of a line SHALL be taken from its first field among `level`, `lvl`, `severity` and `log.level` when its value names a level, and otherwise detected from the text as without this feature; the timestamp of a line SHALL be taken from its first field among `ts`, `time`, `timestamp`, `@timestamp` and `t` when its value is a timestamp the line parser recognises (ISO 8601, epoch seconds or milliseconds), and otherwise detected from the text. The level colouring, the minimum level filter and counters, the time range, the timeline histogram, go to time and the time delta column SHALL use these values, with the same results on the synchronous and the background paths. Changing the parser SHALL recompute the levels and timestamps without rebuilding the line index.

#### Scenario: Time range on a JSON log
- **WHEN** a JSON stream's lines carry the time only in `"ts":"2026-09-28T14:02:05.120Z"` and the user sets the time range from `14:02` to `14:05`
- **THEN** only lines whose `ts` falls in that window are shown.

#### Scenario: Level from a field
- **WHEN** a JSON line reads `{"msg":"disk almost full","severity":"WARNING"}`
- **THEN** the line is counted and coloured as WARN and passes the `>= WARN` filter.

### Requirement: Search, Rules, Copy and Export with Fields
Search, highlight rules, quick labels, bookmarks, copy and "Export visible lines..." SHALL keep working on the text of the line whether the column view is on or off. In the column view, search tints and rule or label spans SHALL be drawn in the cells their bytes fall in, split at cell boundaries, and whole-row rule styles SHALL colour the row. "Copy as shown" in the column view SHALL copy one line per selected row with the shown cells separated by tab characters.

#### Scenario: Search in the column view
- **WHEN** the column view shows `level`, `status` and `msg` and the user searches `timeout`
- **THEN** the rows whose line contains `timeout` are marked and the word is tinted in the `msg` cell.

#### Scenario: Copy as shown
- **WHEN** the column view shows `level`, `status` and `msg` and the user copies a row as shown
- **THEN** the clipboard holds `error`, `502` and `payment failed` separated by tab characters.

#### Scenario: Export keeps the raw line
- **WHEN** the column view is on and the user exports visible lines
- **THEN** the file contains the original lines of the log, not the columns.
