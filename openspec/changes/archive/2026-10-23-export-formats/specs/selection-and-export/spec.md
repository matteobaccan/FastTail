## ADDED Requirements

### Requirement: Export Dialog
The stream menu SHALL offer "Export…", opening a dialog to choose what to export (visible lines, selected lines, search matches) and the format (text, CSV, HTML). "Selected lines" SHALL be disabled when the stream has no selection. The existing items "Export visible lines..." and "Export search matches..." SHALL open the same dialog with their scope and the text format preselected, and the text format SHALL write exactly what those items write today. The chosen format and the CSV options SHALL be remembered in `fasttail.ini` `[general]` as `export_format`, `export_csv_separator`, `export_csv_bom` and `export_csv_protect`. Exports of more than 1,000,000 lines and every HTML export SHALL run on a worker thread with progress in the stream bar and a Cancel button, covering the lines present when the export started, and SHALL stop with a message if the file is truncated meanwhile.

#### Scenario: Text export unchanged
- **WHEN** the user picks "Export visible lines..." and confirms the dialog without changing it
- **THEN** the written file is byte-identical to what FastTail 0.12.0 writes for the same stream.

#### Scenario: Exporting the selection
- **WHEN** rows 10, 11 and 40 are selected and the user exports selected lines as text
- **THEN** the file holds those three lines in file order.

### Requirement: CSV Export
The CSV format SHALL follow RFC 4180 with CRLF line ends, UTF-8, a header row and a separator chosen among comma (default), semicolon and tab. A cell SHALL be quoted when it contains the separator, a double quote, CR or LF, a double quote being doubled. On a stream without an active field parser the columns SHALL be `line` (1-based), `time` (the detected timestamp in ISO 8601 with the line's own precision, empty when none), `level` (the detected level, empty when none) and `text` (the line without ANSI escape sequences when the stream's ANSI mode is render or strip). On a stream with an active field parser (see the structured-fields capability) the columns SHALL be `line`, the fields shown in the column view in their order, and `message`; a line that does not parse SHALL have empty fields and its whole text in `message`. A UTF-8 byte order mark SHALL be written unless the user turns it off. With formula protection on (the default), a cell starting with `=`, `+`, `-`, `@`, tab or carriage return SHALL be prefixed with `'`.

#### Scenario: Plain log to CSV
- **WHEN** the visible line 7 reads `2026-09-28 14:02:11.123 ERROR payment failed, card "x"` and the user exports visible lines as CSV with the comma separator
- **THEN** the file holds the row `7,2026-09-28T14:02:11.123,ERROR,"2026-09-28 14:02:11.123 ERROR payment failed, card ""x"""`.

#### Scenario: Structured stream to CSV
- **WHEN** a JSON stream shows the columns `ts`, `level`, `status` in that order and the user exports it as CSV
- **THEN** the header row is `line,ts,level,status,message`.

#### Scenario: Formula in a log line
- **WHEN** a line reads `=HYPERLINK("http://evil")` and formula protection is on
- **THEN** its `text` cell starts with `'=HYPERLINK`.

### Requirement: HTML Export
The HTML format SHALL write one standalone file with no script and no external resource, holding the line numbers and the lines drawn with the current theme's background and text colours and with the styles FastTail shows for them: highlight rules (foreground, background, bold, italic, underline), quick colour labels, level colours, ANSI colours in render mode, and the search hits. Every character of the line text SHALL be HTML-escaped and no ANSI escape byte SHALL be written. At most 200,000 lines SHALL be written; when the scope holds more, the dialog SHALL say so before exporting and the file SHALL hold the first 200,000.

#### Scenario: Colours kept
- **WHEN** a rule paints `ERROR` red on bold and the user exports visible lines as HTML
- **THEN** opening the file in a browser shows the `ERROR` words red and bold on the theme background.

#### Scenario: Markup in a line
- **WHEN** a line contains `<script>alert(1)</script>`
- **THEN** the HTML file shows that text literally and no script runs.

#### Scenario: Too many lines for HTML
- **WHEN** the visible lines number 350,000 and the user chooses HTML
- **THEN** the dialog says that only the first 200,000 lines will be exported, and the file holds lines up to the 200,000th visible one.
