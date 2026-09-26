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
`F3` / `Shift+F3`, `Ctrl+F`, `Enter` / `Shift+Enter` in the search box and the keyboard navigation shortcuts SHALL act only on the stream shown in the focused dock panel. When no panel has been focused yet, the first stream of the main surface is treated as focused. A stream's search results pane is part of that stream's panel: clicking in it SHALL focus the panel and give the pane keyboard focus. While the pane has keyboard focus, the arrow keys, `PgUp` / `PgDown` and `Ctrl+Home` / `Ctrl+End` SHALL move the pane selection, `Enter` SHALL commit it and `Esc` SHALL return keyboard focus to the main view; `F3` / `Shift+F3`, `Ctrl+F`, `Ctrl+G`, the bookmark keys and the other stream shortcuts SHALL keep acting on the stream.

#### Scenario: F3 with two visible streams
- **WHEN** two streams with active searches are visible side by side and the user presses `F3`
- **THEN** only the stream in the focused panel advances to its next match; the other stream's current match does not move.

#### Scenario: F3 while the pane has focus
- **WHEN** the user has clicked a row in the results pane of stream A and presses `F3`
- **THEN** stream A advances to its next match in the main view and the pane selection moves with it; no other stream reacts.

#### Scenario: Leaving the pane
- **WHEN** the pane has keyboard focus and the user presses `Esc`, then `PgDown`
- **THEN** the main view scrolls by one page and the pane selection does not move.

### Requirement: Match Counter, Wrap-Around and History
The stream bar SHALL show the current match position and total (`[current / total]`), navigation SHALL wrap around at either end emitting a beep when sound effects are enabled, and the last 10 distinct queries SHALL be kept in a history dropdown persisted in the configuration file. Rescans while typing SHALL be debounced so that each keystroke does not block the interface. A stream SHALL store at most 1,000,000 line hits; past that the search SHALL keep counting without storing, the total SHALL be the true number of matching lines, the counter and the results pane SHALL say that only the first 1,000,000 are listed, and navigation SHALL wrap within the stored hits. Byte-level hits in HEX view SHALL be capped at 20,000.

#### Scenario: Wrapping past the last match
- **WHEN** the counter shows `[7 / 7]` and the user presses `F3`
- **THEN** the current match becomes `[1 / 7]`, a beep is played if sound effects are enabled, and the query is stored at the top of the search history.

#### Scenario: More matches than the cap
- **WHEN** a search for `INFO` matches 3,412,009 lines of a large log
- **THEN** the counter's total reads 3,412,009 with a note that the first 1,000,000 are listed, and `F3` from the 1,000,000th hit wraps to the first.

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
Ctrl+G SHALL open a go-to popup for the focused stream. Entering a 1-based line number and pressing Enter SHALL scroll that line to the middle of the viewport and pause follow mode. Numbers past the end SHALL clamp to the last line. Under active filters the first visible line at or after the target SHALL be used and the popup SHALL say so. The forms `+N` and `-N` SHALL jump relative to the current top line. An input containing `:` SHALL be read as a time instead, in the forms the timestamp range filter accepts (a bare `HH:MM[:SS]` belonging to the day of the stream's first timestamped line, a `YYYY-MM-DD HH:MM[:SS]`, or a timestamp copied out of a line), and SHALL jump to the first line whose timestamp is at or after it. When the stream has not been fully timed yet, the timestamp cache SHALL be built first, in the background when more than 16 MB remain to be timed: the popup SHALL show the timing progress and the jump SHALL happen when the cache is complete. Closing the popup or entering another target SHALL cancel a jump that is still waiting, without stopping the timing. The search SHALL be a binary search when the timestamps never go back in time and a linear scan otherwise. Anything else, including a bare epoch number, is a line number.

#### Scenario: Jumping to a hidden line
- **WHEN** an include filter hides line 500 and the user goes to line 500
- **THEN** the viewport centres on the first visible line after 500 and the popup reports the substitution.

#### Scenario: Number beyond the file
- **WHEN** the file has 1,000 lines and the user enters 5000
- **THEN** the viewport shows line 1,000 and follow mode is paused.

#### Scenario: Jump to a time on a freshly opened log
- **WHEN** the user opens an ordered log, presses Ctrl+G without touching the time range and enters `14:03:30`
- **THEN** the viewport centres on the first line stamped at or after 14:03:30 on the day of the log's first entry.

#### Scenario: Time past the end of the log
- **WHEN** the user enters a time later than every timestamp in the stream
- **THEN** the popup reports that the input cannot be resolved and the viewport does not move.

#### Scenario: Jump to a time on a large untimed log
- **WHEN** the user enters `09:15` in the Ctrl+G popup of a 4 GB stream that has never been timed
- **THEN** the interface stays responsive, the popup shows the timing progress, and when timing completes the viewport centres on the first line stamped at or after 09:15.

#### Scenario: Waiting jump cancelled
- **WHEN** a time jump is waiting for timing and the user presses Esc
- **THEN** the popup closes, the viewport does not move when timing completes, and the timing itself continues.

### Requirement: Line Bookmarks
Each stream SHALL keep a set of bookmarked line indices. Ctrl+F2 SHALL toggle a bookmark on the current row, F2 / Shift+F2 SHALL jump to the next / previous bookmark visible under the active filters with wrap-around, and the stream menu SHALL offer "Clear bookmarks". Bookmarked rows SHALL show `★` in the marker column unless the row is a search match, and SHALL be tinted across their width. Bookmarks SHALL be dropped when the file is truncated or rewritten.

#### Scenario: Navigating bookmarks with a filter active
- **WHEN** rows 5, 60 and 900 are bookmarked, an include filter hides row 60, and the user presses F2 from row 5
- **THEN** the view jumps to row 900, and pressing F2 again wraps to row 5.

#### Scenario: Bookmark on a search match
- **WHEN** a bookmarked row is also the current search match
- **THEN** the marker column shows `▶` and the row keeps the bookmark tint.

### Requirement: Bookmark Persistence per File
Bookmarks SHALL be saved in `fasttail.ini` keyed by absolute file path (the entry path `<archive>/<entry>` for a zip entry), at most 1,000 per file and 50 files, and restored when the same path is reopened, provided the file still has at least as many lines as the largest saved index; a restored compressed stream applies them once its index covers them.

#### Scenario: Reopening a file
- **WHEN** the user bookmarks rows 10 and 200 in `app.log`, closes FastTail and opens `app.log` again
- **THEN** rows 10 and 200 are bookmarked.

#### Scenario: File rewritten smaller
- **WHEN** saved bookmarks reference row 200 and the reopened file has 50 lines
- **THEN** the saved bookmarks for that file are discarded.

### Requirement: Search Results Pane
A stream with an active query SHALL be able to show a results pane below its rows, toggled from the stream bar, listing only the matching lines in file order with their 1-based line number and text, the query tinted and the level colour applied, under a header naming the query, the total, the capped note and the progress of a running search. The pane SHALL be virtualized: only the rows on screen are read, whatever the number of hits, and hits found by a running search or on appended lines SHALL appear as they are found. Clicking a row, or selecting it with the arrow keys and pressing `Enter`, SHALL make that hit the current match, centre it in the main view and pause follow mode. The current match SHALL be marked `▶` in the pane, and the pane SHALL scroll to keep it visible when it changes by `F3`, `Shift+F3` or a refresh. The pane's open state and height SHALL be one preference shared by every stream (the pane shows in each stream with an active query), persisted in `fasttail.ini` as `search_pane` and `search_pane_height`. In HEX view the pane SHALL show a notice instead of rows.

#### Scenario: Jumping from the pane
- **WHEN** a search for `timeout` on a 2 GB log lists 340 hits in the pane and the user clicks the hit on line 1,204,881
- **THEN** the main view centres line 1,204,881, follow mode is paused, the counter shows that hit's position, and the pane row carries `▶`.

#### Scenario: Pane follows F3
- **WHEN** the pane is open and the user presses `F3` three times
- **THEN** the current match advances three hits in the main view, and the pane scrolls so that the `▶` row stays visible.

#### Scenario: Keyboard selection without moving the view
- **WHEN** the pane has keyboard focus and the user presses `↓` five times
- **THEN** the pane selection moves five rows while the main view stays where it was, and pressing `Enter` then centres the selected hit in the main view.

### Requirement: Overview Strip
The Text view of a stream SHALL show, beside its vertical scrollbar, an overview strip marking the proportional position among the visible rows of the search hits, the bookmarks, the ERROR and FATAL lines and the current match, with the viewport drawn as a box. Clicking or dragging on the strip SHALL scroll the main view to that position and pause follow mode. Hovering the strip SHALL show the line number under the pointer. Hit marks SHALL be exact for the stored hits (the first 1,000,000) and bookmark marks SHALL be exact. Error marks cover the lines whose level has already been detected; they SHALL be exact without a filter and on filtered views of up to 4,000,000 visible rows; above that they MAY be sampled, and the strip's tooltip SHALL say so. The strip SHALL be recomputed only when the rows, the hits, the bookmarks, the level cache or the filters change, not on every frame; while the stream grows it SHALL be rebuilt at most four times a second, and a change of its height or of the bookmarks SHALL rebuild it at once. The strip SHALL be hidden in HEX and rendered Markdown views and when there is nothing to mark, and SHALL be switchable off in Settings (on by default).

#### Scenario: Seeing where errors cluster
- **WHEN** a 1 GB log without filters has ERROR lines only around its middle and near its end
- **THEN** the strip shows error marks at those two heights, and clicking the lower cluster scrolls the main view there.

#### Scenario: Large filtered view
- **WHEN** an exclude filter leaves 30 million visible rows and the user hovers the strip
- **THEN** search hit and bookmark marks are exact, and the tooltip says that error marks are sampled.

### Requirement: Search All Streams
The user SHALL be able to run one query across every open stream. The match SHALL be the same case-insensitive text match as the per-stream search, over the lines each stream shows under its own include/exclude, level and time filters; streams in HEX view SHALL be skipped and reported as skipped. Each stream SHALL be searched by its own background job, independent of the stream's own filter and search scans, with at most four jobs (fewer on machines with fewer cores) running at once and the others queued. A new query, a Stop button and closing the results SHALL cancel every job; closing a stream SHALL cancel its job and remove its results. At most 100,000 hits SHALL be stored per stream, with the true total still counted. The results SHALL be a snapshot of each stream at the moment its job started, with a Refresh action to run the query again.

#### Scenario: A request id across three logs
- **WHEN** `gateway.log`, `payment.log` and `audit.log` are open and the user searches all streams for `req-7f3a`
- **THEN** each stream is searched in the background, and the results report 4 matches in `gateway.log`, 2 in `payment.log` and none in `audit.log`.

#### Scenario: Filters of each stream are honoured
- **WHEN** `payment.log` has an exclude filter `healthcheck` and one line contains both `req-7f3a` and `healthcheck`
- **THEN** that line is not among the results, as it would not be for the stream's own search.

#### Scenario: Bounded concurrency
- **WHEN** ten multi-GB streams are open and the user runs a query on an eight-core machine
- **THEN** at most four searches run at the same time, the others show as queued, and the interface stays responsive.

#### Scenario: Cancelling
- **WHEN** a search across streams is running and the user presses Stop
- **THEN** every running and queued job stops, and the results found so far stay listed.

### Requirement: Find Results Tab
The results of a search across streams SHALL be shown in a single "Find results" dock tab, opened or focused by `Ctrl+Shift+F` with the focused stream's query prefilled, and not saved in the dock layout. Results SHALL be grouped by stream, each group headed by the stream name, its match count, its progress while running and a note when only the first 100,000 hits are listed; groups SHALL be collapsible, and the whole list SHALL be virtualized. Clicking a result, or selecting it with the keyboard and pressing `Enter`, SHALL activate that stream's tab, focus its panel, centre the line (or the next visible line when the stream's filters now hide it) and pause follow mode, without changing the stream's own search query. When a stream has been reloaded, truncated, rewritten or switched to another file since its results were found, its group SHALL be marked stale and its results SHALL NOT jump.

#### Scenario: Jumping to a result
- **WHEN** the Find results tab lists a match on line 88,120 of `payment.log`, which is a background tab, and the user clicks it
- **THEN** the `payment.log` tab becomes active and focused, line 88,120 is centred, follow is paused, and the search box of `payment.log` keeps its previous query.

#### Scenario: Stream rewritten after the search
- **WHEN** `gateway.log` is truncated by its writer after the search and the user clicks one of its results
- **THEN** the view does not move and the group says the stream changed and offers Refresh.

