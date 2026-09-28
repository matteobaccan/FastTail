## ADDED Requirements

### Requirement: Log Format Files
FastTail SHALL load user log formats from the `formats` folder next to `fasttail.ini`, one format per `*.fasttail-format.ini` file with a `[format]` section, at most 256 formats. A format SHALL have a `name`, one to 8 line patterns (`regex`, `regex.2` … `regex.8`) that are regular expressions with named groups tried in order, and MAY have a `timestamp_field`, a `timestamp_format`, a `timestamp_zone`, a `level_field` with a `level_map`, a `message_field`, an `entry_start` pattern, `files` globs separated by `;`, default `columns` and up to 20 `sample.N` lines. A file whose patterns do not compile, have no named group, or whose field roles name a group that a pattern lacks SHALL be listed in the Log formats dialog with its error and SHALL NOT be used. The Apache / nginx combined and RFC 3164 syslog formats SHALL be built in, read-only, and duplicable as editable files.

#### Scenario: Loading a format file
- **WHEN** `formats/payments.fasttail-format.ini` defines the name `Payments` and a pattern with the groups `ts`, `thread`, `level` and `msg`
- **THEN** the parser menu of every stream lists `Payments`, and choosing it shows those four fields as columns in the column view.

#### Scenario: Invalid format file
- **WHEN** a format file's `regex` has an unbalanced parenthesis
- **THEN** the Log formats dialog lists the file with the regex error and its position, and no stream can use it.

### Requirement: Format Timestamps and Levels
When a format names a timestamp field, the timestamp of a line SHALL be read from that field, with the format's `timestamp_format` when set (the strftime directives `%Y %y %m %d %e %H %I %M %S %p %b %j %z %f %3f %6f %9f %%`, or `epoch_s` / `epoch_ms`) and with `detect_timestamp` otherwise; the time range, the histogram and the time delta column SHALL use it. When a format names a level field, the level of a line SHALL be the `level_map` entry for the field's value, compared without regard to case, or the level the built-in level detection gives that value when it has no entry. A line whose field cannot be read SHALL fall back to the built-in detection on the whole line.

#### Scenario: A timestamp the built-in parser does not read
- **WHEN** a format has `timestamp_format=%d.%m.%y %H:%M:%S,%3f` and a line starts with `28.09.26 14:02:11,123`
- **THEN** the line is placed at 2026-09-28 14:02:11.123 in the time range, the histogram and the time delta column.

#### Scenario: Mapped level
- **WHEN** a format has `level_map=W=warn,E=error` and a line's level field is `E`
- **THEN** the line is coloured and filtered as an ERROR line.

### Requirement: Entry Start Pattern
When the format of a stream has an `entry_start` pattern, a line of that stream SHALL be a continuation of the previous entry exactly when it does not match the pattern, in place of the built-in stack-trace heuristic, for the filters, the timestamp inherited by continuation lines, the level of continuation lines and collapse, on the synchronous and background paths alike. Choosing, changing or clearing the format SHALL rebuild the filter, the timestamp and level caches and the collapse groups of the stream.

#### Scenario: Multi-line entries kept together
- **WHEN** a format has `entry_start=^\d{4}-\d\d-\d\d ` and an entry spans a header line and four lines of an XML body, and the include term matches only the header
- **THEN** the view shows the header and the four body lines, and all five lines carry the header's timestamp.

### Requirement: Format Detection and Association
On a stream whose parser is automatic, detection SHALL try, in order, the formats whose `files` globs match the file name, every other user format, and then the built-in JSON and logfmt detection. A user format SHALL be chosen when at least 80 % of the entries of the 200-line sample match one of its patterns; among such formats the highest ratio SHALL win, then a glob-associated format, then the one with the most named groups, then the name in alphabetical order. A stream whose saved parser is `format:<name>` SHALL use that format without detection; when no format has that name it SHALL fall back to automatic detection and say so in the parser chip's tooltip. The chosen format SHALL be shown by name in the parser chip and saved per stream as `fields_parser=format:<name>` in the workspace and in session files.

#### Scenario: Association by file name
- **WHEN** a format has `files=payments-*.log` and the user opens `payments-2026-09-28.log` whose lines match its pattern
- **THEN** the parser chip reads `Payments` without the user choosing it.

#### Scenario: Missing saved format
- **WHEN** a session saved a stream with `fields_parser=format:Payments` and that format file has been deleted
- **THEN** the stream opens with automatic detection and the parser chip's tooltip says the format `Payments` was not found.

### Requirement: Log Formats Dialog
Settings SHALL offer a Log formats dialog listing the built-in and user formats with new, duplicate, delete (after confirmation), import and export actions. Editing a format SHALL be a draft with a live preview, refreshed after typing pauses, of the captures on the format's sample lines and on the first 200 lines of the focused stream, showing the match ratio, the unmatched lines, the timestamp and level each line gets, the entry count, and the average matching time per line; compile errors SHALL be shown with their position. Saving SHALL write the format file and SHALL re-run detection only on streams whose parser is automatic. The row menu SHALL offer "Create format from these lines…", opening a new draft with the selected lines as samples.

#### Scenario: Preview while typing
- **WHEN** the user edits a pattern in the dialog so that 190 of the focused stream's first 200 lines match
- **THEN** the preview shows a 95 % match ratio, lists the 10 unmatched lines, and no stream changes until the format is saved.

#### Scenario: Format from selected lines
- **WHEN** the user selects three rows and picks "Create format from these lines…"
- **THEN** the dialog opens a new format draft whose samples are those three lines.
