# Selection and Export Specification

## Purpose
Defines row selection in the stream view, copying selected rows to the clipboard, and exporting the visible rows or the search matches to a text file.
## Requirements
### Requirement: Row Selection in the Stream View
Each stream SHALL keep its own set of selected rows. A click selects a row, Shift+click extends the selection over the visible rows between the anchor and the clicked row, Ctrl+click toggles one row, and Ctrl+A selects every visible row of the focused stream. Selected rows SHALL be tinted distinctly from search and highlight tints, and the selection SHALL be cleared when the file is truncated or reopened.

#### Scenario: Range selection under an active filter
- **WHEN** an include filter shows rows 10, 42 and 97 and the user clicks row 10 then Shift+clicks row 97
- **THEN** rows 10, 42 and 97 are selected and the hidden rows in between are not.

#### Scenario: Truncation clears the selection
- **WHEN** rows are selected and the writer truncates the file
- **THEN** the selection is empty and no stale index remains.

### Requirement: Copy Selected Rows to the Clipboard
Ctrl+C in the focused stream SHALL copy the selected rows as plain text in file order, one line per row, without line numbers or markers. With no selection it SHALL copy the row under the cursor of the current search match, if any.

#### Scenario: Copying three rows
- **WHEN** rows 3, 1 and 2 were selected in that order and the user presses Ctrl+C
- **THEN** the clipboard holds the text of rows 1, 2 and 3 separated by newlines.

### Requirement: Export Visible Lines and Search Matches
The stream menu SHALL offer "Export visible lines..." and "Export search matches...". Each SHALL open the native save dialog and write the corresponding rows as plain text, streaming to disk without holding the whole output in memory. The exported content SHALL be the line text regardless of the active view mode, without its ANSI escape sequences when the stream's ANSI mode is render or strip (see the ansi-escape-codes capability) and as stored otherwise. "Export search matches..." SHALL write the stored hits, that is at most the first 1,000,000 matching lines when the search counted more.

#### Scenario: Exporting a filtered stream
- **WHEN** an exclude filter hides half of a 2 million line file and the user exports visible lines
- **THEN** the file contains exactly the rows that pass the filter, in order.

#### Scenario: Exporting from HEX view
- **WHEN** the stream is in HEX view and the user exports search matches
- **THEN** the file contains the text of the matching lines, not a hex dump.

#### Scenario: Exporting a coloured stream
- **WHEN** a stream in ANSI render mode shows `ESC[31mERRORESC[0m payment failed` and the user exports visible lines
- **THEN** the exported line reads `ERROR payment failed` without any escape byte.

#### Scenario: Exporting more matches than are stored
- **WHEN** a search counts 3,412,009 matching lines and the user exports search matches
- **THEN** the file contains the first 1,000,000 matching lines, in order.

### Requirement: Selection and Export of Collapsed Rows
While a stream's collapse mode is on, selecting a collapsed row SHALL select every line of its group, and CTRL + A SHALL select every visible line including the hidden ones. Copy (CTRL + C), "Export visible lines..." and the elapsed time of the selection SHALL use every underlying line in file order, never the `×N` badge, so the collapse mode never changes what is copied or exported. The row context menu SHALL also offer "Copy as shown", which copies one line per selected row, a collapsed row written as its first entry followed by ` ×N`.

#### Scenario: Copying a collapsed row
- **WHEN** a collapsed row stands for 3 lines `retry 1`, `retry 2` and `retry 3` in Numbers mode and the user selects it and presses CTRL + C
- **THEN** the clipboard holds the three lines separated by newlines.

#### Scenario: Copying as shown
- **WHEN** the same row is selected and the user picks "Copy as shown"
- **THEN** the clipboard holds the single line `retry 1 ×3`.

#### Scenario: Exporting a collapsed view
- **WHEN** mode is Exact, a filter leaves 10,000 visible lines shown as 800 rows and the user exports visible lines
- **THEN** the file contains the 10,000 visible lines in order.

### Requirement: Selection and Export with Context Lines
While a stream shows context lines, context rows SHALL be selectable like any row, `CTRL + A` SHALL select every row shown, and copy (`CTRL + C`), "Copy as shown", "Export visible lines..." and the elapsed time of the selection SHALL use the lines shown, match and context lines alike, in file order. The separator between groups SHALL NOT be copied or exported.

#### Scenario: Exporting matches with context
- **WHEN** `N` is 1 and the filter keeps lines 10 and 50 of a file
- **THEN** "Export visible lines..." writes lines 9, 10, 11, 49, 50 and 51 in that order and nothing else.

#### Scenario: Copying across a separator
- **WHEN** the user selects the last row of one group and the first row of the next and presses `CTRL + C`
- **THEN** the clipboard holds the two lines separated by one newline.

### Requirement: Bookmark Report
The stream menu SHALL offer "Bookmark report…" for its stream and the main menu SHALL offer it for every open stream that has bookmarks. The report dialog SHALL let the user choose the number of context lines (0 to 20, default 3), whether automatic bookmarks are included (off by default), a tag filter (none by default, meaning every bookmark), and the order (by stream, or by time across streams); the choices SHALL be saved in `fasttail.ini` `[general]` as `report_context`, `report_auto` and `report_order`. The report SHALL be Markdown: a title with the generation time, the FastTail version, the number of streams and bookmarks, the time span of the bookmarked lines that have a timestamp and the list of tags with their counts; then, per stream in dock order, the tab title and the file path, and per bookmark in line order a heading with the 1-based line number, the timestamp when the line has one, and the note, followed by a fenced code block holding the context lines before, the bookmarked line marked with `>`, and the context lines after, each prefixed by its line number. Overlapping context of neighbouring bookmarks SHALL be merged into one block. A fence SHALL be longer than any backtick run in its block. Lines SHALL be written without ANSI escape sequences when the stream's ANSI mode is render or strip and cut at 2,000 characters with `…`. In the time order, bookmarks SHALL form one list sorted by timestamp, each heading naming its stream, bookmarks without a timestamp last. Automatic bookmarks, when included, SHALL be limited to the first 1,000 per stream, and the report SHALL state how many were left out. The report SHALL be copied to the clipboard when it is at most 4 MB, and SHALL be savable in any case to a `.md` file through the native save dialog as UTF-8. When the report needs more than 20,000 lines read, the lines SHALL be read on a worker thread with progress and a Cancel button, the UI staying responsive.

#### Scenario: Report of one stream
- **WHEN** `app.log` has bookmarks on rows 1,204 (note `retry storm #incident`) and 1,206 (no note), the context is 3, and the user copies the report
- **THEN** the clipboard holds a Markdown report with one section `app.log`, two headings for lines 1,204 and 1,206, and a single code block covering lines 1,201 to 1,209 with lines 1,204 and 1,206 marked `>`.

#### Scenario: Report ordered by time
- **WHEN** `api.log` has a bookmark at `14:02:11` and `db.log` one at `14:01:59`, and the user saves the report of all streams ordered by time
- **THEN** the `.md` file lists the `db.log` bookmark first and the `api.log` bookmark second, each heading naming its stream.

#### Scenario: Filtering by tag
- **WHEN** three bookmarks carry `#deploy` and five do not, and the user picks the tag filter `#deploy`
- **THEN** the report holds exactly the three tagged bookmarks and its tag summary lists `#deploy (3)`.

#### Scenario: A line holding backticks
- **WHEN** a bookmarked line contains three backticks in a row
- **THEN** its code block is fenced with at least four backticks and renders as one block.

#### Scenario: Large report
- **WHEN** the report of all streams exceeds 4 MB
- **THEN** the Copy button is disabled with a tooltip giving the reason and Save writes the whole report.

