# Search and Navigation Specification

## Purpose
Defines in-stream text search: per-stream queries, match navigation scoped to the focused window, live refresh of matches, and consistent match marking across the Text, Hex and Markdown views.

## Requirements

### Requirement: Per-Stream Search Query
Each open stream SHALL own its search query, match list and current match position. Typing in the search box of one stream SHALL NOT change the query or the matches of any other stream.

#### Scenario: Two streams searched independently
- **WHEN** the user searches `ERROR` in stream A and `timeout` in stream B
- **THEN** stream A keeps `ERROR` with its own matches and stream B keeps `timeout` with its own matches, even when both are visible side by side.

### Requirement: Match Navigation Scoped to the Focused Window
`F3` / `Shift+F3`, `Ctrl+F`, `Enter` / `Shift+Enter` in the search box and the keyboard navigation shortcuts SHALL act only on the stream shown in the focused dock panel. When no panel has been focused yet, the first stream of the main surface is treated as focused.

#### Scenario: F3 with two visible streams
- **WHEN** two streams with active searches are visible side by side and the user presses `F3`
- **THEN** only the stream in the focused panel advances to its next match; the other stream's current match does not move.

### Requirement: Match Counter, Wrap-Around and History
The stream bar SHALL show the current match position and total (`[current / total]`), navigation SHALL wrap around at either end emitting a beep when sound effects are enabled, and the last 10 distinct queries SHALL be kept in a history dropdown persisted in the configuration file. Rescans while typing SHALL be debounced so that each keystroke does not block the interface.

#### Scenario: Wrapping past the last match
- **WHEN** the counter shows `[7 / 7]` and the user presses `F3`
- **THEN** the current match becomes `[1 / 7]`, a beep is played if sound effects are enabled, and the query is stored at the top of the search history.

### Requirement: Live Refresh of Matches
Matches SHALL be recomputed when the file grows, is truncated or rewritten, and when the include/exclude filters change. Only lines that can be displayed under the active filters are searchable. The current match SHALL stay on the same line whenever that line still matches; otherwise it moves to the nearest following match.

#### Scenario: New matching lines appended
- **WHEN** a search for `ERROR` is active and the writer appends two more `ERROR` lines
- **THEN** the counter total grows by two and `F3` reaches the new lines, without the user editing the query.

### Requirement: Match Marker Column and Row Highlight in Every View
While a query is active, every view SHALL show a fixed-width marker column at the left of each row: `▶` on the row of the current match, `●` on rows of other matches, blank elsewhere. Matching rows SHALL be tinted across their full width, with a stronger tint on the current match. The column width SHALL not change when the current match moves.

#### Scenario: Moving the current match
- **WHEN** three rows match and the user presses `F3`
- **THEN** the `▶` marker and the stronger tint move to the next matching row, the previous row keeps `●` and the normal tint, and no row shifts horizontally.

### Requirement: Byte-Level Search in HEX View
In HEX view the query SHALL be matched against the file bytes: as ASCII text ignoring case, and additionally as a byte pattern when the query is an even-length string of hex digits (spaces and colons ignored, e.g. `0A 0D`). A hit spanning two hex rows SHALL mark both rows. `F3` / `Shift+F3` SHALL navigate between byte offsets and the counter SHALL count byte-level hits.

#### Scenario: Text spanning a row boundary
- **WHEN** the word `needle` starts at byte 15 of a stream shown with 16 bytes per row and the user searches `needle`
- **THEN** rows `00000000` and `00000010` are both marked and the counter shows one hit.

### Requirement: Search in Markdown View
In Markdown view an active query SHALL switch the stream to its source lines, marked and highlighted exactly as in Text view, with a notice that the source is being shown. Clearing the query SHALL restore the rendered Markdown document.

#### Scenario: Searching inside a rendered README
- **WHEN** a stream in MD view is showing a rendered README and the user types `install` in the search box
- **THEN** the stream shows the Markdown source lines with matches marked and a notice that the source is displayed; clearing the box returns to the rendered document.

### Requirement: Search Cursor Preserved Across Views
Switching a stream between Text, Hex and Markdown views SHALL keep the query and keep the current match position within the range of the list navigated by the new view.

#### Scenario: Switching from Text to Hex with an active search
- **WHEN** a Text view search shows `[5 / 12]` and the user switches the stream to HEX view
- **THEN** the query stays in the search box, byte-level hits are computed, and the current match index is clamped to the new hit count.

### Requirement: Go To Line
Ctrl+G SHALL open a go-to popup for the focused stream. Entering a 1-based line number and pressing Enter SHALL scroll that line to the middle of the viewport and pause follow mode. Numbers past the end SHALL clamp to the last line. Under active filters the first visible line at or after the target SHALL be used and the popup SHALL say so. The forms `+N` and `-N` SHALL jump relative to the current top line.

#### Scenario: Jumping to a hidden line
- **WHEN** an include filter hides line 500 and the user goes to line 500
- **THEN** the viewport centres on the first visible line after 500 and the popup reports the substitution.

#### Scenario: Number beyond the file
- **WHEN** the file has 1,000 lines and the user enters 5000
- **THEN** the viewport shows line 1,000 and follow mode is paused.

### Requirement: Line Bookmarks
Each stream SHALL keep a set of bookmarked line indices. Ctrl+F2 SHALL toggle a bookmark on the current row, F2 / Shift+F2 SHALL jump to the next / previous bookmark visible under the active filters with wrap-around, and the stream menu SHALL offer "Clear bookmarks". Bookmarked rows SHALL show `★` in the marker column unless the row is a search match, and SHALL be tinted across their width. Bookmarks SHALL be dropped when the file is truncated or rewritten.

#### Scenario: Navigating bookmarks with a filter active
- **WHEN** rows 5, 60 and 900 are bookmarked, an include filter hides row 60, and the user presses F2 from row 5
- **THEN** the view jumps to row 900, and pressing F2 again wraps to row 5.

#### Scenario: Bookmark on a search match
- **WHEN** a bookmarked row is also the current search match
- **THEN** the marker column shows `▶` and the row keeps the bookmark tint.

### Requirement: Bookmark Persistence per File
Bookmarks SHALL be saved in `fasttail.ini` keyed by absolute file path, at most 1,000 per file and 50 files, and restored when the same path is reopened, provided the file still has at least as many lines as the largest saved index.

#### Scenario: Reopening a file
- **WHEN** the user bookmarks rows 10 and 200 in `app.log`, closes FastTail and opens `app.log` again
- **THEN** rows 10 and 200 are bookmarked.

#### Scenario: File rewritten smaller
- **WHEN** saved bookmarks reference row 200 and the reopened file has 50 lines
- **THEN** the saved bookmarks for that file are discarded.
