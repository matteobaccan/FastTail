## ADDED Requirements

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
