## ADDED Requirements

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
